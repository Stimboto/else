#![no_std]
#![no_main]

use userspace::{sys_log, sys_cap_info, sys_port_in, sys_port_out, sys_map_phys, sys_wait_irq, sys_thread_spawn, sys_send, Message, CAP_NIC_DRIVER_EP, CAP_NETWORK_SERVICE_EP, CAP_PACKET_BUFFER};

// RTL8139 Registers
const REG_MAC0: u16 = 0x00;
const REG_TX_STATUS0: u16 = 0x10;
const REG_TX_ADDR0: u16 = 0x20;
const REG_RX_BUF: u16 = 0x30;
const REG_COMMAND: u16 = 0x37;
const REG_CAPR: u16 = 0x38;
const REG_IMR: u16 = 0x3C;
const REG_ISR: u16 = 0x3E;
const REG_RCR: u16 = 0x44;
const REG_CONFIG1: u16 = 0x52;

pub const MSG_NET_RX: u64 = 100;
pub const MSG_NET_TX: u64 = 101;
pub const MSG_NET_TX_ACK: u64 = 102;

static mut IO_BASE: u16 = 0;
static mut TX_BUF_VIRT: u64 = 0;
static mut TX_BUF_PHYS: u64 = 0;
static mut NEXT_TX_DESC: u8 = 0;
static mut SHARED_BUF_VIRT: u64 = 0;

#[unsafe(no_mangle)]
pub extern "C" fn tx_thread() {
    userspace::sys_log("[RTL8139-TX] Thread started.");
    let mut msg = Message { rdi: 0, rsi: 0, rdx: 0, r10: 0, r8: 0, r9: 0 };
    loop {
        // Wait for a TX request on CAP_NIC_DRIVER_EP
        userspace::sys_recv(CAP_NIC_DRIVER_EP, &mut msg);
        if msg.rdi == MSG_NET_TX {
            userspace::sys_log("[RTL8139-TX] Received TX request.");
            let length = msg.rsi as usize;
            
            unsafe {
                let io_base = IO_BASE;
                let tx_buf_virt = TX_BUF_VIRT;
                let tx_buf_phys = TX_BUF_PHYS;
                let shared_buf_virt = SHARED_BUF_VIRT;
                let desc = NEXT_TX_DESC;
                
                // Copy from Shared Buffer (offset 2048) to TX DMA buffer
                let src_ptr = (shared_buf_virt + 2048) as *const u8;
                let dst_ptr = (tx_buf_virt + (desc as u64 * 2048)) as *mut u8;
                
                core::ptr::copy_nonoverlapping(src_ptr, dst_ptr, length);
                
                // Pad to 60 bytes if necessary
                if length < 60 {
                    core::ptr::write_bytes(dst_ptr.add(length), 0, 60 - length);
                }
                
                let send_len = if length < 60 { 60 } else { length };
                
                // Write physical address to TX_ADDR
                sys_port_out(21, io_base + REG_TX_ADDR0 + (desc as u16 * 4), 4, (tx_buf_phys + (desc as u64 * 2048)) as u32);
                // Send it!
                sys_port_out(21, io_base + REG_TX_STATUS0 + (desc as u16 * 4), 4, send_len as u32);
                
                userspace::sys_log("[RTL8139-TX] Transmitted packet.");
                NEXT_TX_DESC = (desc + 1) % 4;
            }
            
            // Send ACK back to the sender
            let ack_msg = Message { rdi: MSG_NET_TX_ACK, rsi: 0, rdx: 0, r10: 0, r8: 0, r9: 0 };
            userspace::sys_send(CAP_NETWORK_SERVICE_EP, &ack_msg);
        }
    }
}

// Global stack for the TX thread
static mut TX_THREAD_STACK: [u8; 8192] = [0; 8192];

