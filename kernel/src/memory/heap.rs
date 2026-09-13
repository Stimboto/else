use linked_list_allocator::LockedHeap;
use x86_64::{structures::paging::{Page, PageTableFlags, Size4KiB, FrameAllocator}, VirtAddr};

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

pub const HEAP_START: usize = 0xffff_9000_0000_0000;
pub const HEAP_SIZE: usize = 100 * 1024; // 100 KiB

pub fn init() {
    let page_range = {
        let heap_start = VirtAddr::new(HEAP_START as u64);
        let heap_end = heap_start + HEAP_SIZE as u64 - 1u64;
        let heap_start_page = Page::<Size4KiB>::containing_address(heap_start);
        let heap_end_page = Page::<Size4KiB>::containing_address(heap_end);
        Page::range_inclusive(heap_start_page, heap_end_page)
    };

    let mut frame_allocator = super::frame_allocator::ALLOCATOR.lock();

    for page in page_range {
        let frame = frame_allocator
            .allocate_frame()
            .expect("Failed to allocate physical frame for kernel heap");
        
        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
        
        // We drop the lock temporarily so map_page can re-acquire it if it needs to allocate page tables.
        // Wait, map_page internally locks MAPPER and ALLOCATOR!
        // We must drop the lock here before calling map_page.
        drop(frame_allocator);
        
        super::page_table::map_page(page, frame, flags);
        
        // Re-acquire the lock for the next iteration
        frame_allocator = super::frame_allocator::ALLOCATOR.lock();
    }
    
    drop(frame_allocator);

    // SAFETY: We have just mapped the virtual pages for the heap region to valid physical frames.
    unsafe {
        ALLOCATOR.lock().init(HEAP_START as *mut u8, HEAP_SIZE);
    }
    
    log::info!("[MEMORY] heap initialized at {:#x} ({} bytes)", HEAP_START, HEAP_SIZE);
}
