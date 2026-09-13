use core::ptr::{read_volatile, write_volatile};

pub fn init() {
    let lapic_phys = unsafe { crate::arch::x86_64::acpi::LAPIC_ADDR };
    if lapic_phys == 0 {
        log::warn!("[APIC] LAPIC address is 0, cannot initialize!");
        return;
    }
    
    let hhdm = crate::memory::hhdm_offset();
    let lapic_virt = lapic_phys + hhdm;
    
    // Ensure the LAPIC memory is mapped in the HHDM
    crate::memory::page_table::ensure_mapped(lapic_phys, 4096);
    
    log::info!("[APIC] Initializing Local APIC at {:#x}", lapic_virt);
    
    unsafe {
        // Enable LAPIC by setting the Spurious Interrupt Vector Register
        // Bit 8 is the APIC Software Enable/Disable flag.
        //        // Spurious interrupt vector (offset 0x00F0)
        let spurious_reg = (lapic_virt + 0x00F0) as *mut u32;
        write_volatile(spurious_reg, 0x1FF); // Vector 255, bit 8 to enable
        
        let id_reg = (lapic_virt + 0x0020) as *const u32;
        let lapic_id = read_volatile(id_reg) >> 24;
        log::info!("[APIC] LAPIC initialized. LAPIC ID: {}", lapic_id);
        
        // Write 0 to Task Priority Register (TPR) to accept all interrupts
        write_volatile((lapic_virt + 0x0080) as *mut u32, 0);
    }
}

pub fn calibrate_timer() {
    let lapic_phys = unsafe { crate::arch::x86_64::acpi::LAPIC_ADDR };
    if lapic_phys == 0 { return; }
    
    let hhdm = crate::memory::hhdm_offset();
    let lapic_virt = lapic_phys + hhdm;
    
    unsafe {
        let lvt_timer = (lapic_virt + 0x0320) as *mut u32;
        let initial_count = (lapic_virt + 0x0380) as *mut u32;
        let current_count = (lapic_virt + 0x0390) as *mut u32;
        let divide_config = (lapic_virt + 0x03E0) as *mut u32;
        
        // Disable timer (Masked)
        write_volatile(lvt_timer, 0x10000);
        
        // Set divide by 16
        write_volatile(divide_config, 0x03); 
        
        // Set count to 0xFFFFFFFF
        write_volatile(initial_count, 0xFFFFFFFF);
        
        // Wait 10ms using PIT Channel 2
        crate::arch::x86_64::timer::pit_wait_10ms();
        
        // Disable LAPIC Timer while reading
        write_volatile(lvt_timer, 0x10000);
        
        // Read current count
        let count = read_volatile(current_count);
        let ticks_in_10ms = 0xFFFFFFFF - count;
        
        log::info!("[APIC] LAPIC Timer calibrated: {} ticks in 10ms", ticks_in_10ms);
        
        log::info!("[APIC] Switching scheduler preemption from PIT to LAPIC Timer...");
        crate::arch::x86_64::timer::disable_pit_interrupts();
        
        // Enable LAPIC Timer, Vector 32, Periodic mode (bit 17)
        // Vector 32 maps to InterruptIndex::Timer
        write_volatile(lvt_timer, 32 | 0x20000); 
        write_volatile(initial_count, ticks_in_10ms);
    }
}

pub fn eoi() {
    let lapic_phys = unsafe { crate::arch::x86_64::acpi::LAPIC_ADDR };
    if lapic_phys != 0 {
        let hhdm = crate::memory::hhdm_offset();
        let lapic_virt = lapic_phys + hhdm;
        unsafe {
            // Write 0 to EOI register (offset 0x00B0)
            write_volatile((lapic_virt + 0x00B0) as *mut u32, 0);
        }
    }
}
