// Parses PE import tables and performs lightweight PE validity checks for search results.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

const IMAGE_FILE_MACHINE_I386: u16 = 0x014C;
const IMAGE_FILE_MACHINE_AMD64: u16 = 0x8664;
const OPTIONAL_HEADER_MAGIC_PE32: u16 = 0x010B;
const OPTIONAL_HEADER_MAGIC_PE32_PLUS: u16 = 0x020B;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImageArchitecture {
    X64,
    X86,
    Other { machine: u16, magic: u16 },
}

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

pub fn image_architecture(module_path: &Path) -> Result<ImageArchitecture, String> {
    let data = fs::read(module_path)
        .map_err(|e| format!("failed to read {}: {e}", module_path.display()))?;
    Ok(parse_pe_layout(&data)?.architecture)
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
    architecture: ImageArchitecture,
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

    let machine = read_u16(data, pe_offset + 4)?;
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
        OPTIONAL_HEADER_MAGIC_PE32 => optional_header_off + 96,
        OPTIONAL_HEADER_MAGIC_PE32_PLUS => optional_header_off + 112,
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
        architecture: image_architecture_from_headers(machine, magic),
        import_rva,
        sections,
    })
}

fn image_architecture_from_headers(machine: u16, magic: u16) -> ImageArchitecture {
    match (machine, magic) {
        (IMAGE_FILE_MACHINE_AMD64, OPTIONAL_HEADER_MAGIC_PE32_PLUS) => ImageArchitecture::X64,
        (IMAGE_FILE_MACHINE_I386, OPTIONAL_HEADER_MAGIC_PE32) => ImageArchitecture::X86,
        _ => ImageArchitecture::Other { machine, magic },
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    const PE_OFFSET: usize = 0x80;
    const OPTIONAL_HEADER_SIZE: u16 = 0xF0;
    const SECTION_TABLE_OFFSET: usize = PE_OFFSET + 24 + OPTIONAL_HEADER_SIZE as usize;
    const OPTIONAL_HEADER_OFFSET: usize = PE_OFFSET + 24;
    const DATA_DIR_START: usize = OPTIONAL_HEADER_OFFSET + 112;
    const IMPORT_DIRECTORY_RVA_OFFSET: usize = DATA_DIR_START + 8;
    const NUMBER_OF_SECTIONS_OFFSET: usize = PE_OFFSET + 6;
    const SIZE_OF_OPTIONAL_HEADER_OFFSET: usize = PE_OFFSET + 20;
    const SECTION_VIRTUAL_ADDRESS: u32 = 0x1000;
    const SECTION_RAW_DATA_PTR: u32 = 0x200;

    struct BuiltPe {
        bytes: Vec<u8>,
        descriptor_offsets: Vec<usize>,
        name_offsets: Vec<usize>,
    }

    fn build_test_pe(imports: &[&str]) -> BuiltPe {
        build_test_pe_with_headers(imports, 0x8664, 0x020B)
    }

    fn build_test_pe_with_headers(imports: &[&str], machine: u16, magic: u16) -> BuiltPe {
        let descriptor_bytes = (imports.len() + 1) * 20;
        let strings_bytes: usize = imports.iter().map(|name| name.len() + 1).sum();
        let section_size = descriptor_bytes.max(1) + strings_bytes.max(1);
        let total_len = SECTION_RAW_DATA_PTR as usize + section_size;
        let mut bytes = vec![0u8; total_len];

        bytes[0..2].copy_from_slice(b"MZ");
        write_u32(&mut bytes, 0x3C, PE_OFFSET as u32);

        bytes[PE_OFFSET..PE_OFFSET + 4].copy_from_slice(b"PE\0\0");
        write_u16(&mut bytes, PE_OFFSET + 4, machine);
        write_u16(&mut bytes, NUMBER_OF_SECTIONS_OFFSET, 1);
        write_u16(
            &mut bytes,
            SIZE_OF_OPTIONAL_HEADER_OFFSET,
            OPTIONAL_HEADER_SIZE,
        );

        write_u16(&mut bytes, OPTIONAL_HEADER_OFFSET, magic);
        let data_dir_start = data_dir_start_for_magic(magic);
        write_u32(
            &mut bytes,
            data_dir_start + 8,
            if imports.is_empty() {
                0
            } else {
                SECTION_VIRTUAL_ADDRESS
            },
        );
        write_u32(&mut bytes, data_dir_start + 12, descriptor_bytes as u32);

        bytes[SECTION_TABLE_OFFSET..SECTION_TABLE_OFFSET + 6].copy_from_slice(b".rdata");
        write_u32(&mut bytes, SECTION_TABLE_OFFSET + 8, section_size as u32);
        write_u32(
            &mut bytes,
            SECTION_TABLE_OFFSET + 12,
            SECTION_VIRTUAL_ADDRESS,
        );
        write_u32(&mut bytes, SECTION_TABLE_OFFSET + 16, section_size as u32);
        write_u32(&mut bytes, SECTION_TABLE_OFFSET + 20, SECTION_RAW_DATA_PTR);

        let mut descriptor_offsets = Vec::new();
        let mut name_offsets = Vec::new();
        let mut string_cursor = SECTION_RAW_DATA_PTR as usize + descriptor_bytes;
        for (idx, import) in imports.iter().enumerate() {
            let descriptor_offset = SECTION_RAW_DATA_PTR as usize + idx * 20;
            let name_rva =
                SECTION_VIRTUAL_ADDRESS + (string_cursor - SECTION_RAW_DATA_PTR as usize) as u32;
            write_u32(&mut bytes, descriptor_offset + 12, name_rva);
            descriptor_offsets.push(descriptor_offset);
            name_offsets.push(string_cursor);

            bytes[string_cursor..string_cursor + import.len()].copy_from_slice(import.as_bytes());
            string_cursor += import.len();
            bytes[string_cursor] = 0;
            string_cursor += 1;
        }

        BuiltPe {
            bytes,
            descriptor_offsets,
            name_offsets,
        }
    }

    fn data_dir_start_for_magic(magic: u16) -> usize {
        match magic {
            0x010B => OPTIONAL_HEADER_OFFSET + 96,
            _ => DATA_DIR_START,
        }
    }

    fn write_u16(bytes: &mut [u8], offset: usize, value: u16) {
        bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }

    fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    #[test]
    fn rejects_file_too_small_for_dos_header() {
        assert_eq!(
            direct_imports_from_bytes(&[0u8; 16]).unwrap_err(),
            "file too small for DOS header"
        );
    }

    #[test]
    fn rejects_missing_mz_header() {
        let mut pe = build_test_pe(&[]);
        pe.bytes[0..2].copy_from_slice(b"NZ");
        assert_eq!(
            direct_imports_from_bytes(&pe.bytes).unwrap_err(),
            "missing MZ header"
        );
    }

    #[test]
    fn rejects_invalid_pe_header_offset() {
        let mut pe = build_test_pe(&[]);
        let len = pe.bytes.len() as u32;
        write_u32(&mut pe.bytes, 0x3C, len);
        assert_eq!(
            direct_imports_from_bytes(&pe.bytes).unwrap_err(),
            "invalid PE header offset"
        );
    }

    #[test]
    fn rejects_missing_pe_signature() {
        let mut pe = build_test_pe(&[]);
        pe.bytes[PE_OFFSET..PE_OFFSET + 4].copy_from_slice(b"PX\0\0");
        assert_eq!(
            direct_imports_from_bytes(&pe.bytes).unwrap_err(),
            "missing PE signature"
        );
    }

    #[test]
    fn rejects_truncated_optional_header() {
        let pe = build_test_pe(&[]);
        let truncated = &pe.bytes[..OPTIONAL_HEADER_OFFSET + OPTIONAL_HEADER_SIZE as usize - 1];
        assert_eq!(
            direct_imports_from_bytes(truncated).unwrap_err(),
            "truncated optional header"
        );
    }

    #[test]
    fn rejects_unsupported_optional_header_magic() {
        let mut pe = build_test_pe(&[]);
        write_u16(&mut pe.bytes, OPTIONAL_HEADER_OFFSET, 0x1234);
        assert_eq!(
            direct_imports_from_bytes(&pe.bytes).unwrap_err(),
            "unsupported optional header format"
        );
    }

    #[test]
    fn rejects_missing_data_directories() {
        let mut pe = build_test_pe(&[]);
        write_u16(&mut pe.bytes, SIZE_OF_OPTIONAL_HEADER_OFFSET, 120);
        assert_eq!(
            direct_imports_from_bytes(&pe.bytes).unwrap_err(),
            "optional header missing data directories"
        );
    }

    #[test]
    fn rejects_truncated_section_table() {
        let mut pe = build_test_pe(&[]);
        write_u16(&mut pe.bytes, NUMBER_OF_SECTIONS_OFFSET, 2);
        pe.bytes.truncate(SECTION_TABLE_OFFSET + 79);
        assert_eq!(
            direct_imports_from_bytes(&pe.bytes).unwrap_err(),
            "truncated section table"
        );
    }

    #[test]
    fn rva_to_offset_returns_none_outside_sections() {
        let sections = vec![Section {
            virtual_address: 0x1000,
            virtual_size: 0x200,
            raw_data_ptr: 0x400,
            raw_data_size: 0x100,
        }];
        assert_eq!(rva_to_offset(0x2000, &sections), None);
    }

    #[test]
    fn rva_to_offset_uses_max_of_virtual_and_raw_size() {
        let sections = vec![Section {
            virtual_address: 0x1000,
            virtual_size: 0x20,
            raw_data_ptr: 0x400,
            raw_data_size: 0x200,
        }];
        assert_eq!(rva_to_offset(0x1100, &sections), Some(0x500));
    }

    #[test]
    fn rejects_import_table_rva_that_cannot_be_mapped() {
        let mut pe = build_test_pe(&["a.dll"]);
        write_u32(&mut pe.bytes, IMPORT_DIRECTORY_RVA_OFFSET, 0x5000);
        assert_eq!(
            direct_imports_from_bytes(&pe.bytes).unwrap_err(),
            "invalid import table RVA"
        );
    }

    #[test]
    fn rejects_truncated_import_descriptor_table() {
        let pe = build_test_pe(&["a.dll"]);
        let truncated = &pe.bytes[..pe.descriptor_offsets[0] + 19];
        assert_eq!(
            direct_imports_from_bytes(truncated).unwrap_err(),
            "truncated import descriptor table"
        );
    }

    #[test]
    fn rejects_invalid_import_name_rva() {
        let mut pe = build_test_pe(&["a.dll"]);
        write_u32(&mut pe.bytes, pe.descriptor_offsets[0] + 12, 0x5000);
        assert_eq!(
            direct_imports_from_bytes(&pe.bytes).unwrap_err(),
            "invalid import name RVA"
        );
    }

    #[test]
    fn rejects_unterminated_import_string() {
        let pe = build_test_pe(&["a.dll"]);
        let truncated = &pe.bytes[..pe.name_offsets[0] + "a.dll".len()];
        assert_eq!(
            direct_imports_from_bytes(truncated).unwrap_err(),
            "unterminated import string"
        );
    }

    #[test]
    fn empty_import_table_returns_empty_list() {
        let pe = build_test_pe(&[]);
        assert_eq!(
            direct_imports_from_bytes(&pe.bytes).unwrap(),
            Vec::<String>::new()
        );
    }

    #[test]
    fn duplicate_import_names_collapse_deterministically() {
        let pe = build_test_pe(&["z.dll", "A.dll", "a.dll"]);
        assert_eq!(
            direct_imports_from_bytes(&pe.bytes).unwrap(),
            vec!["a.dll".to_string(), "z.dll".to_string()]
        );
    }

    #[test]
    fn returned_imports_are_lexicographically_ordered() {
        let pe = build_test_pe(&["kernel32.dll", "a.dll", "z.dll"]);
        assert_eq!(
            direct_imports_from_bytes(&pe.bytes).unwrap(),
            vec![
                "a.dll".to_string(),
                "kernel32.dll".to_string(),
                "z.dll".to_string(),
            ]
        );
    }

    #[test]
    fn image_architecture_detects_x64() {
        let pe = build_test_pe(&[]);
        assert_eq!(
            parse_pe_layout(&pe.bytes).unwrap().architecture,
            ImageArchitecture::X64
        );
    }

    #[test]
    fn image_architecture_detects_x86() {
        let pe = build_test_pe_with_headers(&[], 0x014C, 0x010B);
        assert_eq!(
            parse_pe_layout(&pe.bytes).unwrap().architecture,
            ImageArchitecture::X86
        );
    }

    #[test]
    fn image_architecture_marks_machine_magic_mismatch_as_other() {
        let pe = build_test_pe_with_headers(&[], 0x014C, 0x020B);
        assert_eq!(
            parse_pe_layout(&pe.bytes).unwrap().architecture,
            ImageArchitecture::Other {
                machine: 0x014C,
                magic: 0x020B
            }
        );
    }

    #[test]
    fn direct_imports_supports_pe32_layout_for_detection_plumbing() {
        let pe = build_test_pe_with_headers(&["Kernel32.dll"], 0x014C, 0x010B);
        assert_eq!(
            direct_imports_from_bytes(&pe.bytes).unwrap(),
            vec!["kernel32.dll".to_string()]
        );
    }
}
