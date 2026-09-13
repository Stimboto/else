use x86_64::registers::model_specific::{Star, LStar, SFMask};
use x86_64::structures::gdt::SegmentSelector;
use x86_64::PrivilegeLevel;
use x86_64::VirtAddr;
use crate::arch::x86_64::gdt;

core::arch::global_asm!(include_str!("entry.s"));

extern "C" {
    fn syscall_entry();
}

pub fn init() {
    log::info!("[SYSCALL] Initializing SYSCALL/SYSRET...");
    
    let selectors = gdt::selectors();
    
    // STAR MSR needs to be set up:
    // Bits 32-47: Kernel CS (SS is CS + 8)
    // Bits 48-63: User CS (CS is User CS + 16, SS is User CS + 8).
    // In our GDT:
    // kernel_code_selector (index 1)
    // kernel_data_selector (index 2)
    // user_data_selector (index 3)
    // user_code_selector (index 4)
    // So if User Base is index 2, SYSRET CS = 4 and SYSRET SS = 3.
    
    let user_cs = SegmentSelector::new(selectors.user_code_selector.index(), PrivilegeLevel::Ring3);
    let user_ds = SegmentSelector::new(selectors.user_data_selector.index(), PrivilegeLevel::Ring3);
    let kernel_cs = SegmentSelector::new(selectors.kernel_code_selector.index(), PrivilegeLevel::Ring0);
    let kernel_ds = SegmentSelector::new(selectors.kernel_data_selector.index(), PrivilegeLevel::Ring0);
    
    Star::write(
        user_cs, 
        user_ds, 
        kernel_cs, 
        kernel_ds
    ).unwrap();
    
    LStar::write(VirtAddr::new(syscall_entry as usize as u64));
    
    // Disable interrupts when entering syscall (mask IF)
    SFMask::write(x86_64::registers::rflags::RFlags::INTERRUPT_FLAG);
    
    // Enable System Call Extensions (EFER.SCE)
    unsafe {
        x86_64::registers::model_specific::Efer::update(|efer| {
            *efer |= x86_64::registers::model_specific::EferFlags::SYSTEM_CALL_EXTENSIONS;
        });
    }
}

