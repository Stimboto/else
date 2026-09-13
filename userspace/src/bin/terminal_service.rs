#![no_std]
#![no_main]

use userspace::{sys_log, sys_recv, sys_map_phys, sys_framebuffer_info, FramebufferInfo, Message, KeyEvent, rdtsc};

// Extremely minimal 8x8 font for demonstration
const FONT_A: [u8; 8] = [0x3c, 0x66, 0x66, 0x7e, 0x66, 0x66, 0x66, 0x00];
const FONT_0: [u8; 8] = [0x3c, 0x66, 0x66, 0x66, 0x66, 0x66, 0x3c, 0x00];
const FONT_SPACE: [u8; 8] = [0x00; 8];
const FONT_UNKNOWN: [u8; 8] = [0xff; 8]; // Solid block for unknown

fn get_glyph(c: u8) -> &'static [u8; 8] {
    match c {
        b'a'..=b'z' | b'A'..=b'Z' => &FONT_A,
        b'0'..=b'9' => &FONT_0,
        b' ' => &FONT_SPACE,
        _ => &FONT_UNKNOWN,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn main() {
    sys_log("[Terminal] Service starting...");
    
    let mut fb_info = FramebufferInfo { physical_address: 0, width: 0, height: 0, pitch: 0, bpp: 0, size: 0 };
    if sys_framebuffer_info(&mut fb_info) != 0 {
        sys_log("[Terminal] Failed to get framebuffer info!");
        userspace::sys_exit();
    }
    
    sys_log("[Terminal] Mapping framebuffer...");
    // Map framebuffer (requires MemoryMap capability at Handle 3)
    let fb_virt = sys_map_phys(3, fb_info.physical_address, fb_info.size);
    if fb_virt == 0 {
        sys_log("[Terminal] Failed to map framebuffer!");
        userspace::sys_exit();
    }
    
    let fb_ptr = fb_virt as *mut u8;
    
    // Clear screen
    unsafe {
        core::ptr::write_bytes(fb_ptr, 0, fb_info.size as usize);
    }
    
    let mut cursor_x = 10;
    let mut cursor_y = 10;
    
    // kbd_ep is handle 4
    let ep_handle = 4;
    let mut msg = Message { rdi: 0, rsi: 0, rdx: 0, r10: 0, r8: 0, r9: 0 };
    
    sys_log("[Terminal] Listening for keystrokes...");
    loop {
        if sys_recv(ep_handle, &mut msg) == 0 {
            sys_log("[Terminal] Received keystroke IPC!");
            let tsc_recv = rdtsc();
            
            if let Some((event, tsc_send)) = KeyEvent::from_message(&msg) {
                if event.pressed == 1 {
                    let diff = tsc_recv.saturating_sub(tsc_send);
                    // Draw the character basic log for verification
                    // Manually format integer into string
                    let mut buf = [b'0'; 20];
                    let mut temp = diff;
                    let mut i = 19;
                    if temp == 0 {
                        buf[i] = b'0';
                    } else {
                        while temp > 0 && i > 0 {
                            buf[i] = b'0' + (temp % 10) as u8;
                            temp /= 10;
                            i -= 1;
                        }
                    }
                    
                    sys_log("[Terminal] KeyEvent received!");
                    unsafe {
                        let mut msg_buf = [b'L', b'a', b't', b'e', b'n', b'c', b'y', b':', b' ', 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
                        let mut j = 9;
                        for k in (i + 1)..=19 {
                            msg_buf[j] = buf[k];
                            j += 1;
                        }
                        sys_log(core::str::from_utf8_unchecked(&msg_buf[..j]));
                    }
                    
                    if event.key == b'\n' as u64 {
                        cursor_x = 10;
                        cursor_y += 10;
                    } else if event.key == 0x08 { // Backspace
                        if cursor_x > 10 {
                            cursor_x -= 8;
                            // draw space
                            draw_char(fb_ptr, &fb_info, cursor_x, cursor_y, b' ');
                        }
                    } else if event.key >= 32 && event.key <= 126 {
                        draw_char(fb_ptr, &fb_info, cursor_x, cursor_y, event.key as u8);
                        cursor_x += 8;
                        if cursor_x + 8 > fb_info.width {
                            cursor_x = 10;
                            cursor_y += 10;
                        }
                    }
                }
            }
        }
    }
}

fn draw_char(fb_ptr: *mut u8, info: &FramebufferInfo, x: u64, y: u64, c: u8) {
    let glyph = get_glyph(c);
    let bytes_per_pixel = (info.bpp / 8) as u64;
    
    for row in 0..8 {
        let row_data = glyph[row as usize];
        for col in 0..8 {
            if (row_data & (1 << (7 - col))) != 0 {
                // Draw pixel (white)
                let offset = (y + row) * info.pitch + (x + col) * bytes_per_pixel;
                if offset + bytes_per_pixel <= info.size {
                    unsafe {
                        core::ptr::write(fb_ptr.add(offset as usize) as *mut u32, 0x00FFFFFF);
                    }
                }
            } else {
                // Draw background (black)
                let offset = (y + row) * info.pitch + (x + col) * bytes_per_pixel;
                if offset + bytes_per_pixel <= info.size {
                    unsafe {
                        core::ptr::write(fb_ptr.add(offset as usize) as *mut u32, 0x00000000);
                    }
                }
            }
        }
    }
}
