#![no_std]
#![no_main]

mod logging;
mod panic;
pub mod arch;
pub mod memory;
pub mod task;
pub mod capability;
pub mod ipc;
pub mod syscall;
pub mod elf;
pub mod fs;
pub mod pci;

pub static INITRAMFS: &[u8] = include_bytes!("../../target/initramfs.tar");

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct FramebufferInfo {
    pub physical_address: u64,
    pub width: u64,
    pub height: u64,
    pub pitch: u64,
    pub bpp: u16,
    pub size: u64,
}

pub static FRAMEBUFFER_INFO: spin::Once<FramebufferInfo> = spin::Once::new();

extern crate alloc;


use limine::request::{BootloaderInfoRequest, RsdpRequest, FramebufferRequest};
use limine::BaseRevision;
use arch::x86_64::cpu;

#[used]
#[link_section = ".requests"]
static BASE_REVISION: BaseRevision = BaseRevision::new();

#[used]
#[link_section = ".requests"]
static BOOT_INFO_REQUEST: BootloaderInfoRequest = BootloaderInfoRequest::new();

#[used]
#[link_section = ".requests"]
static RSDP_REQUEST: RsdpRequest = RsdpRequest::new();

#[used]
#[link_section = ".requests"]
static FRAMEBUFFER_REQUEST: FramebufferRequest = FramebufferRequest::new();

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    // Initialize the serial logger first
    logging::init();
    
    log::info!("[ELSE] booting");
    log::info!("[ELSE] architecture: x86_64");

    if let Some(boot_info) = BOOT_INFO_REQUEST.get_response() {
        log::info!("[ELSE] Bootloader: {} {}", 
            boot_info.name(),
            boot_info.version()
        );
    } else {
        log::warn!("[ELSE] Failed to get bootloader info");
    }

    // Initialize Memory Subsystem FIRST so HHDM_OFFSET is available
    crate::memory::init();

    // Initialize architecture (GDT, IDT, PIC, APIC)
    let mut rsdp_addr = 0;
    if let Some(rsdp_response) = RSDP_REQUEST.get_response() {
        rsdp_addr = rsdp_response.address() as u64;
        log::info!("[ELSE] RSDP retrieved from bootloader at {:#x}", rsdp_addr);
    } else {
        log::warn!("[ELSE] Failed to get RSDP from bootloader");
    }
    
    arch::init(rsdp_addr);

    // Initialize Task Subsystem
    crate::task::init();

    // Initialize Syscalls
    crate::syscall::init();

    // Store Framebuffer Info
    if let Some(fb_res) = FRAMEBUFFER_REQUEST.get_response() {
        if let Some(fb) = fb_res.framebuffers().next() {
            let info = FramebufferInfo {
                physical_address: fb.addr() as u64, // Wait, limine maps framebuffer in HHDM. fb.addr() is virtual. Let's subtract hhdm.
                width: fb.width() as u64,
                height: fb.height() as u64,
                pitch: fb.pitch() as u64,
                bpp: fb.bpp() as u16,
                size: (fb.pitch() * fb.height()) as u64,
            };
            FRAMEBUFFER_INFO.call_once(|| FramebufferInfo {
                physical_address: (fb.addr() as u64) - crate::memory::hhdm_offset(),
                ..info
            });
            log::info!("[ELSE] Framebuffer initialized: {}x{} ({} bpp)", fb.width(), fb.height(), fb.bpp());
        }
    }

    log::info!("[ELSE] kernel online");

    #[cfg(feature = "test_memory")]
    {
        use crate::memory::frame_allocator::{ALLOCATOR, bitmap_phys_addr, is_frame_used};
        use x86_64::{structures::paging::{Page, PageTableFlags, Size4KiB, FrameAllocator}, VirtAddr, PhysAddr};
        use crate::memory::page_table;

        log::info!("[TEST] Executing memory diagnostics...");
        
        // 0. Verify bitmap self-reservation
        let bitmap_phys = bitmap_phys_addr();
        let bitmap_frame = x86_64::structures::paging::PhysFrame::containing_address(PhysAddr::new(bitmap_phys));
        assert!(is_frame_used(bitmap_frame), "Bitmap's own storage was NOT reserved!");
        log::info!("[TEST] Bitmap self-reservation verified.");

        // 1. Allocate a physical frame
        let frame = ALLOCATOR.lock().allocate_frame().expect("Failed to allocate test frame");
        log::info!("[TEST] Allocated frame at {:#x}", frame.start_address().as_u64());

        // 2. Map the frame to a test virtual address
        let test_virt = VirtAddr::new(0xffff_a000_0000_0000);
        let test_page = Page::<Size4KiB>::containing_address(test_virt);
        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
        
        page_table::map_page(test_page, frame, flags);
        log::info!("[TEST] Mapped page {:#x} -> frame {:#x}", test_page.start_address().as_u64(), frame.start_address().as_u64());

        // 3. Write and read a test value
        let ptr = test_virt.as_mut_ptr::<u64>();
        unsafe {
            *ptr = 0xDEADBEEFCAFEBABE;
        }
        let val = unsafe { *ptr };
        log::info!("[TEST] Wrote and read value: {:#x}", val);
        assert_eq!(val, 0xDEADBEEFCAFEBABE);

        // 4. Unmap the page and free the frame
        page_table::unmap_page(test_page);
        log::info!("[TEST] Unmapped page {:#x}", test_page.start_address().as_u64());
        use x86_64::structures::paging::FrameDeallocator;
        unsafe { ALLOCATOR.lock().deallocate_frame(frame) };
        assert!(!is_frame_used(frame), "Frame was not marked free after deallocation!");
        log::info!("[TEST] Freed frame {:#x} successfully", frame.start_address().as_u64());

        // 5. Test Heap Allocation
        extern crate alloc;
        let b = alloc::boxed::Box::new(42_u64);
        log::info!("[TEST] Heap allocated Box: {}", *b);
        
        log::info!("[TEST] Memory diagnostics passed!");
    }

    #[cfg(feature = "test_page_fault")]
    {
        log::info!("[TEST] Triggering deliberate page fault...");
        let ptr = 0xdeadbeef as *mut u64;
        unsafe { *ptr = 42; } // This will trigger a page fault and panic
    }

    #[cfg(feature = "test_exceptions")]
    {
        // Trigger a development-only exception test
        arch::idt::test_breakpoint();
    }

    #[cfg(feature = "test_ipc")]
    {
        use crate::task::scheduler::{Thread, SCHEDULER, sys_send, sys_recv};
        use crate::ipc::Message;
        use alloc::sync::Arc;
        
        log::info!("[TEST] Setting up IPC Test...");
        
        let cspace = Arc::new(spin::Mutex::new(crate::capability::CSpace::new()));
        
        // Create an endpoint in the shared cspace
        let ep = Arc::new(crate::ipc::Endpoint::new());
        let cap = crate::capability::Capability::Endpoint(ep, crate::capability::Rights::ALL);
        let ep_handle = cspace.lock().insert(cap);
        
        extern "C" fn sender_thread() {
            crate::arch::x86_64::cpu::interrupts_enable();
            log::info!("[IPC TEST] Sender Thread running...");
            let msg = crate::ipc::Message { rdi: 42, rsi: 99, rdx: 0, r10: 0, r8: 0, r9: 0 };
            
            // Wait for next timer tick to ensure receiver blocks first
            crate::task::scheduler::yield_now(); 
            
            log::info!("[IPC TEST] Sending message...");
            if let Err(e) = crate::task::scheduler::sys_send(0, msg) {
                log::error!("[IPC TEST] Send failed: {:?}", e);
            }
            log::info!("[IPC TEST] Message sent! Sender halting.");
            loop { crate::arch::x86_64::cpu::halt(); }
        }
        
        extern "C" fn receiver_thread() {
            crate::arch::x86_64::cpu::interrupts_enable();
            log::info!("[IPC TEST] Receiver Thread running...");
            log::info!("[IPC TEST] Blocking on receive...");
            match crate::task::scheduler::sys_recv(0) {
                Ok(msg) => {
                    log::info!("[IPC TEST] Message received! rdi: {}, rsi: {}", msg.rdi, msg.rsi);
                }
                Err(e) => {
                    log::error!("[IPC TEST] Recv failed: {:?}", e);
                }
            }
            log::info!("[IPC TEST] Receiver halting.");
            loop { crate::arch::x86_64::cpu::halt(); }
        }
        
        let mut t1 = Thread::new_kernel_thread(sender_thread);
        t1.cspace = Some(cspace.clone());
        
        let mut t2 = Thread::new_kernel_thread(receiver_thread);
        t2.cspace = Some(cspace.clone());
        
        SCHEDULER.lock().add_thread(t2); // Add receiver first so it blocks
        SCHEDULER.lock().add_thread(t1); // Add sender second
        
        log::info!("[TEST] IPC threads scheduled. Waiting for timer interrupt...");
    }

    #[cfg(feature = "test_scheduler")]
    {
        use crate::task::{Thread, SCHEDULER, Process};
        use x86_64::{structures::paging::{Page, PageTableFlags, Size4KiB, PhysFrame, FrameAllocator}, VirtAddr};
        use crate::memory::frame_allocator::ALLOCATOR;

        log::info!("[TEST] Spawning kernel threads...");
        extern "C" fn kernel_thread_1() {
            crate::arch::x86_64::cpu::interrupts_enable(); // Enable interrupts!
            log::info!("[THREAD 1] Started in kernel mode. Yielding to scheduler.");
            loop { crate::arch::x86_64::cpu::halt(); }
        }
        
        extern "C" fn kernel_thread_2() {
            crate::arch::x86_64::cpu::interrupts_enable(); // Enable interrupts!
            log::info!("[THREAD 2] Started in kernel mode. Yielding to scheduler.");
            loop { crate::arch::x86_64::cpu::halt(); }
        }
        
        SCHEDULER.lock().add_thread(Thread::new_kernel_thread(kernel_thread_1));
        SCHEDULER.lock().add_thread(Thread::new_kernel_thread(kernel_thread_2));
        
        log::info!("[TEST] Spawning user thread...");
        
        let mut process = Process::new();
        let code_frame = ALLOCATOR.lock().allocate_frame().unwrap();
        let stack_frame = ALLOCATOR.lock().allocate_frame().unwrap();
        
        // Let's write the raw user payload to the physical frame via HHDM
        let hhdm = crate::memory::hhdm_offset();
        let code_virt = code_frame.start_address().as_u64() + hhdm;
        let code_ptr = code_virt as *mut u8;
        
        // Simple raw x86_64 assembly for user mode:
        // We will try to write to a kernel address to trigger a page fault!
        // mov rax, 0xffffa00000000000
        // mov [rax], 42
        // jmp $
        let payload = [
            0x48, 0xb8, 0x00, 0x00, 0x00, 0x00, 0x00, 0xa0, 0xff, 0xff, // movabs rax, 0xffffa00000000000
            0x48, 0xc7, 0x00, 0x2a, 0x00, 0x00, 0x00,                   // mov qword ptr [rax], 42
            0xeb, 0xfe                                                  // jmp $ (infinite loop)
        ];
        
        unsafe {
            core::ptr::copy_nonoverlapping(payload.as_ptr(), code_ptr, payload.len());
        }
        
        let user_code_addr = VirtAddr::new(0x400000);
        let user_stack_addr = VirtAddr::new(0x800000);
        
        let user_flags = PageTableFlags::PRESENT | PageTableFlags::USER_ACCESSIBLE;
        process.map_page(Page::<Size4KiB>::containing_address(user_code_addr), code_frame, user_flags);
        process.map_page(Page::<Size4KiB>::containing_address(user_stack_addr), stack_frame, user_flags | PageTableFlags::WRITABLE);
        
        let user_stack_top = user_stack_addr.as_u64() + 4096;
        let user_thread = Thread::new_user_thread(user_code_addr.as_u64(), user_stack_top, process.cr3.start_address().as_u64(), process.cspace.clone());
        SCHEDULER.lock().add_thread(user_thread);
        
        log::info!("[TEST] Threads scheduled. Waiting for timer interrupt to preempt into scheduler...");
    }

    let tar = crate::fs::tar::TarArchive::new(INITRAMFS);
    if let Some(elf_bytes) = tar.find_file("init") {
        let (_, cspace_arc) = crate::elf::spawn_from_elf(elf_bytes, None);
        let mut cspace = cspace_arc.lock();
        
        // Grant hardware capabilities to init
        // handle 1: IRQ 1 (Keyboard)
        cspace.insert_at(1, crate::capability::Capability::Interrupt(1));
        // handle 2: PortIO 0x60-0x64 (Keyboard/PS2)
        cspace.insert_at(2, crate::capability::Capability::PortIO(0x60, 5, 1));
        // handle 3: Framebuffer MemoryMap
        if let Some(fb) = crate::FRAMEBUFFER_INFO.get() {
            cspace.insert_at(3, crate::capability::Capability::MemoryMap(fb.physical_address, fb.size));
        }
        // handle 4: IRQ 14 (ATA Primary)
        cspace.insert_at(4, crate::capability::Capability::Interrupt(14));
        // handle 5: PortIO 0x1F0-0x1F7 (ATA Primary Bus)
        cspace.insert_at(5, crate::capability::Capability::PortIO(0x1F0, 8, 0));
        // handle 6: PortIO 0x3F6 (ATA Primary Control)
        cspace.insert_at(6, crate::capability::Capability::PortIO(0x3F6, 1, 1));
        
        // Scan PCI for RTL8139
        if let Some(nic) = crate::pci::scan_bus_zero() {
            // Route the IRQ to Vector 47
            crate::arch::x86_64::ioapic::route_irq(nic.irq_line, 47, 0);
            
            // handle 20: RTL8139 Interrupt
            cspace.insert_at(20, crate::capability::Capability::Interrupt(nic.irq_line));
            
            // handle 21: RTL8139 BAR0 (PortIO or MMIO)
            if nic.bar0_is_io {
                cspace.insert_at(21, crate::capability::Capability::PortIO(nic.bar0_base as u16, nic.bar0_size as u16, 0));
            } else {
                cspace.insert_at(21, crate::capability::Capability::MemoryMap(nic.bar0_base as u64, nic.bar0_size as u64));
            }
            
            // Grant a 64KB DMA pool capability
            // handle 22: RTL8139 DMA Pool
            let dma_phys = unsafe { crate::memory::frame_allocator::DMA_POOL_PHYS };
            cspace.insert_at(22, crate::capability::Capability::MemoryMap(dma_phys, 65536));
        }
    } else {
        panic!("init not found in initramfs");
    }
    
    // Enable interrupts now that all initial threads and capabilities are ready
    crate::arch::x86_64::cpu::interrupts_enable();
    log::info!("Kernel initialization complete. Yielding to scheduler.");
    
    loop {
        crate::task::scheduler::yield_now();
    }
}
