use x86_64::structures::paging::{
    Mapper, OffsetPageTable, Page, PageTable, PageTableFlags, PhysFrame, Size4KiB,
};
use x86_64::VirtAddr;
use spin::Mutex;

/// The active level-4 page table.
static MAPPER: Mutex<Option<OffsetPageTable<'static>>> = Mutex::new(None);

pub fn init() {
    let hhdm_offset = super::hhdm_offset();
    let phys_offset = VirtAddr::new(hhdm_offset);

    // SAFETY: We get the active level 4 table from the CR3 register, and we know Limine
    // has correctly mapped all physical memory to the HHDM offset.
    let mapper = unsafe {
        let (level_4_table_frame, _) = x86_64::registers::control::Cr3::read();
        let phys = level_4_table_frame.start_address();
        let virt = phys_offset + phys.as_u64();
        let page_table_ptr: *mut PageTable = virt.as_mut_ptr();
        OffsetPageTable::new(&mut *page_table_ptr, phys_offset)
    };

    *MAPPER.lock() = Some(mapper);
    log::info!("[MEMORY] page-table mapper initialized");
}

/// Maps a virtual page to a physical frame with the given flags.
/// Allocates intermediate page tables from the global BitmapAllocator if necessary.
pub fn map_page(page: Page<Size4KiB>, frame: PhysFrame, flags: PageTableFlags) {
    let mut mapper_lock = MAPPER.lock();
    let mapper = mapper_lock.as_mut().expect("Mapper not initialized");
    
    let mut frame_allocator = super::frame_allocator::ALLOCATOR.lock();
    
    // SAFETY: We are trusting the caller to provide valid frames and flags, and the allocator
    // to provide valid unused frames for intermediate tables.
    let map_to_result = unsafe {
        mapper.map_to(page, frame, flags, &mut *frame_allocator)
    };
    
    match map_to_result {
        Ok(tlb) => tlb.flush(),
        Err(e) => panic!("Failed to map page {:?} to frame {:?}: {:?}", page, frame, e),
    }
}

/// Unmaps a virtual page.
pub fn unmap_page(page: Page<Size4KiB>) {
    let mut mapper_lock = MAPPER.lock();
    let mapper = mapper_lock.as_mut().expect("Mapper not initialized");
    
    let unmap_result = mapper.unmap(page);
    match unmap_result {
        Ok((_frame, tlb)) => {
            tlb.flush();
            // We intentionally do not deallocate the physical frame here, as the caller 
            // should manage physical memory lifecycle independently of virtual memory mappings.
        }
        Err(e) => panic!("Failed to unmap page {:?}: {:?}", page, e),
    }
}

/// Ensures that a physical memory range is mapped in the HHDM.
/// This is necessary because some bootloaders (like Limine Revision 1+)
/// may not map reserved or ACPI reclaimable memory in the HHDM.
pub fn ensure_mapped(phys_addr: u64, size: u64) {
    use x86_64::structures::paging::mapper::MapToError;
    use x86_64::PhysAddr;
    
    let hhdm = super::hhdm_offset();
    let mut mapper_lock = MAPPER.lock();
    let mapper = mapper_lock.as_mut().expect("Mapper not initialized");
    let mut frame_allocator = super::frame_allocator::ALLOCATOR.lock();
    
    let start_page = Page::<Size4KiB>::containing_address(VirtAddr::new(phys_addr + hhdm));
    let end_page = Page::<Size4KiB>::containing_address(VirtAddr::new(phys_addr + size.saturating_sub(1) + hhdm));
    
    for page in Page::range_inclusive(start_page, end_page) {
        let frame = PhysFrame::containing_address(PhysAddr::new(page.start_address().as_u64() - hhdm));
        
        // MMIO regions require WRITABLE and NO_CACHE flags to function correctly.
        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::NO_EXECUTE | PageTableFlags::NO_CACHE;
        
        let map_result = unsafe { mapper.map_to(page, frame, flags, &mut *frame_allocator) };
        match map_result {
            Ok(tlb) => tlb.flush(),
            Err(MapToError::PageAlreadyMapped(_)) => {
                // It's already mapped (likely by Limine HHDM as Write-Back). 
                // We MUST update the flags to NO_CACHE for MMIO!
                let update_result = unsafe { mapper.update_flags(page, flags) };
                match update_result {
                    Ok(tlb) => tlb.flush(),
                    Err(e) => {
                        log::error!("[MEMORY] Failed to update flags for already mapped page {:?}: {:?}", page, e);
                    }
                }
            },
            Err(e) => {
                log::error!("[MEMORY] Failed to ensure mapped page {:?}: {:?}", page, e);
                panic!("Failed to ensure mapped page {:?}: {:?}", page, e);
            }
        }
    }
}

