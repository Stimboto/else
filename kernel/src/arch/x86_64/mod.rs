pub mod cpu;
pub mod gdt;
pub mod idt;
pub mod interrupts;
pub mod timer;
pub mod acpi;
pub mod apic;
pub mod ioapic;

pub fn init(rsdp_addr: u64) {
    log::info!("[BOOT] Initializing x86_64 architecture...");
    gdt::init();
    cpu::init_per_cpu();
    idt::init();
    
    if rsdp_addr != 0 {
        acpi::init(rsdp_addr);
        apic::init();
        ioapic::init();
    }
    
    interrupts::init();
    timer::init();
    
    // Interrupts will be enabled at the end of kernel/src/main.rs
    // once all user threads and capabilities are set up.
    
    if rsdp_addr != 0 {
        apic::calibrate_timer();
        interrupts::disable_pic();
        ioapic::route_irq(1, 33, 0); // Route Keyboard (IRQ1) to Vector 33
        ioapic::route_irq(14, 46, 0); // Route ATA Primary (IRQ14) to Vector 46
    }
}
