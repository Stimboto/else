use core::slice;

#[repr(C, packed)]
struct RsdpDescriptor {
    signature: [u8; 8],
    checksum: u8,
    oemid: [u8; 6],
    revision: u8,
    rsdt_address: u32,
}

#[repr(C, packed)]
struct RsdpDescriptor20 {
    first_part: RsdpDescriptor,
    length: u32,
    xsdt_address: u64,
    extended_checksum: u8,
    reserved: [u8; 3],
}

#[repr(C, packed)]
struct SdtHeader {
    signature: [u8; 4],
    length: u32,
    revision: u8,
    checksum: u8,
    oemid: [u8; 6],
    oem_table_id: [u8; 8],
    oem_revision: u32,
    creator_id: u32,
    creator_revision: u32,
}

pub static mut LAPIC_ADDR: u64 = 0;
pub static mut IOAPIC_ADDR: u64 = 0;

pub fn init(rsdp_addr: u64) {
    let hhdm = crate::memory::hhdm_offset();
    
    // Some versions/modes of Limine return a physical address, some return a virtual address.
    let rsdp_virt = if rsdp_addr < hhdm {
        rsdp_addr + hhdm
    } else {
        rsdp_addr
    };
    
    // Ensure the RSDP memory is mapped
    let rsdp_phys = if rsdp_addr < hhdm { rsdp_addr } else { rsdp_addr - hhdm };
    crate::memory::page_table::ensure_mapped(rsdp_phys, core::mem::size_of::<RsdpDescriptor20>() as u64);
    
    log::info!("[ACPI] Parsing ACPI tables from RSDP at physical/virtual {:#x}, mapped to {:#x}", rsdp_addr, rsdp_virt);
    
    let rsdp = unsafe { &*(rsdp_virt as *const RsdpDescriptor) };
    
    // Validate RSDP signature
    if &rsdp.signature != b"RSD PTR " {
        log::error!("[ACPI] Invalid RSDP signature");
        return;
    }
    
    let mut rsdt_addr = 0;
    let mut xsdt_addr = 0;
    
    if rsdp.revision >= 2 {
        let rsdp20 = unsafe { &*(rsdp_virt as *const RsdpDescriptor20) };
        xsdt_addr = rsdp20.xsdt_address;
    } else {
        rsdt_addr = rsdp.rsdt_address as u64;
    }
    
    let is_xsdt = xsdt_addr != 0;
    let sdt_addr = if is_xsdt { xsdt_addr } else { rsdt_addr };
    
    // Ensure the SDT memory is mapped.
    crate::memory::page_table::ensure_mapped(sdt_addr, 4096);
    
    // XSDT/RSDT pointers inside RSDP are physical addresses. Add HHDM offset.
    let sdt_virt = sdt_addr + hhdm;
    
    let sdt_header = unsafe { &*(sdt_virt as *const SdtHeader) };
    
    // Now ensure the whole table is mapped
    crate::memory::page_table::ensure_mapped(sdt_addr, sdt_header.length as u64);
    
    let entries_count = if is_xsdt {
        (sdt_header.length as usize - core::mem::size_of::<SdtHeader>()) / 8
    } else {
        (sdt_header.length as usize - core::mem::size_of::<SdtHeader>()) / 4
    };
    
    let entries_ptr = (sdt_virt + core::mem::size_of::<SdtHeader>() as u64) as *const u8;
    
    let mut madt_virt = 0;
    
    for i in 0..entries_count {
        let entry_phys = if is_xsdt {
            unsafe { *(entries_ptr.add(i * 8) as *const u64) }
        } else {
            let addr32 = unsafe { *(entries_ptr.add(i * 4) as *const u32) };
            addr32 as u64
        };
        
        // Ensure the header is mapped
        crate::memory::page_table::ensure_mapped(entry_phys, 4096);
        
        let entry_virt = entry_phys + hhdm;
        let header = unsafe { &*(entry_virt as *const SdtHeader) };
        
        // Ensure the entire table is mapped
        crate::memory::page_table::ensure_mapped(entry_phys, header.length as u64);
        
        if &header.signature == b"APIC" {
            madt_virt = entry_virt;
            break;
        }
    }
    
    if madt_virt != 0 {
        parse_madt(madt_virt);
    } else {
        log::warn!("[ACPI] MADT not found! APIC cannot be initialized.");
    }
}

#[repr(C, packed)]
struct MadtHeader {
    header: SdtHeader,
    local_apic_addr: u32,
    flags: u32,
}

fn parse_madt(madt_virt: u64) {
    let madt = unsafe { &*(madt_virt as *const MadtHeader) };
    let lapic_addr = madt.local_apic_addr;
    log::info!("[ACPI] MADT found! Default LAPIC physical address: {:#x}", lapic_addr);
    
    unsafe { LAPIC_ADDR = lapic_addr as u64; }
    
    let flags = madt.flags;
    if (flags & 1) == 1 {
        // PCAT_COMPAT is set. We should use IMCR to force APIC mode.
        unsafe {
            use x86_64::instructions::port::Port;
            let mut port_22: Port<u8> = Port::new(0x22);
            let mut port_23: Port<u8> = Port::new(0x23);
            port_22.write(0x70);
            port_23.write(0x01);
        }
        log::info!("[ACPI] PCAT_COMPAT set. Sent IMCR to switch to APIC mode.");
    }
    
    let mut offset = core::mem::size_of::<MadtHeader>() as u64;
    let end = madt.header.length as u64;
    
    while offset < end {
        let entry_ptr = (madt_virt + offset) as *const u8;
        let entry_type = unsafe { *entry_ptr };
        let entry_len = unsafe { *(entry_ptr.add(1)) };
        
        match entry_type {
            1 => { // IOAPIC
                let ioapic_id = unsafe { *(entry_ptr.add(2)) };
                let ioapic_addr = unsafe { *(entry_ptr.add(4) as *const u32) };
                let gsi_base = unsafe { *(entry_ptr.add(8) as *const u32) };
                log::info!("[ACPI] Found IOAPIC {} at {:#x}, GSI Base: {}", ioapic_id, ioapic_addr, gsi_base);
                
                if unsafe { IOAPIC_ADDR } == 0 {
                    unsafe { IOAPIC_ADDR = ioapic_addr as u64; }
                }
            },
            2 => { // Interrupt Source Override
                let bus = unsafe { *(entry_ptr.add(2)) };
                let source = unsafe { *(entry_ptr.add(3)) };
                let gsi = unsafe { *(entry_ptr.add(4) as *const u32) };
                let flags = unsafe { *(entry_ptr.add(8) as *const u16) };
                log::info!("[ACPI] ISO: Bus {} Source {} -> GSI {} (Flags {:#x})", bus, source, gsi, flags);
            },
            5 => { // LAPIC Address Override
                let lapic_addr = unsafe { *(entry_ptr.add(4) as *const u64) };
                log::info!("[ACPI] LAPIC Address Override: {:#x}", lapic_addr);
                unsafe { LAPIC_ADDR = lapic_addr; }
            }
            _ => {}
        }
        
        offset += entry_len as u64;
    }
}
