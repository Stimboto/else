#![no_std]
#![no_main]

use userspace::{sys_log, sys_map_phys, sys_recv, sys_send, Message, CAP_NETWORK_SERVICE_EP, CAP_APPLICATION_EP, CAP_PACKET_BUFFER};

pub const MSG_UDP_BIND: u64 = 103;
pub const MSG_UDP_SEND: u64 = 104;
pub const MSG_UDP_RECV: u64 = 105;



#[unsafe(no_mangle)]
pub extern "C" fn main() {
    sys_log("[NetTest] Starting...");
    
    let shared_buf_virt = sys_map_phys(CAP_PACKET_BUFFER, core::u64::MAX, core::u64::MAX);
    if shared_buf_virt == 0 {
        sys_log("[NetTest] Failed to map CAP_PACKET_BUFFER");
        loop {}
    }
    
    // Bind to port 8080
    let bind_msg = Message {
        rdi: MSG_UDP_BIND,
        rsi: 8080,
        rdx: CAP_APPLICATION_EP as u64,
        r10: 0, r8: 0, r9: 0
    };
    
    sys_send(CAP_NETWORK_SERVICE_EP, &bind_msg);
    sys_log("[NetTest] Bound to UDP port 8080");
    
    let mut msg = Message { rdi: 0, rsi: 0, rdx: 0, r10: 0, r8: 0, r9: 0 };
    
    loop {
        sys_recv(CAP_APPLICATION_EP, &mut msg);
        
        if msg.rdi == MSG_UDP_RECV {
            let payload_len = msg.rsi as usize;
            let src_ip = msg.rdx as u32;
            let src_port = msg.r10 as u16;
            
            sys_log("[NetTest] Received UDP packet!");
            sys_log("Length:");
            userspace::sys_log_num(payload_len);
            sys_log("Src Port:");
            userspace::sys_log_num(src_port as usize);
            
            // Print payload (if it's text)
            unsafe {
                let payload = core::slice::from_raw_parts((shared_buf_virt + 3072) as *const u8, payload_len);
                if let Ok(s) = core::str::from_utf8(payload) {
                    sys_log("[NetTest] Payload: ");
                    sys_log(s);
                }
                
                // Echo the packet back!
                // The payload is already at shared_buf_virt + 3072
                let send_msg = Message {
                    rdi: MSG_UDP_SEND,
                    rsi: src_ip as u64,
                    rdx: src_port as u64,
                    r10: 8080, // src port
                    r8: payload_len as u64,
                    r9: 0
                };
                
                sys_log("[NetTest] Sending echo reply...");
                sys_send(CAP_NETWORK_SERVICE_EP, &send_msg);
            }
        }
    }
}
