use limine::response::MemoryMapResponse;
use x86_64::structures::paging::{FrameAllocator, FrameDeallocator, PhysFrame, Size4KiB};
use x86_64::PhysAddr;
use spin::Mutex;
use limine::memory_map::EntryType;

const PAGE_SIZE: u64 = 4096;

pub struct BitmapAllocator {
    bitmap: Option<&'static mut [u8]>,
    total_frames: usize,
    free_frames: usize,
    last_alloc_idx: usize,
    bitmap_phys_addr: u64, // Used for testing self-reservation
}

pub static mut DMA_POOL_PHYS: u64 = 0;
pub static mut DMA_POOL_USED: u64 = 0;

pub fn allocate_dma_frames(bytes: u64) -> Option<PhysAddr> {
    unsafe {
        let aligned = (bytes + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
        if DMA_POOL_USED + aligned <= 65536 {
            let addr = DMA_POOL_PHYS + DMA_POOL_USED;
            DMA_POOL_USED += aligned;
            
            // Zero the DMA memory via HHDM
            let hhdm_offset = super::hhdm_offset();
            core::ptr::write_bytes((addr + hhdm_offset) as *mut u8, 0, aligned as usize);
            
            Some(PhysAddr::new(addr))
        } else {
            None
        }
    }
}


impl BitmapAllocator {
    pub const fn new() -> Self {
        Self {
            bitmap: None,
            total_frames: 0,
            free_frames: 0,
            last_alloc_idx: 0,
            bitmap_phys_addr: 0,
        }
    }

    /// Mark a frame as used (1) or free (0)
    fn set_bit(&mut self, frame_idx: usize, used: bool) {
        if let Some(bitmap) = self.bitmap.as_mut() {
            let byte_idx = frame_idx / 8;
            let bit_idx = frame_idx % 8;
            if used {
                bitmap[byte_idx] |= 1 << bit_idx;
            } else {
                bitmap[byte_idx] &= !(1 << bit_idx);
            }
        }
    }

    fn get_bit(&self, frame_idx: usize) -> bool {
        if let Some(bitmap) = self.bitmap.as_ref() {
            let byte_idx = frame_idx / 8;
            let bit_idx = frame_idx % 8;
            (bitmap[byte_idx] & (1 << bit_idx)) != 0
        } else {
            true // If no bitmap, pretend everything is used
        }
    }
}

unsafe impl FrameAllocator<Size4KiB> for BitmapAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame> {
        if self.free_frames == 0 || self.bitmap.is_none() {
            return None;
        }

        let total = self.total_frames;
        for i in 0..total {
            let idx = (self.last_alloc_idx + i) % total;
            if !self.get_bit(idx) {
                // Found a free frame
                self.set_bit(idx, true);
                self.free_frames -= 1;
                self.last_alloc_idx = idx + 1;
                let phys_addr = PhysAddr::new((idx as u64) * PAGE_SIZE);
                return Some(PhysFrame::containing_address(phys_addr));
            }
        }
        None
    }
}

impl FrameDeallocator<Size4KiB> for BitmapAllocator {
    unsafe fn deallocate_frame(&mut self, frame: PhysFrame) {
        let idx = (frame.start_address().as_u64() / PAGE_SIZE) as usize;
        if idx < self.total_frames && self.get_bit(idx) {
            self.set_bit(idx, false);
            self.free_frames += 1;
        }
    }
}

pub static ALLOCATOR: Mutex<BitmapAllocator> = Mutex::new(BitmapAllocator::new());

