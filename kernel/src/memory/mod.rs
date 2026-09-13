use limine::request::{HhdmRequest, MemoryMapRequest};

pub mod frame_allocator;
pub mod heap;
pub mod page_table;

#[used]
#[link_section = ".requests"]
static HHDM_REQUEST: HhdmRequest = HhdmRequest::new();

#[used]
#[link_section = ".requests"]
static MEMORY_MAP_REQUEST: MemoryMapRequest = MemoryMapRequest::new();

/// The offset added to physical addresses to access them via the Higher Half Direct Map.
static mut HHDM_OFFSET: u64 = 0;

pub fn init() {
    log::info!("[MEMORY] initializing");

    // 1. Validate HHDM response
    let hhdm_response = HHDM_REQUEST
        .get_response()
        .expect("Bootloader did not provide an HHDM response!");
    
    // SAFETY: We only set this once during single-threaded boot
    unsafe {
        HHDM_OFFSET = hhdm_response.offset();
    }
    log::info!("[MEMORY] HHDM offset: {:#x}", hhdm_response.offset());

    // 2. Validate Memory Map response
    let memmap_response = MEMORY_MAP_REQUEST
        .get_response()
        .expect("Bootloader did not provide a memory map!");

    let entries = memmap_response.entries();
    let mut total_usable: u64 = 0;
    let mut total_reserved: u64 = 0;

    for entry in entries.iter() {
        if entry.entry_type == limine::memory_map::EntryType::USABLE {
            total_usable += entry.length;
        } else {
            total_reserved += entry.length;
        }
    }

    log::info!("[MEMORY] total usable RAM: {} MB", total_usable / 1024 / 1024);
    log::info!("[MEMORY] total reserved RAM: {} MB", total_reserved / 1024 / 1024);

    // 3. Initialize Physical Frame Allocator
    frame_allocator::init(memmap_response);

    // 4. Establish page-table mapper
    page_table::init();

    // 5. Initialize Kernel Heap
    heap::init();

    log::info!("[MEMORY] virtual memory initialized");
}

pub fn hhdm_offset() -> u64 {
    // SAFETY: HHDM_OFFSET is initialized before any concurrent execution or allocation occurs
    unsafe { HHDM_OFFSET }
}
