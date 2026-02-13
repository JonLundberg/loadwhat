use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

#[derive(Clone, Copy)]
struct Section {
    virtual_address: u32,
    virtual_size: u32,
    raw_data_ptr: u32,
    raw_data_size: u32,
}

pub fn direct_imports(module_path: &Path) -> Result<Vec<String>, String> {
    let data = fs::read(module_path)
        .map_err(|e| format!("failed to read {}: {e}", module_path.display()))?;
    direct_imports_from_bytes(&data)
}

pub fn is_probably_pe_file(module_path: &Path) -> bool {
    let Ok(data) = fs::read(module_path) else {
        return false;
    };
    parse_pe_layout(&data).is_ok()
}

fn direct_imports_from_bytes(data: &[u8]) -> Result<Vec<String>, String> {
    let pe = parse_pe_layout(data)?;
    if pe.import_rva == 0 {
        return Ok(Vec::new());
    }

    let mut imports = BTreeSet::new();
    let mut off = rva_to_offset(pe.import_rva, &pe.sections)
        .ok_or_else(|| "invalid import table RVA".to_string())?;

    loop {
        if off + 20 > data.len() {
            return Err("truncated import descriptor table".to_string());
        }

        let original_first_thunk = read_u32(data, off)?;
        let time_date_stamp = read_u32(data, off + 4)?;
        let forwarder_chain = read_u32(data, off + 8)?;
        let name_rva = read_u32(data, off + 12)?;
        let first_thunk = read_u32(data, off + 16)?;

        if original_first_thunk == 0
            && time_date_stamp == 0
            && forwarder_chain == 0
            && name_rva == 0
            && first_thunk == 0
        {
            break;
        }

        let name_off = rva_to_offset(name_rva, &pe.sections)
            .ok_or_else(|| "invalid import name RVA".to_string())?;
        let name = read_c_string(data, name_off)?;
        imports.insert(name.to_ascii_lowercase());

        off += 20;
    }

    Ok(imports.into_iter().collect())
}

struct PeLayout {
    import_rva: u32,
    sections: Vec<Section>,
}

fn parse_pe_layout(data: &[u8]) -> Result<PeLayout, String> {
    if data.len() < 0x40 {
        return Err("file too small for DOS header".to_string());
    }

    if &data[0..2] != b"MZ" {
        return Err("missing MZ header".to_string());
    }

    let pe_offset = read_u32(data, 0x3C)? as usize;
    if pe_offset + 24 > data.len() {
        return Err("invalid PE header offset".to_string());
    }

    if &data[pe_offset..pe_offset + 4] != b"PE\0\0" {
        return Err("missing PE signature".to_string());
    }

    let number_of_sections = read_u16(data, pe_offset + 6)? as usize;
    let size_of_optional_header = read_u16(data, pe_offset + 20)? as usize;
    let optional_header_off = pe_offset + 24;

    if optional_header_off + size_of_optional_header > data.len() {
        return Err("truncated optional header".to_string());
    }
    if size_of_optional_header < 2 {
        return Err("optional header too small".to_string());
    }

    let magic = read_u16(data, optional_header_off)?;
    let data_dir_start = match magic {
        0x010B => optional_header_off + 96,
        0x020B => optional_header_off + 112,
        _ => return Err("unsupported optional header format".to_string()),
    };

    if data_dir_start + 16 > optional_header_off + size_of_optional_header {
        return Err("optional header missing data directories".to_string());
    }

    let import_rva = read_u32(data, data_dir_start + 8)?;
    let section_table_off = optional_header_off + size_of_optional_header;
    let section_table_len = number_of_sections
        .checked_mul(40)
        .ok_or_else(|| "section table overflow".to_string())?;

    if section_table_off + section_table_len > data.len() {
        return Err("truncated section table".to_string());
    }

    let mut sections = Vec::with_capacity(number_of_sections);
    for i in 0..number_of_sections {
        let base = section_table_off + i * 40;
        let virtual_size = read_u32(data, base + 8)?;
        let virtual_address = read_u32(data, base + 12)?;
        let raw_data_size = read_u32(data, base + 16)?;
        let raw_data_ptr = read_u32(data, base + 20)?;
        sections.push(Section {
            virtual_address,
            virtual_size,
            raw_data_ptr,
            raw_data_size,
        });
    }

    Ok(PeLayout {
        import_rva,
        sections,
    })
}

fn rva_to_offset(rva: u32, sections: &[Section]) -> Option<usize> {
    for section in sections {
        let start = section.virtual_address;
        let size = section.virtual_size.max(section.raw_data_size);
        let end = start.saturating_add(size);
        if rva >= start && rva < end {
            let delta = rva - start;
            return section.raw_data_ptr.checked_add(delta).map(|v| v as usize);
        }
    }
    None
}

fn read_c_string(data: &[u8], offset: usize) -> Result<String, String> {
    if offset >= data.len() {
        return Err("string offset out of bounds".to_string());
    }
    let mut end = offset;
    while end < data.len() && data[end] != 0 {
        end += 1;
    }
    if end == data.len() {
        return Err("unterminated import string".to_string());
    }
    String::from_utf8(data[offset..end].to_vec())
        .map_err(|_| "import name is not valid UTF-8".to_string())
}

fn read_u16(data: &[u8], offset: usize) -> Result<u16, String> {
    let bytes = data
        .get(offset..offset + 2)
        .ok_or_else(|| "unexpected EOF".to_string())?;
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn read_u32(data: &[u8], offset: usize) -> Result<u32, String> {
    let bytes = data
        .get(offset..offset + 4)
        .ok_or_else(|| "unexpected EOF".to_string())?;
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}