pub fn init(memmap: &MemoryMapResponse) {
    let entries = memmap.entries();
    
    // 1. Find max physical address (only considering RAM types) to prevent excessive bitmap sizes
    let mut max_phys_addr = 0;
    for entry in entries.iter() {
        let is_ram = matches!(
            entry.entry_type,
            EntryType::USABLE
                | EntryType::BOOTLOADER_RECLAIMABLE
                | EntryType::EXECUTABLE_AND_MODULES
                | EntryType::ACPI_RECLAIMABLE
                | EntryType::ACPI_NVS
        );

        if is_ram {
            let end = entry.base + entry.length;
            if end > max_phys_addr {
                max_phys_addr = end;
            }
        }
    }

    let total_frames = (max_phys_addr / PAGE_SIZE) as usize;
    let bitmap_size_bytes = (total_frames + 7) / 8;

    // 2. Find a usable region to store the bitmap itself
    let mut bitmap_phys_addr = 0;
    for entry in entries.iter() {
        if entry.entry_type == limine::memory_map::EntryType::USABLE && entry.length >= bitmap_size_bytes as u64 {
            bitmap_phys_addr = entry.base;
            break;
        }
    }

    if bitmap_phys_addr == 0 {
        panic!("Failed to find a memory region large enough for the bitmap ({} bytes)", bitmap_size_bytes);
    }

    let hhdm_offset = super::hhdm_offset();
    let bitmap_virt_addr = hhdm_offset + bitmap_phys_addr;
    
    // SAFETY: We found a USABLE region large enough, and HHDM maps it here.
    let bitmap_slice = unsafe {
        core::slice::from_raw_parts_mut(bitmap_virt_addr as *mut u8, bitmap_size_bytes)
    };
    
    // Initialize all to 1 (used)
    bitmap_slice.fill(0xFF);

    let mut allocator = ALLOCATOR.lock();
    allocator.bitmap = Some(bitmap_slice);
    allocator.total_frames = total_frames;
    allocator.free_frames = 0;
    allocator.bitmap_phys_addr = bitmap_phys_addr;

    // 3. Mark usable regions as free
    for entry in entries.iter() {
        if entry.entry_type == limine::memory_map::EntryType::USABLE {
            let start_frame = (entry.base / PAGE_SIZE) as usize;
            let end_frame = ((entry.base + entry.length) / PAGE_SIZE) as usize;
            
            for i in start_frame..end_frame {
                allocator.set_bit(i, false);
                allocator.free_frames += 1;
            }
        }
    }

    // 4. Re-mark the bitmap itself as used!
    let bitmap_start_frame = (bitmap_phys_addr / PAGE_SIZE) as usize;
    let bitmap_end_frame = ((bitmap_phys_addr + bitmap_size_bytes as u64 + PAGE_SIZE - 1) / PAGE_SIZE) as usize;
    
    for i in bitmap_start_frame..bitmap_end_frame {
        if !allocator.get_bit(i) {
            allocator.set_bit(i, true);
            allocator.free_frames -= 1;
        }
    }

    log::info!("[MEMORY] frame allocator: Bitmap");
    log::info!("[MEMORY] bitmap stored at physical {:#x} ({} bytes)", bitmap_phys_addr, bitmap_size_bytes);
    log::info!("[MEMORY] free frames: {}", allocator.free_frames);

    // 5. Reserve DMA pool (64KB contiguous = 16 pages)
    let dma_pages = 16;
    let mut dma_start_frame = 0;
    let mut current_run = 0;
    let mut run_start = 0;
    
    for i in 0..allocator.total_frames {
        if !allocator.get_bit(i) {
            if current_run == 0 {
                run_start = i;
            }
            current_run += 1;
            if current_run == dma_pages {
                dma_start_frame = run_start;
                break;
            }
        } else {
            current_run = 0;
        }
    }
    
    if dma_start_frame != 0 {
        for i in dma_start_frame..(dma_start_frame + dma_pages) {
            allocator.set_bit(i, true);
            allocator.free_frames -= 1;
        }
        unsafe { DMA_POOL_PHYS = (dma_start_frame as u64) * PAGE_SIZE; }
        log::info!("[MEMORY] Reserved 64KB contiguous DMA pool at physical {:#x}", unsafe { DMA_POOL_PHYS });
    } else {
        panic!("Failed to allocate 64KB DMA pool during boot");
    }
}

// --- Development Testing Helpers ---

#[cfg(feature = "test_memory")]
pub fn bitmap_phys_addr() -> u64 {
    ALLOCATOR.lock().bitmap_phys_addr
}

#[cfg(feature = "test_memory")]
pub fn is_frame_used(frame: PhysFrame) -> bool {
    let idx = (frame.start_address().as_u64() / PAGE_SIZE) as usize;
    ALLOCATOR.lock().get_bit(idx)
}