#[unsafe(no_mangle)]
pub extern "C" fn main() {
    sys_log("[RTL8139] Starting service...");

    let io_base = sys_cap_info(21) as u16;
    let dma_phys = sys_cap_info(22);

    if io_base == 0xFFFF {
        sys_log("[RTL8139] Error: Capability 21 missing or invalid");
        loop {}
    }

    let dma_virt = sys_map_phys(22, core::u64::MAX, core::u64::MAX);
    if dma_virt == 0 {
        sys_log("[RTL8139] Failed to map DMA pool");
        loop {}
    }
    
    let shared_buf_virt = sys_map_phys(CAP_PACKET_BUFFER, core::u64::MAX, core::u64::MAX);
    if shared_buf_virt == 0 {
        sys_log("[RTL8139] Failed to map CAP_PACKET_BUFFER");
        loop {}
    }

    unsafe {
        IO_BASE = io_base;
        TX_BUF_VIRT = dma_virt + 16384;
        TX_BUF_PHYS = dma_phys + 16384;
        SHARED_BUF_VIRT = shared_buf_virt;
    }
    
    let rx_ring_phys = dma_phys;

    sys_log("[RTL8139] Initializing hardware...");
    sys_port_out(21, io_base + REG_CONFIG1, 1, 0x00);
    sys_port_out(21, io_base + REG_COMMAND, 1, 0x10);
    loop {
        if (sys_port_in(21, io_base + REG_COMMAND, 1) & 0x10) == 0 {
            break;
        }
    }
    sys_port_out(21, io_base + REG_RX_BUF, 4, rx_ring_phys as u32);
    sys_port_out(21, io_base + REG_IMR, 2, 0x0005);
    sys_port_out(21, io_base + REG_RCR, 4, 0x8F);
    sys_port_out(21, io_base + REG_COMMAND, 1, 0x0C);

    let mut mac = [0u8; 6];
    for i in 0..6 {
        mac[i] = sys_port_in(21, io_base + REG_MAC0 + i as u16, 1) as u8;
    }
    
    unsafe {
        let stack_top = core::ptr::addr_of_mut!(TX_THREAD_STACK) as usize + 8192;
        let tid = sys_thread_spawn(tx_thread, stack_top);
        sys_log("[RTL8139] Spawned TX thread ID:");
        userspace::sys_log_num(tid as usize);
    }
    
    let mac_msg = Message {
        rdi: MSG_NET_RX,
        rsi: 0xFFFFFFFF, // Special length to indicate MAC address
        rdx: ((mac[0] as u64) << 40) | ((mac[1] as u64) << 32) | ((mac[2] as u64) << 24) | ((mac[3] as u64) << 16) | ((mac[4] as u64) << 8) | (mac[5] as u64),
        r10: 0,
        r8: 0,
        r9: 0,
    };
    sys_send(CAP_NETWORK_SERVICE_EP, &mac_msg);

    let mut rx_offset = 0;

    loop {
        sys_wait_irq(20);
        
        let target_port = io_base + REG_ISR;
        let status = sys_port_in(21, target_port, 2);
        
        if (status & 0x04) != 0 {
            sys_port_out(21, target_port, 2, 0x04); // Clear TOK
        }
        
        if (status & 0x01) != 0 {
            loop {
                let cmd = sys_port_in(21, io_base + REG_COMMAND, 1);
                if (cmd & 0x01) != 0 {
                    break;
                }
                
                let cur_rx_ptr = (dma_virt + rx_offset as u64) as *const u8;
                let _header = unsafe { core::ptr::read_unaligned(cur_rx_ptr as *const u16) };
                let length = unsafe { core::ptr::read_unaligned(cur_rx_ptr.add(2) as *const u16) };
                
                let packet_len = (length - 4) as usize; // Remove CRC
                
                userspace::sys_log("[RTL8139-RX] Received packet.");
                
                unsafe {
                    let dst_ptr = shared_buf_virt as *mut u8;
                    let src_ptr = cur_rx_ptr.add(4);
                    core::ptr::copy_nonoverlapping(src_ptr, dst_ptr, packet_len);
                }
                
                let rx_msg = Message {
                    rdi: MSG_NET_RX,
                    rsi: packet_len as u64,
                    rdx: 0, r10: 0, r8: 0, r9: 0
                };
                
                // Send to network service
                sys_send(CAP_NETWORK_SERVICE_EP, &rx_msg);

                rx_offset = (rx_offset + length as u32 + 4 + 3) & !3;
                if rx_offset >= 8192 {
                    rx_offset -= 8192;
                }
                sys_port_out(21, io_base + REG_CAPR, 2, (rx_offset - 16) as u32);
            }
            
            sys_port_out(21, io_base + REG_ISR, 2, 0x01); // Clear ROK
        }
    }
}
