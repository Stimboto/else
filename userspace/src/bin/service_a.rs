#![no_std]
#![no_main]

use userspace::{sys_log, sys_send, Message};

#[unsafe(no_mangle)]
fn main() {
    sys_log("[service_a] Started execution.");
    
    // In Phase 7, capability index 1 is expected to be the IPC endpoint granted by init.
    let ep = 1;
    
    let msg = Message {
        rdi: 0xDEADBEEF,
        rsi: 0, rdx: 0, r10: 0, r8: 0, r9: 0,
    };
    
    sys_log("[service_a] Sending IPC message to init...");
    let res = sys_send(ep, &msg);
    if res == 0 {
        sys_log("[service_a] IPC send successful.");
    } else {
        sys_log("[service_a] IPC send failed.");
    }
    
    sys_log("[service_a] Exiting.");
}
