use x86_64::structures::gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector};
use x86_64::structures::tss::TaskStateSegment;
use x86_64::VirtAddr;
use spin::Lazy;

pub const DOUBLE_FAULT_IST_INDEX: u16 = 0;

static mut TSS: TaskStateSegment = TaskStateSegment::new();

pub struct Selectors {
    pub kernel_code_selector: SegmentSelector,
    pub kernel_data_selector: SegmentSelector,
    pub user_code_selector: SegmentSelector,
    pub user_data_selector: SegmentSelector,
    pub tss_selector: SegmentSelector,
}

static GDT: Lazy<(GlobalDescriptorTable, Selectors)> = Lazy::new(|| {
    let mut gdt = GlobalDescriptorTable::new();
    
    let kernel_code_selector = gdt.append(Descriptor::kernel_code_segment());
    let kernel_data_selector = gdt.append(Descriptor::kernel_data_segment());
    let user_data_selector = gdt.append(Descriptor::user_data_segment());
    let user_code_selector = gdt.append(Descriptor::user_code_segment());
    
    // Set up TSS
    unsafe {
        TSS.interrupt_stack_table[DOUBLE_FAULT_IST_INDEX as usize] = {
            const STACK_SIZE: usize = 4096 * 5; // 20 KiB stack for double faults
            static mut STACK: [u8; STACK_SIZE] = [0; STACK_SIZE];
            let stack_start = VirtAddr::from_ptr(&raw const STACK);
            stack_start + STACK_SIZE as u64
        };
    }
    
    let tss_selector = unsafe { gdt.append(Descriptor::tss_segment(&*(&raw const TSS))) };
    
    (
        gdt,
        Selectors {
            kernel_code_selector,
            kernel_data_selector,
            user_code_selector,
            user_data_selector,
            tss_selector,
        },
    )
});

pub fn init() {
    log::info!("[CPU] Loading GDT and TSS...");
    GDT.0.load();
    
    unsafe {
        use x86_64::instructions::segmentation::{CS, DS, ES, FS, GS, SS, Segment};
        use x86_64::instructions::tables::load_tss;
        
        CS::set_reg(GDT.1.kernel_code_selector);
        SS::set_reg(GDT.1.kernel_data_selector);
        
        // Nullify other segment registers
        let null_sel = SegmentSelector::new(0, x86_64::PrivilegeLevel::Ring0);
        DS::set_reg(null_sel);
        ES::set_reg(null_sel);
        FS::set_reg(null_sel);
        GS::set_reg(null_sel);
        
        load_tss(GDT.1.tss_selector);
    }
}

pub fn set_tss_rsp0(rsp0: u64) {
    unsafe {
        TSS.privilege_stack_table[0] = VirtAddr::new(rsp0);
    }
}

pub fn selectors() -> &'static Selectors {
    &GDT.1
}
