#![no_std]
#![no_main]

use core::panic::PanicInfo;
use core::arch::asm;

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    sys_log("User panic!");
    sys_exit();
}

unsafe extern "Rust" {
    fn main();
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    unsafe { main(); }
    sys_exit();
}

#[inline(always)]
fn syscall(sys_num: u64, arg1: u64, arg2: u64, arg3: u64, arg4: u64, arg5: u64, arg6: u64) -> u64 {
    let ret: u64;
    unsafe {
        asm!(
            "syscall",
            inout("rax") sys_num => ret,
            in("rdi") arg1,
            in("rsi") arg2,
            in("rdx") arg3,
            in("r10") arg4,
            in("r8") arg5,
            in("r9") arg6,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
    }
    ret
}

pub fn sys_yield() {
    syscall(0, 0, 0, 0, 0, 0, 0);
}

pub fn sys_exit() -> ! {
    syscall(1, 0, 0, 0, 0, 0, 0);
    loop {}
}

pub fn sys_log(msg: &str) {
    syscall(2, msg.as_ptr() as u64, msg.len() as u64, 0, 0, 0, 0);
}

pub fn sys_log_num(mut num: usize) {
    if num == 0 {
        sys_log("0");
        return;
    }
    let mut buf = [0u8; 32];
    let mut i = 32;
    while num > 0 {
        i -= 1;
        buf[i] = (num % 10) as u8 + b'0';
        num /= 10;
    }
    let s = unsafe { core::str::from_utf8_unchecked(&buf[i..]) };
    sys_log(s);
}

pub fn sys_ep_create() -> usize {
    syscall(3, 0, 0, 0, 0, 0, 0) as usize
}

// Ensure the Message structure layout matches the kernel's exactly.
#[repr(C)]
pub struct Message {
    pub rdi: u64,
    pub rsi: u64,
    pub rdx: u64,
    pub r10: u64,
    pub r8: u64,
    pub r9: u64,
}

pub fn sys_send(handle: usize, msg: &Message) -> isize {
    syscall(4, handle as u64, msg as *const _ as u64, 0, 0, 0, 0) as isize
}

pub fn sys_recv(handle: usize, msg: &mut Message) -> isize {
    syscall(5, handle as u64, msg as *mut _ as u64, 0, 0, 0, 0) as isize
}

pub fn sys_spawn(name: &str, ep_handle: usize) -> usize {
    syscall(6, name.as_ptr() as u64, name.len() as u64, ep_handle as u64, 0, 0, 0) as usize
}

pub fn sys_wait(process_handle: usize) -> isize {
    syscall(7, process_handle as u64, 0, 0, 0, 0, 0) as isize
}

pub fn sys_wait_irq(handle: usize) -> isize {
    syscall(8, handle as u64, 0, 0, 0, 0, 0) as isize
}

pub fn sys_port_in(handle: usize, port: u16, size: u8) -> u64 {
    syscall(9, handle as u64, port as u64, size as u64, 0, 0, 0)
}

pub fn sys_port_out(handle: usize, port: u16, size: u8, val: u32) {
    syscall(10, handle as u64, port as u64, size as u64, val as u64, 0, 0);
}

pub fn sys_map_phys(handle: usize, phys_addr: u64, length: u64) -> u64 {
    syscall(11, handle as u64, phys_addr, length, 0, 0, 0)
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct FramebufferInfo {
    pub physical_address: u64,
    pub width: u64,
    pub height: u64,
    pub pitch: u64,
    pub bpp: u16,
    pub size: u64,
}

pub fn sys_framebuffer_info(info: &mut FramebufferInfo) -> isize {
    syscall(12, info as *mut _ as u64, 0, 0, 0, 0, 0) as isize
}

pub fn sys_frame_alloc(size: u64) -> usize {
    syscall(13, size, 0, 0, 0, 0, 0) as usize
}

pub fn sys_dma_alloc(size: u64) -> usize {
    syscall(14, size, 0, 0, 0, 0, 0) as usize
}

pub fn sys_cap_info(handle: usize) -> u64 {
    syscall(15, handle as u64, 0, 0, 0, 0, 0)
}

pub fn sys_thread_spawn(entry_point: extern "C" fn(), user_stack: usize) -> u64 {
    syscall(16, entry_point as u64, user_stack as u64, 0, 0, 0, 0)
}

// Logical Bootstrap Capabilities (Phase 11 ABI)
pub const CAP_NIC_DRIVER_EP: usize = 11;
pub const CAP_NETWORK_SERVICE_EP: usize = 12;
pub const CAP_APPLICATION_EP: usize = 13;
pub const CAP_PACKET_BUFFER: usize = 14;

pub const MSG_KEY_EVENT: u64 = 1;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct KeyEvent {
    pub key: u64, // ASCII or special keycode
    pub pressed: u64, // 1 for pressed, 0 for released
    pub modifiers: u64, // Bitmask
}

impl KeyEvent {
    pub fn to_message(&self, rdtsc: u64) -> Message {
        Message {
            rdi: MSG_KEY_EVENT,
            rsi: self.key,
            rdx: self.pressed,
            r10: self.modifiers,
            r8: rdtsc,
            r9: 0,
        }
    }
    
    pub fn from_message(msg: &Message) -> Option<(Self, u64)> {
        if msg.rdi == MSG_KEY_EVENT {
            Some((KeyEvent {
                key: msg.rsi,
                pressed: msg.rdx,
                modifiers: msg.r10,
            }, msg.r8))
        } else {
            None
        }
    }
}

#[inline(always)]
pub fn rdtsc() -> u64 {
    let eax: u32;
    let edx: u32;
    unsafe {
        asm!("rdtsc", out("eax") eax, out("edx") edx, options(nomem, nostack, preserves_flags));
    }
    ((edx as u64) << 32) | (eax as u64)
}
