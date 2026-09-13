pub fn dummy(gdt: &mut x86_64::structures::gdt::GlobalDescriptorTable) { gdt.add_entry(); }
