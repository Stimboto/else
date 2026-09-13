use x86_64::structures::paging::{PageTable, PhysFrame, Size4KiB, PageTableFlags};
use x86_64::registers::control::{Cr3, Cr3Flags};
use crate::memory::frame_allocator::ALLOCATOR;
use x86_64::VirtAddr;
use alloc::vec;
use x86_64::structures::paging::FrameAllocator;

use alloc::sync::Arc;
use crate::capability::CSpace;
use spin::Mutex;
use alloc::collections::VecDeque;

pub struct ProcessState {
    pub terminated: bool,
    pub wait_queue: VecDeque<crate::task::scheduler::Thread>,
}

pub struct Process {
    pub cr3: PhysFrame,
    pub cspace: Arc<Mutex<CSpace>>,
    pub state: Arc<Mutex<ProcessState>>,
}

impl Process {
    pub fn new() -> Self {
        // Allocate a new frame for the L4 Page Table
        let l4_frame = ALLOCATOR.lock().allocate_frame().expect("Failed to allocate frame for L4 table");
        
        let hhdm_offset = crate::memory::hhdm_offset();
        let l4_virt_addr = VirtAddr::new(l4_frame.start_address().as_u64() + hhdm_offset);
        
        let l4_table_ptr = l4_virt_addr.as_mut_ptr::<PageTable>();
        
        unsafe {
            // Zero the table
            core::ptr::write_bytes(l4_table_ptr as *mut u8, 0, 4096);
            
            let l4_table = &mut *l4_table_ptr;
            
            // We must copy the high-half entries (indices 256..512) from the current CR3.
            let (current_cr3, _) = Cr3::read();
            let current_l4_virt = VirtAddr::new(current_cr3.start_address().as_u64() + hhdm_offset);
            let current_l4_table = &*current_l4_virt.as_ptr::<PageTable>();
            
            for i in 256..512 {
                l4_table[i] = current_l4_table[i].clone();
            }
        }
        
        Self {
            cr3: l4_frame,
            cspace: Arc::new(spin::Mutex::new(CSpace::new())),
            state: Arc::new(spin::Mutex::new(ProcessState {
                terminated: false,
                wait_queue: VecDeque::new(),
            })),
        }
    }
    
    pub fn activate(&self) {
        unsafe {
            Cr3::write(self.cr3, Cr3Flags::empty());
        }
    }
    
    pub fn map_page(&mut self, page: x86_64::structures::paging::Page<Size4KiB>, frame: PhysFrame, flags: PageTableFlags) {
        let hhdm_offset = crate::memory::hhdm_offset();
        let l4_virt_addr = VirtAddr::new(self.cr3.start_address().as_u64() + hhdm_offset);
        let l4_table_ptr = l4_virt_addr.as_mut_ptr::<PageTable>();
        
        let mut mapper = unsafe { x86_64::structures::paging::OffsetPageTable::new(&mut *l4_table_ptr, VirtAddr::new(hhdm_offset)) };
        let mut frame_allocator = ALLOCATOR.lock();
        
        let map_to_result = unsafe {
            use x86_64::structures::paging::Mapper;
            mapper.map_to(page, frame, flags, &mut *frame_allocator)
        };
        
        match map_to_result {
            Ok(tlb) => tlb.flush(), // This flush will flush on the CURRENT CR3, which might not be this process's CR3, but that's fine.
            Err(e) => panic!("Failed to map page in process: {:?}", e),
        }
    }
}
