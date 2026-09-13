use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame};
use x86_64::VirtAddr;
use spin::Lazy;
use crate::arch::x86_64::gdt;
use crate::arch::x86_64::cpu;
use crate::arch::x86_64::interrupts::InterruptIndex;

static IDT: Lazy<InterruptDescriptorTable> = Lazy::new(|| {
    let mut idt = InterruptDescriptorTable::new();
    
    unsafe {
        // CPU Exceptions
        idt.breakpoint.set_handler_addr(VirtAddr::new(breakpoint_handler_wrapper as *const () as usize as u64));
        
        idt.double_fault
            .set_handler_addr(VirtAddr::new(double_fault_handler_wrapper as *const () as usize as u64))
            .set_stack_index(gdt::DOUBLE_FAULT_IST_INDEX);
        
        idt.general_protection_fault.set_handler_addr(VirtAddr::new(general_protection_fault_handler_wrapper as *const () as usize as u64));
        idt.invalid_opcode.set_handler_addr(VirtAddr::new(invalid_opcode_handler_wrapper as *const () as usize as u64));
        idt.page_fault.set_handler_addr(VirtAddr::new(page_fault_handler_wrapper as *const () as usize as u64));
        idt.divide_error.set_handler_addr(VirtAddr::new(divide_error_handler_wrapper as *const () as usize as u64));
        
        // Map the timer interrupt (IRQ 0)
        idt[InterruptIndex::Timer as u8].set_handler_addr(VirtAddr::new(crate::task::timer_interrupt_stub as *const () as usize as u64));
        
        // Map the keyboard interrupt (IRQ 1)
        idt[InterruptIndex::Keyboard as u8].set_handler_addr(VirtAddr::new(keyboard_handler_wrapper as *const () as usize as u64));
        
        // Map the ATA Primary interrupt (IRQ 14 -> Vector 46)
        idt[46].set_handler_addr(VirtAddr::new(ata_handler_wrapper as *const () as usize as u64));
        
        // Map the RTL8139 interrupt (IRQ 11 usually -> Vector 47)
        idt[47].set_handler_addr(VirtAddr::new(rtl8139_handler_wrapper as *const () as usize as u64));
    }

    idt
});

pub fn init() {
    log::info!("[CPU] Loading IDT...");
    IDT.load();
}

use core::arch::global_asm;

// Macro to generate a stable interrupt wrapper using global_asm!
// This saves caller-saved registers and passes a pointer to the interrupt frame.
macro_rules! interrupt_wrapper {
    ($name:ident, $impl_name:ident) => {
        global_asm!(
            concat!(".global ", stringify!($name)),
            concat!(stringify!($name), ":"),
            // Check if coming from user mode (CS != 0x8)
            "cmp qword ptr [rsp + 8], 0x8",
            "je 1f",
            "swapgs",
            "1:",
            "push r12",
            "push rax",
            "push rcx",
            "push rdx",
            "push rsi",
            "push rdi",
            "push r8",
            "push r9",
            "push r10",
            "push r11",
            
            // First argument (RDI): pointer to InterruptStackFrame
            // We pushed 10 registers (80 bytes)
            "mov rdi, rsp",
            "add rdi, 80",
            
            // Second argument (RSI): not applicable for non-error code exceptions
            "mov rsi, 0",
            
            "mov r12, rsp",
            "and rsp, -16",
            concat!("call ", stringify!($impl_name)),
            "mov rsp, r12",
            
            "pop r11",
            "pop r10",
            "pop r9",
            "pop r8",
            "pop rdi",
            "pop rsi",
            "pop rdx",
            "pop rcx",
            "pop rax",
            "pop r12",
            
            "cmp qword ptr [rsp + 8], 0x8",
            "je 2f",
            "swapgs",
            "2:",
            "iretq"
        );
        extern "C" {
            pub fn $name();
        }
    };
}

// Macro for exceptions that push an error code (like double fault, page fault)
macro_rules! interrupt_wrapper_error_code {
    ($name:ident, $impl_name:ident) => {
        global_asm!(
            concat!(".global ", stringify!($name)),
            concat!(stringify!($name), ":"),
            // Error code is already pushed by CPU. Swap it with a register or just save around it.
            // Stack: [Error Code] [RIP] [CS] [RFLAGS] [RSP] [SS]
            "cmp qword ptr [rsp + 16], 0x8", // CS is at RSP+16 because of the error code
            "je 1f",
            "swapgs",
            "1:",
            "push r12",
            "push rax",
            "push rcx",
            "push rdx",
            "push rsi",
            "push rdi",
            "push r8",
            "push r9",
            "push r10",
            "push r11",
            
            // First argument: InterruptStackFrame pointer
            "mov rdi, rsp",
            "add rdi, 88", // 80 bytes of registers + 8 byte error code
            
            // Second argument: Error code
            "mov rsi, [rsp + 80]",
            
            "mov r12, rsp",
            "and rsp, -16",
            concat!("call ", stringify!($impl_name)),
            "mov rsp, r12",
            
            "pop r11",
            "pop r10",
            "pop r9",
            "pop r8",
            "pop rdi",
            "pop rsi",
            "pop rdx",
            "pop rcx",
            "pop rax",
            "pop r12",
            "add rsp, 8", // pop error code
            "cmp qword ptr [rsp + 8], 0x8", // After popping error code, CS is at RSP+8
            "je 2f",
            "swapgs",
            "2:",
            "iretq"
        );
        extern "C" {
            pub fn $name();
        }
    };
}

