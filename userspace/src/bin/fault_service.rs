#![no_std]
#![no_main]

use core::ptr;
use userspace::sys_log;

#[unsafe(no_mangle)]
fn main() {
    sys_log("[fault_service] Started execution.");
    sys_log("[fault_service] Preparing to trigger a page fault to test isolation...");
    
    // Deliberately dereference a null pointer
    unsafe {
        ptr::read_volatile(0x0 as *const u64);
    }
    
    // This should never be reached
    sys_log("[fault_service] Error: Survived page fault!");
}
