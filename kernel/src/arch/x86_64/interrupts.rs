use pic8259::ChainedPics;
use spin::Mutex;

// Remap PIC interrupts to avoid colliding with CPU exceptions (0-31)
pub const PIC_1_OFFSET: u8 = 32;
pub const PIC_2_OFFSET: u8 = PIC_1_OFFSET + 8;

// SAFE: Offsets are correctly mapped outside the exception range.
// spin::Mutex is used as it operates without the standard library.
pub static PICS: Mutex<ChainedPics> = Mutex::new(unsafe { ChainedPics::new(PIC_1_OFFSET, PIC_2_OFFSET) });

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum InterruptIndex {
    Timer = PIC_1_OFFSET,
    Keyboard,
}

impl InterruptIndex {
    pub fn as_u8(self) -> u8 {
        self as u8
    }
    
    pub fn as_usize(self) -> usize {
        usize::from(self.as_u8())
    }
}

pub fn init() {
    log::info!("[INTERRUPT] Initializing 8259 PIC (Phase 2 transitional)...");
    unsafe { PICS.lock().initialize() };
}

pub fn disable_pic() {
    unsafe {
        PICS.lock().write_masks(0xFF, 0xFF);
    }
}
