#![no_std]
#![no_main]

use userspace::{sys_log, sys_wait_irq, sys_port_in, sys_send, rdtsc, KeyEvent};

#[unsafe(no_mangle)]
pub extern "C" fn main() {
    sys_log("[Keyboard] Service starting...");

    // Capabilities granted via init's cloned CSpace
    let irq_handle = 1;
    let port_handle = 2;
    let ep_handle = 4; // kbd_ep from init.rs

    let mut shift_pressed = false;

    // Flush any pending byte in the PS/2 controller buffer
    // so the edge-triggered IRQ line goes LOW and can fire again.
    // Read status from 0x64. Bit 0 indicates output buffer status (1 = full)
    loop {
        let status = sys_port_in(port_handle, 0x64, 1) as u8;
        if (status & 1) == 0 {
            break;
        }
        let _ = sys_port_in(port_handle, 0x60, 1);
    }
    sys_log("[Keyboard] Flushed PS/2 buffer.");
    
    loop {
        sys_wait_irq(irq_handle);
        let tsc = rdtsc();
        
        let scancode = sys_port_in(port_handle, 0x60, 1) as u8;
        sys_log("[Keyboard] IRQ received! Scancode read.");
        
        if scancode == 0xE0 {
            // We ignore E0 for this basic implementation
            continue;
        }

        let released = (scancode & 0x80) != 0;
        let code = scancode & 0x7F;
        
        if code == 0x2A || code == 0x36 {
            shift_pressed = !released;
            continue;
        }
        
        if !released {
            let key: u64 = match code {
                0x02..=0x0A => (b'1' + code - 0x02) as u64, // 1-9
                0x0B => b'0' as u64,
                0x0E => 0x08, // Backspace
                0x0F => b'\t' as u64,
                0x10 => if shift_pressed { b'Q' as u64 } else { b'q' as u64 },
                0x11 => if shift_pressed { b'W' as u64 } else { b'w' as u64 },
                0x12 => if shift_pressed { b'E' as u64 } else { b'e' as u64 },
                0x13 => if shift_pressed { b'R' as u64 } else { b'r' as u64 },
                0x14 => if shift_pressed { b'T' as u64 } else { b't' as u64 },
                0x15 => if shift_pressed { b'Y' as u64 } else { b'y' as u64 },
                0x16 => if shift_pressed { b'U' as u64 } else { b'u' as u64 },
                0x17 => if shift_pressed { b'I' as u64 } else { b'i' as u64 },
                0x18 => if shift_pressed { b'O' as u64 } else { b'o' as u64 },
                0x19 => if shift_pressed { b'P' as u64 } else { b'p' as u64 },
                0x1C => b'\n' as u64, // Enter
                0x1E => if shift_pressed { b'A' as u64 } else { b'a' as u64 },
                0x1F => if shift_pressed { b'S' as u64 } else { b's' as u64 },
                0x20 => if shift_pressed { b'D' as u64 } else { b'd' as u64 },
                0x21 => if shift_pressed { b'F' as u64 } else { b'f' as u64 },
                0x22 => if shift_pressed { b'G' as u64 } else { b'g' as u64 },
                0x23 => if shift_pressed { b'H' as u64 } else { b'h' as u64 },
                0x24 => if shift_pressed { b'J' as u64 } else { b'j' as u64 },
                0x25 => if shift_pressed { b'K' as u64 } else { b'k' as u64 },
                0x26 => if shift_pressed { b'L' as u64 } else { b'l' as u64 },
                0x2C => if shift_pressed { b'Z' as u64 } else { b'z' as u64 },
                0x2D => if shift_pressed { b'X' as u64 } else { b'x' as u64 },
                0x2E => if shift_pressed { b'C' as u64 } else { b'c' as u64 },
                0x2F => if shift_pressed { b'V' as u64 } else { b'v' as u64 },
                0x30 => if shift_pressed { b'B' as u64 } else { b'b' as u64 },
                0x31 => if shift_pressed { b'N' as u64 } else { b'n' as u64 },
                0x32 => if shift_pressed { b'M' as u64 } else { b'm' as u64 },
                0x39 => b' ' as u64,
                _ => 0,
            };
            
            if key != 0 {
                let event = KeyEvent {
                    key,
                    pressed: 1,
                    modifiers: if shift_pressed { 1 } else { 0 },
                };
                
                let msg = event.to_message(tsc);
                sys_send(ep_handle, &msg);
                sys_log("[Keyboard] Sent keystroke IPC!");
            }
        }
    }
}
