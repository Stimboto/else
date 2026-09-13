use core::panic::PanicInfo;
use core::arch::asm;

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    log::error!("[ELSE] KERNEL PANIC");
    if let Some(location) = info.location() {
        log::error!("Location: {}:{}", location.file(), location.line());
    }
    log::error!("Message: {}", info.message());

    // Enter idle loop to prevent continuous reboots or busy spinning
    loop {
        // SAFE: Halting the CPU is a normal response to a kernel panic
        unsafe {
            asm!("hlt", options(nomem, nostack, preserves_flags));
        }
    }
}
