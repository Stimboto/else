pub struct TarArchive<'a> {
    data: &'a [u8],
}

impl<'a> TarArchive<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data }
    }

    pub fn find_file(&self, name: &str) -> Option<&'a [u8]> {
        let mut offset = 0;
        
        while offset + 512 <= self.data.len() {
            let header = &self.data[offset..offset + 512];
            
            // Check for empty block (end of archive)
            if header[0] == 0 {
                break;
            }
            
            // File name is first 100 bytes
            let mut name_len = 0;
            while name_len < 100 && header[name_len] != 0 {
                name_len += 1;
            }
            let file_name = core::str::from_utf8(&header[0..name_len]).unwrap_or("");
            
            // Parse octal size
            let size_bytes = &header[124..136];
            let mut size = 0;
            for &b in size_bytes {
                if b >= b'0' && b <= b'7' {
                    size = size * 8 + (b - b'0') as usize;
                } else if b == 0 || b == b' ' {
                    break;
                }
            }
            
            let type_flag = header[156];
            let is_file = type_flag == b'0' || type_flag == 0;
            
            if is_file && file_name == name {
                let data_start = offset + 512;
                if data_start + size <= self.data.len() {
                    return Some(&self.data[data_start..data_start + size]);
                } else {
                    return None;
                }
            }
            
            // Advance offset
            let blocks = (size + 511) / 512;
            offset += 512 + blocks * 512;
        }
        
        None
    }
}
