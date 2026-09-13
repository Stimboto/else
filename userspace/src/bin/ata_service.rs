#![no_std]
#![no_main]

use userspace::{sys_log, sys_wait_irq, sys_port_in, sys_port_out, sys_recv, sys_send, sys_map_phys, Message};

pub const MSG_BLOCK_READ: u64 = 1;
pub const MSG_BLOCK_WRITE: u64 = 2;
pub const MSG_BLOCK_FLUSH: u64 = 3;

// We use 28-bit PIO
fn ata_wait_ready(port_handle: usize) {
    loop {
        let status = sys_port_in(port_handle, 0x1F7, 1) as u8;
        if (status & 0x80) == 0 && (status & 0x40) != 0 {
            break;
        }
    }
}

fn ata_read_sector(port_handle: usize, irq_handle: usize, lba: u32, buf: &mut [u8]) {
    ata_wait_ready(port_handle);
    sys_port_out(port_handle, 0x1F6, 1, 0xE0 | ((lba >> 24) & 0x0F));
    sys_port_out(port_handle, 0x1F2, 1, 1);
    sys_port_out(port_handle, 0x1F3, 1, lba & 0xFF);
    sys_port_out(port_handle, 0x1F4, 1, (lba >> 8) & 0xFF);
    sys_port_out(port_handle, 0x1F5, 1, (lba >> 16) & 0xFF);
    sys_port_out(port_handle, 0x1F7, 1, 0x20); // READ SECTORS
    
    // Wait for IRQ 14
    sys_wait_irq(irq_handle);
    
    for i in 0..256 {
        let word = sys_port_in(port_handle, 0x1F0, 2) as u16;
        buf[i * 2] = (word & 0xFF) as u8;
        buf[i * 2 + 1] = (word >> 8) as u8;
    }
}

fn ata_write_sector(port_handle: usize, irq_handle: usize, lba: u32, buf: &[u8]) {
    ata_wait_ready(port_handle);
    sys_port_out(port_handle, 0x1F6, 1, 0xE0 | ((lba >> 24) & 0x0F));
    sys_port_out(port_handle, 0x1F2, 1, 1);
    sys_port_out(port_handle, 0x1F3, 1, lba & 0xFF);
    sys_port_out(port_handle, 0x1F4, 1, (lba >> 8) & 0xFF);
    sys_port_out(port_handle, 0x1F5, 1, (lba >> 16) & 0xFF);
    sys_port_out(port_handle, 0x1F7, 1, 0x30); // WRITE SECTORS
    
    ata_wait_ready(port_handle); // Wait for DRQ before writing
    for i in 0..256 {
        let word = (buf[i * 2] as u16) | ((buf[i * 2 + 1] as u16) << 8);
        sys_port_out(port_handle, 0x1F0, 2, word as u32);
    }
    
    // Wait for IRQ 14
    sys_wait_irq(irq_handle);
}

fn ata_flush(port_handle: usize, irq_handle: usize) {
    ata_wait_ready(port_handle);
    sys_port_out(port_handle, 0x1F7, 1, 0xE7); // CACHE FLUSH
    sys_wait_irq(irq_handle);
}

#[unsafe(no_mangle)]
pub extern "C" fn main() {
    sys_log("[ATA] Service starting...");
    
    let irq_handle = 4;
    let port_handle = 5;
    let ata_ep = 8; // Created in init
    let shm_handle = 10; // Created in init
    
    // Map shared memory (pass u64::MAX to let kernel pick actual phys and len)
    let shm_virt = sys_map_phys(shm_handle, core::u64::MAX, core::u64::MAX);
    if shm_virt == 0 {
        sys_log("[ATA] Failed to map shared memory!");
        userspace::sys_exit();
    }
    
    let shm_buf = unsafe { core::slice::from_raw_parts_mut(shm_virt as *mut u8, 4096) };
    sys_log("[ATA] Initialized successfully. Listening for IPC...");
    
    let mut msg = Message { rdi: 0, rsi: 0, rdx: 0, r10: 0, r8: 0, r9: 0 };
    loop {
        if sys_recv(ata_ep, &mut msg) == 0 {
            let cmd = msg.rdi;
            let lba = msg.rsi as u32;
            let offset = msg.rdx as usize;
            
            // Bounds check shared memory offset
            if offset + 512 > 4096 {
                // Out of bounds
                msg.rdi = 1; // error
                sys_send(ata_ep, &msg);
                continue;
            }
            
            // Bounds check LBA (10MB disk ~ 20480 sectors)
            if lba >= 20480 {
                msg.rdi = 1; // error
                sys_send(ata_ep, &msg);
                continue;
            }
            
            match cmd {
                MSG_BLOCK_READ => {
                    ata_read_sector(port_handle, irq_handle, lba, &mut shm_buf[offset..offset+512]);
                    msg.rdi = 0; // success
                }
                MSG_BLOCK_WRITE => {
                    ata_write_sector(port_handle, irq_handle, lba, &shm_buf[offset..offset+512]);
                    msg.rdi = 0; // success
                }
                MSG_BLOCK_FLUSH => {
                    ata_flush(port_handle, irq_handle);
                    msg.rdi = 0; // success
                }
                _ => {
                    msg.rdi = 1; // unknown cmd
                }
            }
            // Reply
            sys_send(ata_ep, &msg);
        }
    }
}
