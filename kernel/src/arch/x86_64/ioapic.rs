use core::ptr::{read_volatile, write_volatile};

fn ioapic_read(ioapic_virt: u64, reg: u8) -> u32 {
    unsafe {
        write_volatile(ioapic_virt as *mut u32, reg as u32);
        read_volatile((ioapic_virt + 0x10) as *const u32)
    }
}

fn ioapic_write(ioapic_virt: u64, reg: u8, value: u32) {
    unsafe {
        write_volatile(ioapic_virt as *mut u32, reg as u32);
        write_volatile((ioapic_virt + 0x10) as *mut u32, value);
    }
}

pub fn init() {
    let ioapic_phys = unsafe { crate::arch::x86_64::acpi::IOAPIC_ADDR };
    if ioapic_phys == 0 {
        log::warn!("[IOAPIC] IOAPIC address is 0, cannot initialize!");
        return;
    }
    
    let hhdm = crate::memory::hhdm_offset();
    let ioapic_virt = ioapic_phys + hhdm;
    
    // Ensure the IOAPIC memory is mapped in the HHDM
    crate::memory::page_table::ensure_mapped(ioapic_phys, 4096);
    
    let ioapic_ver = ioapic_read(ioapic_virt, 0x01);
    let max_intr = (ioapic_ver >> 16) & 0xFF;
    
    log::info!("[IOAPIC] Initializing IOAPIC at {:#x}, Max Interrupts: {}", ioapic_virt, max_intr + 1);
    
    // Mask all interrupts initially
    for i in 0..=max_intr {
        let reg = 0x10 + (i * 2) as u8;
        ioapic_write(ioapic_virt, reg, 0x10000); // Set mask bit (bit 16)
        ioapic_write(ioapic_virt, reg + 1, 0);
    }
}

pub fn route_irq(irq: u8, vector: u8, apic_id: u8) {
    let ioapic_phys = unsafe { crate::arch::x86_64::acpi::IOAPIC_ADDR };
    if ioapic_phys == 0 { return; }
    
    let hhdm = crate::memory::hhdm_offset();
    let ioapic_virt = ioapic_phys + hhdm;
    
    let reg = 0x10 + (irq * 2) as u8;
    
    // Lower 32 bits: Vector, Delivery Mode (0 = Fixed), Destination Mode (0 = Physical),
    // Polarity (0 = High), Trigger Mode (0 = Edge), Mask (0 = Unmasked)
    let low = vector as u32;
    
    // Upper 32 bits: Destination (APIC ID)
    let high = (apic_id as u32) << 24;
    
    ioapic_write(ioapic_virt, reg + 1, high);
    ioapic_write(ioapic_virt, reg, low);
    
    let read_low = ioapic_read(ioapic_virt, reg);
    let read_high = ioapic_read(ioapic_virt, reg + 1);
    
    log::info!("[IOAPIC] Routed IRQ {} to Vector {} on APIC {}. Reg {:#x}: {:#010x}_{:#010x}", 
               irq, vector, apic_id, reg, read_high, read_low);
}
