#![no_std]
#![no_main]

use userspace::{sys_log, sys_ep_create, sys_spawn, sys_recv, Message};

#[unsafe(no_mangle)]
pub extern "C" fn main() {
    sys_log("[init] Supervisor process starting.");
    
    // Create an endpoint for services to talk to us
    let ep = sys_ep_create();
    sys_log("[init] Created Endpoint capability. Handle:");
    userspace::sys_log_num(ep);
    
    // Create an endpoint for keyboard -> terminal communication
    let kbd_ep = sys_ep_create();
    sys_log("[init] Created Keyboard Endpoint capability. Handle:");
    userspace::sys_log_num(kbd_ep);

    // Create an endpoint for ata_service -> fat16_service communication
    let ata_ep = sys_ep_create();
    sys_log("[init] Created ATA Endpoint capability. Handle:");
    userspace::sys_log_num(ata_ep);
    
    // Create an endpoint for fat16_service -> apps communication
    let vfs_ep = sys_ep_create();
    sys_log("[init] Created VFS Endpoint capability. Handle:");
    userspace::sys_log_num(vfs_ep);
    
    // Allocate shared memory frame for ATA <-> FAT16 block transfers
    let shm_handle = userspace::sys_frame_alloc(4096);
    sys_log("[init] Created Shared Memory capability for Block Cache. Handle:");
    userspace::sys_log_num(shm_handle);
    
    // Create Network Service Endpoints
    let rtl_ep = sys_ep_create(); // Handle 11: CAP_NIC_DRIVER_EP
    sys_log("[init] Created NIC Driver Endpoint capability. Handle:");
    userspace::sys_log_num(rtl_ep);
    
    let net_ep = sys_ep_create(); // Handle 12: CAP_NETWORK_SERVICE_EP
    sys_log("[init] Created Network Service Endpoint capability. Handle:");
    userspace::sys_log_num(net_ep);
    
    let test_ep = sys_ep_create(); // Handle 13: CAP_APPLICATION_EP
    sys_log("[init] Created Application Endpoint capability. Handle:");
    userspace::sys_log_num(test_ep);
    
    let net_shm = userspace::sys_frame_alloc(4096); // Handle 14: CAP_PACKET_BUFFER
    sys_log("[init] Created Network Packet Buffer Shared Memory. Handle:");
    userspace::sys_log_num(net_shm);
    
    sys_log("[init] Spawning terminal_service...");
    let _ts_pid = sys_spawn("terminal_service", core::usize::MAX);
    
    sys_log("[init] Spawning keyboard_service with hardware capabilities...");
    let ks_pid = sys_spawn("keyboard_service", core::usize::MAX);
    userspace::sys_log_num(ks_pid);
    
    sys_log("[init] Spawning ata_service with PortIO/IRQ14...");
    let ata_pid = sys_spawn("ata_service", core::usize::MAX);
    userspace::sys_log_num(ata_pid);
    
    sys_log("[init] Spawning fat16_service...");
    let fat_pid = sys_spawn("fat16_service", core::usize::MAX);
    userspace::sys_log_num(fat_pid);
    
    sys_log("[init] Spawning storage_test...");
    let test_pid = sys_spawn("storage_test", core::usize::MAX);
    userspace::sys_log_num(test_pid);
    
    sys_log("[init] Spawning rtl8139_service...");
    let nic_pid = sys_spawn("rtl8139_service", core::usize::MAX);
    userspace::sys_log_num(nic_pid);
    
    sys_log("[init] Spawning network_service...");
    let net_pid = sys_spawn("network_service", core::usize::MAX);
    userspace::sys_log_num(net_pid);
    
    sys_log("[init] Spawning network_test...");
    let ntest_pid = sys_spawn("network_test", core::usize::MAX);
    userspace::sys_log_num(ntest_pid);
    
    // Listen for messages on `ep` (from any process that uses it)
    let mut msg = Message { rdi: 0, rsi: 0, rdx: 0, r10: 0, r8: 0, r9: 0 };
    sys_log("[init] Waiting for IPC messages...");
    
    loop {
        sys_recv(ep, &mut msg);
        sys_log("[init] Received unhandled message on main ep.");
    }
}
