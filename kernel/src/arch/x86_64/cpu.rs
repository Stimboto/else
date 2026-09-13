use core::arch::asm;

/// Halts the CPU until the next interrupt arrives.
pub fn halt() {
    // SAFE: Halting the CPU is a normal operation to reduce power consumption while idle.
    unsafe {
        asm!("hlt", options(nomem, nostack, preserves_flags));
    }
}

/// Disables hardware interrupts.
pub fn interrupts_disable() {
    x86_64::instructions::interrupts::disable();
}

/// Enables hardware interrupts.
pub fn interrupts_enable() {
    x86_64::instructions::interrupts::enable();
}

/// Executes a closure with interrupts disabled.
pub fn without_interrupts<F, R>(f: F) -> R
where
    F: FnOnce() -> R,
{
    x86_64::instructions::interrupts::without_interrupts(f)
}

#[repr(C, packed)]
pub struct PerCpu {
    pub kernel_stack: u64,
    pub user_stack: u64,
}

pub static mut PER_CPU: PerCpu = PerCpu {
    kernel_stack: 0,
    user_stack: 0,
};

pub fn init_per_cpu() {
    unsafe {
        let addr = &raw const PER_CPU as u64;
        log::info!("[CPU] Initializing PerCpu at address {:#x}", addr);
        x86_64::registers::model_specific::GsBase::write(x86_64::VirtAddr::new(addr));
    }
}

pub fn set_kernel_stack(rsp: u64) {
    unsafe {
        PER_CPU.kernel_stack = rsp;
    }
}
