use core::mem::size_of;
use alloc::sync::Arc;
use spin::Mutex;
use x86_64::structures::paging::{PageTableFlags, PhysFrame, Page, Size4KiB, FrameAllocator};
use x86_64::{PhysAddr, VirtAddr};

#[repr(C, packed)]
struct Elf64_Ehdr {
    e_ident: [u8; 16],
    e_type: u16,
    e_machine: u16,
    e_version: u32,
    e_entry: u64,
    e_phoff: u64,
    e_shoff: u64,
    e_flags: u32,
    e_ehsize: u16,
    e_phentsize: u16,
    e_phnum: u16,
    e_shentsize: u16,
    e_shnum: u16,
    e_shstrndx: u16,
}

#[repr(C, packed)]
struct Elf64_Phdr {
    p_type: u32,
    p_flags: u32,
    p_offset: u64,
    p_vaddr: u64,
    p_paddr: u64,
    p_filesz: u64,
    p_memsz: u64,
    p_align: u64,
}

const ELFMAG: [u8; 4] = [0x7F, b'E', b'L', b'F'];
const PT_LOAD: u32 = 1;

pub fn spawn_from_elf(elf_bytes: &[u8], ep_cap: Option<crate::capability::Capability>) -> (Arc<Mutex<crate::task::process::ProcessState>>, Arc<Mutex<crate::capability::CSpace>>) {
    log::info!("[ELF] Loading ELF...");
    
    if elf_bytes.len() < size_of::<Elf64_Ehdr>() {
        panic!("ELF too small");
    }
    
    let ehdr = unsafe { &*(elf_bytes.as_ptr() as *const Elf64_Ehdr) };
    if ehdr.e_ident[0..4] != ELFMAG {
        panic!("Invalid ELF magic");
    }
    
    let user_cr3_frame = crate::memory::frame_allocator::ALLOCATOR.lock().allocate_frame().expect("No frames for user CR3");
    let hhdm = crate::memory::hhdm_offset();
    
    let cr3_ptr = (hhdm + user_cr3_frame.start_address().as_u64()) as *mut x86_64::structures::paging::PageTable;
    
    unsafe {
        core::ptr::write_bytes(cr3_ptr, 0, 1);
        let (current_cr3, _) = x86_64::registers::control::Cr3::read();
        let current_cr3_ptr = (hhdm + current_cr3.start_address().as_u64()) as *const x86_64::structures::paging::PageTable;
        for i in 256..512 {
            (&mut *cr3_ptr)[i] = (&*current_cr3_ptr)[i].clone();
        }
    }
    
    let (old_cr3, old_flags) = x86_64::registers::control::Cr3::read();
    
    let mut mapper = unsafe {
        let cr3_virt = VirtAddr::new(hhdm + user_cr3_frame.start_address().as_u64());
        x86_64::structures::paging::OffsetPageTable::new(&mut *(cr3_virt.as_mut_ptr()), VirtAddr::new(hhdm))
    };
    
    let phoff = ehdr.e_phoff as usize;
    let phnum = ehdr.e_phnum as usize;
    
    for i in 0..phnum {
        let phdr_offset = phoff + i * size_of::<Elf64_Phdr>();
        let phdr = unsafe { &*(elf_bytes.as_ptr().add(phdr_offset) as *const Elf64_Phdr) };
        
        if phdr.p_type == PT_LOAD {
            let vaddr = phdr.p_vaddr;
            let memsz = phdr.p_memsz;
            let offset = phdr.p_offset;
            let filesz = phdr.p_filesz;
            
            let start_page = x86_64::structures::paging::Page::<x86_64::structures::paging::Size4KiB>::containing_address(x86_64::VirtAddr::new(vaddr));
            let end_page = x86_64::structures::paging::Page::containing_address(x86_64::VirtAddr::new(vaddr + memsz - 1));
            
            for page in x86_64::structures::paging::Page::range_inclusive(start_page, end_page) {
                let frame = crate::memory::frame_allocator::ALLOCATOR.lock().allocate_frame().expect("No frames for ELF segment");
                unsafe {
                    use x86_64::structures::paging::Mapper;
                    mapper.map_to(
                        page,
                        frame,
                        PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE,
                        &mut *crate::memory::frame_allocator::ALLOCATOR.lock()
                    ).expect("Failed to map user page").ignore();
                }
                
                let page_ptr = (hhdm + frame.start_address().as_u64()) as *mut u8;
                unsafe { core::ptr::write_bytes(page_ptr, 0, 4096); }
                
                let page_vaddr = page.start_address().as_u64();
                let segment_start = vaddr;
                let segment_end = vaddr + filesz;
                
                let copy_start = core::cmp::max(page_vaddr, segment_start);
                let copy_end = core::cmp::min(page_vaddr + 4096, segment_end);
                
                if copy_start < copy_end {
                    let copy_len = (copy_end - copy_start) as usize;
                    let src_offset = (copy_start - segment_start) as usize;
                    unsafe {
                        let dst = page_ptr.add((copy_start - page_vaddr) as usize);
                        let src = elf_bytes.as_ptr().add(offset as usize + src_offset);
                        core::ptr::copy_nonoverlapping(src, dst, copy_len);
                    }
                }
            }
        }
    }
    
    let stack_pages = 4;
    let stack_top = 0x0000700000000000u64;
    for i in 0..stack_pages {
        let page = x86_64::structures::paging::Page::containing_address(x86_64::VirtAddr::new(stack_top - (i + 1) * 4096));
        let frame = crate::memory::frame_allocator::ALLOCATOR.lock().allocate_frame().expect("No frames for user stack");
        unsafe {
            use x86_64::structures::paging::Mapper;
            mapper.map_to(
                page,
                frame,
                PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE,
                &mut *crate::memory::frame_allocator::ALLOCATOR.lock()
            ).expect("Failed to map user stack page").ignore();
        }
    }
    
    // No need to restore CR3 since we didn't change it
    
    let entry_point = ehdr.e_entry;
    
    let mut cspace_obj = crate::capability::CSpace::new();
    if let Some(cap) = ep_cap {
        cspace_obj.insert_at(1, cap); // Grant capability at index 1
    }
    let cspace = Arc::new(Mutex::new(cspace_obj));
    
    let process_state = Arc::new(Mutex::new(crate::task::process::ProcessState {
        terminated: false,
        wait_queue: alloc::collections::VecDeque::new(),
    }));
    
    let thread = crate::task::scheduler::Thread::new_user_thread(
        entry_point,
        stack_top,
        user_cr3_frame.start_address().as_u64(),
        cspace.clone(),
        process_state.clone(),
    );
    
    crate::task::scheduler::SCHEDULER.lock().add_thread(thread);
    log::info!("[ELF] User thread spawned.");
    
    (process_state, cspace)
}