interrupt_wrapper!(breakpoint_handler_wrapper, breakpoint_handler_impl);
#[no_mangle]
pub extern "C" fn breakpoint_handler_impl(stack_frame: *const InterruptStackFrame) {
    let stack_frame = unsafe { &*stack_frame };
    log::warn!("[EXCEPTION] BREAKPOINT\n{:#?}", stack_frame);
}

interrupt_wrapper_error_code!(double_fault_handler_wrapper, double_fault_handler_impl);
#[no_mangle]
pub extern "C" fn double_fault_handler_impl(
    stack_frame: *const InterruptStackFrame,
    _error_code: u64,
) {
    let stack_frame = unsafe { &*stack_frame };
    log::error!("[EXCEPTION] DOUBLE FAULT\n{:#?}", stack_frame);
    loop { cpu::halt(); }
}

interrupt_wrapper_error_code!(general_protection_fault_handler_wrapper, general_protection_fault_handler_impl);
#[no_mangle]
pub extern "C" fn general_protection_fault_handler_impl(
    stack_frame: *const InterruptStackFrame,
    error_code: u64,
) {
    let stack_frame = unsafe { &*stack_frame };
    log::error!("[EXCEPTION] GENERAL PROTECTION FAULT (Code: {})\n{:#?}", error_code, stack_frame);
    loop { cpu::halt(); }
}

interrupt_wrapper!(invalid_opcode_handler_wrapper, invalid_opcode_handler_impl);
#[no_mangle]
pub extern "C" fn invalid_opcode_handler_impl(stack_frame: *const InterruptStackFrame) {
    let stack_frame = unsafe { &*stack_frame };
    log::error!("[EXCEPTION] INVALID OPCODE\n{:#?}", stack_frame);
    loop { cpu::halt(); }
}

interrupt_wrapper_error_code!(page_fault_handler_wrapper, page_fault_handler_impl);
#[no_mangle]
pub extern "C" fn page_fault_handler_impl(
    stack_frame: *const InterruptStackFrame,
    error_code: u64,
) {
    let stack_frame = unsafe { &*stack_frame };
    use x86_64::registers::control::Cr2;
    use x86_64::structures::idt::PageFaultErrorCode;
    
    let parsed_error = PageFaultErrorCode::from_bits_truncate(error_code);
    
    if parsed_error.contains(PageFaultErrorCode::USER_MODE) {
        log::error!("[EXCEPTION] USER MODE PAGE FAULT");
        log::error!("Accessed Address: {:?}", Cr2::read());
        log::error!("Error Code: {:?} ({:#x})", parsed_error, error_code);
        log::error!("Terminating faulting user thread.");
        crate::task::scheduler::exit_current_thread();
    } else {
        log::error!("[EXCEPTION] KERNEL PAGE FAULT");
        log::error!("Accessed Address: {:?}", Cr2::read());
        log::error!("Error Code: {:?} ({:#x})", parsed_error, error_code);
        log::error!("{:#?}", stack_frame);
        loop { cpu::halt(); }
    }
}

interrupt_wrapper!(divide_error_handler_wrapper, divide_error_handler_impl);
#[no_mangle]
pub extern "C" fn divide_error_handler_impl(stack_frame: *const InterruptStackFrame) {
    let stack_frame = unsafe { &*stack_frame };
    log::error!("[EXCEPTION] DIVIDE BY ZERO\n{:#?}", stack_frame);
    loop { cpu::halt(); }
}

#[cfg(feature = "test_exceptions")]
pub fn test_breakpoint() {
    log::info!("[TEST] Triggering deliberate breakpoint...");
    x86_64::instructions::interrupts::int3();
}

interrupt_wrapper!(keyboard_handler_wrapper, keyboard_interrupt_handler);
interrupt_wrapper!(ata_handler_wrapper, ata_interrupt_handler);
interrupt_wrapper!(rtl8139_handler_wrapper, rtl8139_interrupt_handler);

#[no_mangle]
extern "C" fn keyboard_interrupt_handler(stack_frame: &InterruptStackFrame) {
    log::info!("[INTERRUPT] Keyboard IRQ 1 fired!");
    
    // Unblock any user thread waiting for IRQ 1
    crate::task::scheduler::unblock_irq(1);
    
    // Acknowledge the interrupt
    unsafe {
        crate::arch::x86_64::interrupts::PICS.lock().notify_end_of_interrupt(InterruptIndex::Keyboard as u8);
        crate::arch::x86_64::apic::eoi();
    }
}

#[no_mangle]
extern "C" fn ata_interrupt_handler(stack_frame: &InterruptStackFrame) {
    let irq = 14;
    crate::task::scheduler::unblock_irq(irq);
    
    unsafe {
        crate::arch::x86_64::interrupts::PICS.lock().notify_end_of_interrupt(46);
        crate::arch::x86_64::apic::eoi();
    }
}

#[no_mangle]
extern "C" fn rtl8139_interrupt_handler(stack_frame: &InterruptStackFrame) {
    crate::task::scheduler::unblock_irq(11); // Will be routed via IOAPIC
    
    unsafe {
        crate::arch::x86_64::apic::eoi();
    }
}
