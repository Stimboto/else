use x86_64::structures::idt::InterruptStackFrame;
use crate::arch::x86_64::interrupts::{InterruptIndex, PICS};
use core::sync::atomic::{AtomicUsize, Ordering};

use core::arch::global_asm;

// Timer variables removed for phase 4

pub fn init() {
    log::info!("[TIMER] Keeping PIT at default frequency for now...");
    
    // Unmask the timer interrupt (IRQ 0) on the Master PIC
    unsafe {
        let mut pics = PICS.lock();
        let (mut master, slave) = (pics.read_masks()[0], pics.read_masks()[1]);
        master &= !1; // Clear bit 0
        pics.write_masks(master, slave);
    }
}

pub fn disable_pit_interrupts() {
    unsafe {
        let mut pics = PICS.lock();
        let (mut master, slave) = (pics.read_masks()[0], pics.read_masks()[1]);
        master |= 1; // Mask bit 0 (IRQ 0)
        pics.write_masks(master, slave);
    }
}

pub fn pit_wait_10ms() {
    use x86_64::instructions::port::Port;
    let mut port_61: Port<u8> = Port::new(0x61);
    let mut pit_cmd: Port<u8> = Port::new(0x43);
    let mut pit_ch2: Port<u8> = Port::new(0x42);
    
    unsafe {
        // Disable PC speaker
        let prev = port_61.read();
        port_61.write(prev & 0xFC);
        
        // PIT Channel 2, Mode 0, Binary
        // 1193182 Hz -> 10 ms = 11931 ticks
        pit_cmd.write(0b10110000);
        pit_ch2.write((11931 & 0xFF) as u8); // LSB
        pit_ch2.write(((11931 >> 8) & 0xFF) as u8); // MSB
        
        // Enable PC speaker counter
        let prev = port_61.read();
        port_61.write((prev & 0xFE) | 1);
        
        // Wait until bit 5 is set (Timer 2 output goes high)
        while (port_61.read() & 0x20) == 0 {
            core::hint::spin_loop();
        }
    }
}
