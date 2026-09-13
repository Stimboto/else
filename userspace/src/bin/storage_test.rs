#![no_std]
#![no_main]

use userspace::{sys_log, sys_recv, sys_send, sys_map_phys, Message};

pub const VFS_READ: u64 = 11;
pub const VFS_WRITE: u64 = 12;

#[unsafe(no_mangle)]
pub extern "C" fn main() {
    sys_log("[Storage Test] Starting...");
    
    // Dependencies:
    // Handle 8: VFS Endpoint
    // Handle 9: Shared Memory
    
    let vfs_ep = 9;
    let shm_handle = 10;
    
    let shm_virt = sys_map_phys(shm_handle, core::u64::MAX, core::u64::MAX);
    if shm_virt == 0 {
        sys_log("[Storage Test] Failed to map shared memory!");
        userspace::sys_exit();
    }
    
    let shm_buf = unsafe { core::slice::from_raw_parts_mut(shm_virt as *mut u8, 4096) };
    
    // 1. Write Test
    sys_log("[Storage Test] Writing signature to disk...");
    let signature = b"ELSE_PERSISTENCE_TEST_v1";
    for i in 0..signature.len() {
        shm_buf[i] = signature[i];
    }
    
    let mut msg = Message {
        rdi: VFS_WRITE,
        rsi: 0,
        rdx: 0,
        r10: 0,
        r8: 0,
        r9: 0,
    };
    sys_send(vfs_ep, &msg);
    sys_recv(vfs_ep, &mut msg);
    
    if msg.rdi != 0 {
        sys_log("[Storage Test] VFS_WRITE failed!");
        userspace::sys_exit();
    }
    
    // 2. Clear buffer
    for i in 0..512 {
        shm_buf[i] = 0;
    }
    
    // 3. Read Test
    sys_log("[Storage Test] Reading signature from disk...");
    msg.rdi = VFS_READ;
    sys_send(vfs_ep, &msg);
    sys_recv(vfs_ep, &mut msg);
    
    if msg.rdi != 0 {
        sys_log("[Storage Test] VFS_READ failed!");
        userspace::sys_exit();
    }
    
    // 4. Verify
    let mut success = true;
    for i in 0..signature.len() {
        if shm_buf[i] != signature[i] {
            success = false;
            break;
        }
    }
    
    if success {
        sys_log("==========================================");
        sys_log("[Storage Test] SUCCESS: Disk Persistence Verified!");
        sys_log("==========================================");
    } else {
        sys_log("==========================================");
        sys_log("[Storage Test] FAILED: Signature mismatch.");
        sys_log("==========================================");
    }
    
    userspace::sys_exit();
}
