#![allow(dead_code)]

use core::arch::asm;

const PCI_CONFIG_ADDRESS: u16 = 0xCF8;
const PCI_CONFIG_DATA: u16 = 0xCFC;

pub unsafe fn pci_config_read_dword(bus: u8, slot: u8, func: u8, offset: u8) -> u32 {
    let address: u32 = 0x80000000 | ((bus as u32) << 16) | ((slot as u32) << 11) | ((func as u32) << 8) | (offset as u32 & 0xFC);
    asm!("out dx, eax", in("dx") PCI_CONFIG_ADDRESS, in("eax") address, options(nomem, nostack, preserves_flags));
    let mut tmp: u32;
    asm!("in eax, dx", out("eax") tmp, in("dx") PCI_CONFIG_DATA, options(nomem, nostack, preserves_flags));
    tmp
}

pub unsafe fn pci_config_write_dword(bus: u8, slot: u8, func: u8, offset: u8, value: u32) {
    let address: u32 = 0x80000000 | ((bus as u32) << 16) | ((slot as u32) << 11) | ((func as u32) << 8) | (offset as u32 & 0xFC);
    asm!("out dx, eax", in("dx") PCI_CONFIG_ADDRESS, in("eax") address, options(nomem, nostack, preserves_flags));
    asm!("out dx, eax", in("dx") PCI_CONFIG_DATA, in("eax") value, options(nomem, nostack, preserves_flags));
}

#[derive(Debug)]
pub struct PciDevice {
    pub bus: u8,
    pub slot: u8,
    pub func: u8,
    pub vendor_id: u16,
    pub device_id: u16,
    pub bar0_base: u32,
    pub bar0_size: u32,
    pub bar0_is_io: bool,
    pub irq_line: u8,
}

pub fn scan_bus_zero() -> Option<PciDevice> {
    for slot in 0..32 {
        let vendor_id = unsafe { pci_config_read_dword(0, slot, 0, 0) } & 0xFFFF;
        if vendor_id == 0xFFFF { continue; }
        let device_id = (unsafe { pci_config_read_dword(0, slot, 0, 0) } >> 16) & 0xFFFF;
        
        if vendor_id == 0x10EC && device_id == 0x8139 {
            log::info!("[PCI] Found RTL8139 at 0:{}:0", slot);
            
            // Read and size BAR0
            let bar0 = unsafe { pci_config_read_dword(0, slot, 0, 0x10) };
            unsafe { pci_config_write_dword(0, slot, 0, 0x10, 0xFFFFFFFF) };
            let bar0_size_raw = unsafe { pci_config_read_dword(0, slot, 0, 0x10) };
            unsafe { pci_config_write_dword(0, slot, 0, 0x10, bar0) };
            
            let (bar0_base, bar0_size, is_io) = if bar0 & 1 == 1 {
                let base = bar0 & !0x3;
                let size = !(bar0_size_raw & !0x3) + 1;
                (base, size, true)
            } else {
                let base = bar0 & !0xF;
                let size = !(bar0_size_raw & !0xF) + 1;
                (base, size, false)
            };
            
            log::info!("[PCI] RTL8139 BAR0 Base: {:#010x}, Size: {:#x}, I/O: {}", bar0_base, bar0_size, is_io);
            
            let intr = unsafe { pci_config_read_dword(0, slot, 0, 0x3C) };
            let irq_line = (intr & 0xFF) as u8;
            log::info!("[PCI] RTL8139 IRQ Line: {}", irq_line);
            
            // Enable Bus Mastering and I/O Space (and Memory Space just in case)
            let mut command_reg = unsafe { pci_config_read_dword(0, slot, 0, 0x04) };
            command_reg |= 0x0007; // Bus Master | Mem Space | I/O Space
            unsafe { pci_config_write_dword(0, slot, 0, 0x04, command_reg) };
            log::info!("[PCI] RTL8139 Bus Mastering enabled.");
            
            return Some(PciDevice {
                bus: 0, slot, func: 0,
                vendor_id: vendor_id as u16,
                device_id: device_id as u16,
                bar0_base,
                bar0_size,
                bar0_is_io: is_io,
                irq_line,
            });
        }
    }
    None
}