#[no_mangle]
pub extern "C" fn syscall_handler(
    rdi: u64, rsi: u64, rdx: u64, r10: u64, r8: u64, r9: u64, rax: u64
) -> u64 {
    match rax {
        0 => {
            // SYS_YIELD
            crate::task::scheduler::yield_now();
            0
        },
        1 => {
            // SYS_EXIT
            crate::task::scheduler::exit_current_thread();
        },
        2 => {
            // SYS_LOG
            let ptr = rdi;
            let len = rsi;
            
            let end = ptr.checked_add(len);
            if end.is_none() || end.unwrap() >= 0x0000800000000000 {
                return (-2isize) as u64; // ERR_BAD_ADDRESS
            }
            
            let slice = unsafe { core::slice::from_raw_parts(ptr as *const u8, len as usize) };
            if let Ok(s) = core::str::from_utf8(slice) {
                log::info!("[USER] {}", s);
                0
            } else {
                (-1isize) as u64 // ERR_INVALID_ARG
            }
        },
        3 => {
            // SYS_EP_CREATE
            crate::task::scheduler::sys_endpoint_create() as u64
        },
        4 => {
            // SYS_SEND
            let handle = rdi as usize;
            let msg_ptr = rsi;
            
            // Pointer Validation!
            let msg_size = core::mem::size_of::<crate::ipc::Message>() as u64;
            let end = msg_ptr.checked_add(msg_size);
            if end.is_none() || end.unwrap() >= 0x0000800000000000 {
                return (-2isize) as u64; // ERR_BAD_ADDRESS
            }
            
            let msg = unsafe { core::ptr::read(msg_ptr as *const crate::ipc::Message) };
            match crate::task::scheduler::sys_send(handle, msg) {
                Ok(()) => 0,
                Err(_) => (-3isize) as u64, // ERR_CAPABILITY
            }
        },
        5 => {
            // SYS_RECV
            let handle = rdi as usize;
            let msg_ptr = rsi;
            
            let msg_size = core::mem::size_of::<crate::ipc::Message>() as u64;
            let end = msg_ptr.checked_add(msg_size);
            if end.is_none() || end.unwrap() >= 0x0000800000000000 {
                return (-2isize) as u64;
            }
            
            match crate::task::scheduler::sys_recv(handle) {
                Ok(msg) => {
                    unsafe { core::ptr::write(msg_ptr as *mut crate::ipc::Message, msg); }
                    0
                },
                Err(_) => (-3isize) as u64,
            }
        },
        6 => {
            // SYS_SPAWN (name_ptr, name_len, ep_handle)
            let name_ptr = rdi;
            let name_len = rsi;
            let ep_handle = rdx as usize;
            
            let end = name_ptr.checked_add(name_len);
            if end.is_none() || end.unwrap() >= 0x0000800000000000 {
                return (-2isize) as u64; // ERR_BAD_ADDRESS
            }
            let slice = unsafe { core::slice::from_raw_parts(name_ptr as *const u8, name_len as usize) };
            if let Ok(name) = core::str::from_utf8(slice) {
                // Get the Endpoint capability from current CSpace to grant to child
                let ep_cap_opt = x86_64::instructions::interrupts::without_interrupts(|| {
                    let mut scheduler = crate::task::scheduler::SCHEDULER.lock();
                    let current_thread = scheduler.current.as_ref().unwrap();
                    let cspace_arc = current_thread.cspace.as_ref().unwrap().clone();
                    let cspace = cspace_arc.lock();
                    cspace.get(ep_handle)
                });
                
                // Validate capability type and rights
                let granted_cap = match ep_cap_opt {
                    Some(crate::capability::Capability::Endpoint(ep, rights)) => {
                        // Grant the endpoint, maybe restrict rights if requested (Phase 7 allows full copy)
                        Some(crate::capability::Capability::Endpoint(ep, rights))
                    },
                    _ => None, // Invalid handle or wrong type
                };
                
                // Parse initramfs
                let tar = crate::fs::tar::TarArchive::new(crate::INITRAMFS);
                if let Some(elf_bytes) = tar.find_file(name) {
                    let (process_state, child_cspace) = crate::elf::spawn_from_elf(elf_bytes, granted_cap);
                    
                    // Clone caller's CSpace if ep_handle is MAX? No, just clone it anyway for now to give init's caps.
                    // Wait, we don't want every process to get all caps.
                    // If ep_handle == usize::MAX, we clone all caps.
                    if ep_handle == usize::MAX {
                        x86_64::instructions::interrupts::without_interrupts(|| {
                            let scheduler = crate::task::scheduler::SCHEDULER.lock();
                            let current_thread = scheduler.current.as_ref().unwrap();
                            let parent_cspace = current_thread.cspace.as_ref().unwrap().lock();
                            let mut child_cs = child_cspace.lock();
                            
                            for (i, cap_opt) in parent_cspace.slots.iter().enumerate() {
                                if let Some(cap) = cap_opt {
                                    child_cs.insert_at(i, cap.clone());
                                }
                            }
                        });
                    }
                    
                    // Create Process capability and insert into caller's CSpace
                    let handle = x86_64::instructions::interrupts::without_interrupts(|| {
                        let mut scheduler = crate::task::scheduler::SCHEDULER.lock();
                        let current_thread = scheduler.current.as_ref().unwrap();
                        let cspace_arc = current_thread.cspace.as_ref().unwrap().clone();
                        let mut cspace = cspace_arc.lock();
                        cspace.insert(crate::capability::Capability::Process(process_state, crate::capability::Rights::WAIT))
                    });
                    
                    log::info!("[KERNEL] sys_spawn('{}') returning handle {}", name, handle);
                    handle as u64
                } else {
                    log::info!("[KERNEL] sys_spawn('{}') failed to find file", name);
                    (-4isize) as u64 // ERR_NOT_FOUND
                }
            } else {
                (-1isize) as u64
            }
        },
        7 => {
            // SYS_WAIT (process_handle)
            let handle = rdi as usize;
            crate::task::scheduler::sys_wait(handle) as u64
        },
        8 => {
            // SYS_WAIT_IRQ
            crate::task::scheduler::sys_wait_irq(rdi as usize) as u64
        },
        9 => {
            // SYS_PORT_IN
            let handle = rdi as usize;
            let port = rsi as u16;
            let size = rdx as u8;
            
            let mut allowed = false;
            {
                let mut scheduler = crate::task::scheduler::SCHEDULER.lock();
                let current = scheduler.current.as_ref().unwrap();
                let cspace_arc = current.cspace.as_ref().unwrap().clone();
                let cspace = cspace_arc.lock();
                if let Some(crate::capability::Capability::PortIO(start, len, allowed_width)) = cspace.get(handle) {
                    if port >= start && (port as u32 + size as u32) <= (start as u32 + len as u32) {
                        if allowed_width == 0 || size == allowed_width {
                            allowed = true;
                        }
                    }
                }
            }
            
            if !allowed {
                let mut scheduler = crate::task::scheduler::SCHEDULER.lock();
                let current = scheduler.current.as_ref().unwrap();
                log::warn!("[SYS_PORT_IN] Denied! Thread ID: {}, Port: {:#x}, Size: {}", current.id, port, size);
            }

            if allowed {
                let mut val = 0u64;
                unsafe {
                    match size {
                        1 => {
                            let mut v: u8;
                            core::arch::asm!("in al, dx", out("al") v, in("dx") port);
                            val = v as u64;
                        },
                        2 => {
                            let mut v: u16;
                            core::arch::asm!("in ax, dx", out("ax") v, in("dx") port);
                            val = v as u64;
                        },
                        4 => {
                            let mut v: u32;
                            core::arch::asm!("in eax, dx", out("eax") v, in("dx") port);
                            val = v as u64;
                        },
                        _ => return u64::MAX,
                    }
                }
                val as u64
            } else {
                u64::MAX // Permission denied
            }
        },
        10 => {
            // SYS_PORT_OUT
            let handle = rdi as usize;
            let port = rsi as u16;
            let size = rdx as u8;
            let val = r10 as u32;
            
            let mut allowed = false;
            {
                let mut scheduler = crate::task::scheduler::SCHEDULER.lock();
                let current = scheduler.current.as_ref().unwrap();
                let cspace_arc = current.cspace.as_ref().unwrap().clone();
                let cspace = cspace_arc.lock();
                if let Some(crate::capability::Capability::PortIO(start, len, allowed_width)) = cspace.get(handle) {
                    if port >= start && (port as u32 + size as u32) <= (start as u32 + len as u32) {
                        if allowed_width == 0 || size == allowed_width {
                            allowed = true;
                        }
                    }
                }
            }
            
            if !allowed {
                let mut scheduler = crate::task::scheduler::SCHEDULER.lock();
                let current = scheduler.current.as_ref().unwrap();
                log::warn!("[SYS_PORT_OUT] Denied! Thread ID: {}, Port: {:#x}, Size: {}", current.id, port, size);
            }

            if allowed {
                let val_u8 = val as u8;
                let val_u16 = val as u16;
                unsafe {
                    match size {
                        1 => core::arch::asm!("out dx, al", in("al") val_u8, in("dx") port),
                        2 => core::arch::asm!("out dx, ax", in("ax") val_u16, in("dx") port),
                        4 => core::arch::asm!("out dx, eax", in("eax") val, in("dx") port),
                        _ => return u64::MAX,
                    }
                }
                0
            } else {
                u64::MAX // Permission denied
            }
        },
        11 => {
            // SYS_MAP_PHYS
            let handle = rdi as usize;
            let phys_addr = rsi as u64;
            let length = rdx as u64;
            
            let mut allowed = false;
            {
                let mut scheduler = crate::task::scheduler::SCHEDULER.lock();
                let current = scheduler.current.as_ref().unwrap();
                let cspace_arc = current.cspace.as_ref().unwrap().clone();
                let cspace = cspace_arc.lock();
                if let Some(crate::capability::Capability::MemoryMap(start, len)) = cspace.get(handle) {
                    let mut actual_phys = phys_addr;
                    let mut actual_len = length;
                    
                    if phys_addr == core::u64::MAX {
                        actual_phys = start;
                        actual_len = len;
                    }
                    
                    let end_requested = actual_phys.saturating_add(actual_len);
                    let end_allowed = start.saturating_add(len);
                    log::info!("[SYS_MAP_PHYS] Requested: {:#x} - {:#x} (len: {:#x})", actual_phys, end_requested, actual_len);
                    log::info!("[SYS_MAP_PHYS] Allowed:   {:#x} - {:#x} (len: {:#x})", start, end_allowed, len);
                    if actual_phys >= start && end_requested <= end_allowed && end_requested >= actual_phys {
                        allowed = true;
                        
                        // Overwrite the arguments so the rest of the function uses the actual values
                        // But wait, Rust function arguments are immutable by default unless declared mut.
                        // Let's just shadow them.
                    } else {
                        log::warn!("[SYS_MAP_PHYS] Access denied. Out of bounds.");
                    }
                } else {
                    log::warn!("[SYS_MAP_PHYS] Handle {} is not a MemoryMap capability", handle);
                    for (i, cap_opt) in cspace.slots.iter().enumerate() {
                        if let Some(_) = cap_opt {
                            log::warn!("  -> Slot {} has a capability", i);
                        } else {
                            log::warn!("  -> Slot {} is EMPTY", i);
                        }
                    }
                }
            }
            
            
            let mut actual_phys = phys_addr;
            let mut actual_len = length;
            
            if allowed {
                let cspace_arc = {
                    let scheduler = crate::task::scheduler::SCHEDULER.lock();
                    let current = scheduler.current.as_ref().unwrap();
                    current.cspace.as_ref().unwrap().clone()
                };
                if let Some(crate::capability::Capability::MemoryMap(start, len)) = cspace_arc.lock().get(handle) {
                    if phys_addr == core::u64::MAX {
                        actual_phys = start;
                        actual_len = len;
                    }
                }
                
                let cr3_phys = {
                    let scheduler = crate::task::scheduler::SCHEDULER.lock();
                    scheduler.current.as_ref().unwrap().cr3.unwrap()
                };
                
                // Pick a virtual address (e.g., above 2GB for MMIO)
                // For multiple allocations, we should ideally allocate dynamically, but for now we'll offset by handle
                let virt_addr = 0x8000_0000 + (handle as u64 * 0x1000_000); // offset each mapping by 16MB
                
                use x86_64::structures::paging::{Page, PhysFrame, Size4KiB, PageTableFlags};
                use x86_64::{PhysAddr, VirtAddr};
                
                let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE | PageTableFlags::NO_CACHE;
                
                let start_page = Page::<Size4KiB>::containing_address(VirtAddr::new(virt_addr));
                let end_page = Page::<Size4KiB>::containing_address(VirtAddr::new(virt_addr + actual_len - 1));
                
                let hhdm = crate::memory::hhdm_offset();
                let l4_table_ptr = (cr3_phys + hhdm) as *mut x86_64::structures::paging::PageTable;
                
                let mut current_phys = actual_phys;
                
                let mut frame_allocator = crate::memory::frame_allocator::ALLOCATOR.lock();
                let mut mapper = unsafe { x86_64::structures::paging::OffsetPageTable::new(&mut *l4_table_ptr, VirtAddr::new(hhdm)) };
                
                for page in Page::range_inclusive(start_page, end_page) {
                    let frame = PhysFrame::containing_address(PhysAddr::new(current_phys));
                    unsafe {
                        use x86_64::structures::paging::Mapper;
                        mapper.map_to(page, frame, flags, &mut *frame_allocator).expect("Failed to map phys_addr").ignore();
                    }
                    current_phys += 4096;
                }
                
                virt_addr
            } else {
                0 // Permission denied
            }
        },
        12 => {
            // SYS_FRAMEBUFFER_INFO
            let info_ptr = rdi;
            let info_size = core::mem::size_of::<crate::FramebufferInfo>() as u64;
            let end = info_ptr.checked_add(info_size);
            if end.is_none() || end.unwrap() >= 0x0000800000000000 {
                return (-2isize) as u64; // ERR_BAD_ADDRESS
            }
            if let Some(fb) = crate::FRAMEBUFFER_INFO.get() {
                unsafe { core::ptr::write(info_ptr as *mut crate::FramebufferInfo, *fb); }
                0
            } else {
                (-4isize) as u64 // ERR_NOT_FOUND
            }
        },
        13 => {
            // SYS_FRAME_ALLOC
            let size = rdi;
            if size > 4096 {
                log::warn!("[SYS_FRAME_ALLOC] Requested size > 4096 bytes. Rejecting.");
                return 0;
            }
            use x86_64::structures::paging::FrameAllocator;
            let frame_opt = crate::memory::frame_allocator::ALLOCATOR.lock().allocate_frame();
            if let Some(frame) = frame_opt {
                let phys_addr = frame.start_address().as_u64();
                unsafe {
                    let virt_addr = phys_addr + crate::memory::hhdm_offset();
                    core::ptr::write_bytes(virt_addr as *mut u8, 0, 4096);
                }
                let cap = crate::capability::Capability::MemoryMap(phys_addr, 4096);
                let mut scheduler = crate::task::scheduler::SCHEDULER.lock();
                let current = scheduler.current.as_mut().unwrap();
                let mut cspace = current.cspace.as_ref().unwrap().lock();
                let handle = cspace.insert(cap);
                log::info!("[SYS_FRAME_ALLOC] Allocated physical frame at {:#x}, returned handle {}", phys_addr, handle);
                handle as u64
            } else {
                log::error!("[SYS_FRAME_ALLOC] Out of physical memory!");
                0
            }
        },
        14 => {
            // SYS_DMA_ALLOC
            let size = rdi;
            if let Some(phys) = crate::memory::frame_allocator::allocate_dma_frames(size) {
                let cap = crate::capability::Capability::MemoryMap(phys.as_u64(), size);
                let mut scheduler = crate::task::scheduler::SCHEDULER.lock();
                let current = scheduler.current.as_mut().unwrap();
                let mut cspace = current.cspace.as_ref().unwrap().lock();
                let handle = cspace.insert(cap);
                log::info!("[SYS_DMA_ALLOC] Allocated {} bytes at physical {:#x}, handle {}", size, phys.as_u64(), handle);
                handle as u64
            } else {
                log::error!("[SYS_DMA_ALLOC] Out of contiguous DMA memory!");
                0
            }
        },
        15 => {
            // SYS_CAP_INFO
            let handle = rdi as usize;
            let mut scheduler = crate::task::scheduler::SCHEDULER.lock();
            let current = scheduler.current.as_mut().unwrap();
            let cspace = current.cspace.as_ref().unwrap().lock();
            if let Some(cap) = cspace.get(handle) {
                match cap {
                    crate::capability::Capability::MemoryMap(phys, _) => phys,
                    crate::capability::Capability::PortIO(base, _, _) => base as u64,
                    crate::capability::Capability::Interrupt(irq) => irq as u64,
                    _ => core::u64::MAX,
                }
            } else {
                core::u64::MAX
            }
        },
        16 => {
            // SYS_THREAD_SPAWN
            let entry_point = rdi;
            let user_stack = rsi;
            
            if entry_point >= 0x0000800000000000 || user_stack >= 0x0000800000000000 {
                return (-2isize) as u64; // ERR_BAD_ADDRESS
            }
            
            x86_64::instructions::interrupts::without_interrupts(|| {
                let mut scheduler = crate::task::scheduler::SCHEDULER.lock();
                let current_thread = scheduler.current.as_ref().unwrap();
                let cr3 = current_thread.cr3.unwrap();
                let cspace = current_thread.cspace.as_ref().unwrap().clone();
                let process_state = current_thread.process_state.as_ref().unwrap().clone();
                
                let new_thread = crate::task::scheduler::Thread::new_user_thread(
                    entry_point,
                    user_stack,
                    cr3,
                    cspace,
                    process_state
                );
                
                let thread_id = new_thread.id;
                scheduler.add_thread(new_thread);
                log::info!("[SYS_THREAD_SPAWN] Spawned user thread {} in same process", thread_id);
                thread_id
            })
        },
        _ => (-1isize) as u64, // ERR_UNKNOWN_SYSCALL
    }
}
