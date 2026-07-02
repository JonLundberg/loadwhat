// Parses PE import tables and performs lightweight PE validity checks for search results.

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

/// Machine type of a PE image, derived from the COFF header `Machine` field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MachineType {
    X64,
    X86,
    Unknown,
}

impl MachineType {
    pub fn as_token(&self) -> &'static str {
        match self {
            MachineType::X64 => "x64",
            MachineType::X86 => "x86",
            MachineType::Unknown => "unknown",
        }
    }
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

pub fn machine_type_from_bytes(data: &[u8]) -> Result<MachineType, String> {
    let pe = parse_pe_layout(data)?;
    Ok(match pe.machine {
        0x8664 => MachineType::X64,
        0x014C => MachineType::X86,
        _ => MachineType::Unknown,
    })
}

pub(crate) fn direct_imports_from_bytes(data: &[u8]) -> Result<Vec<String>, String> {
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
    resource_rva: u32,
    machine: u16,
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
    // Resource table is data directory index 2; only present when the
    // optional header carries at least three directory entries.
    let resource_rva = if data_dir_start + 24 <= optional_header_off + size_of_optional_header {
        read_u32(data, data_dir_start + 16)?
    } else {
        0
    };
    let machine = read_u16(data, pe_offset + 4)?;
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
        resource_rva,
        machine,
        sections,
    })
}

const RT_MANIFEST: u32 = 24;

/// Extracts the embedded RT_MANIFEST resource from a PE image, if any.
/// Best-effort: returns None for missing/unparseable resources rather than
/// failing, because a broken resource tree should not abort COM diagnosis.
pub fn extract_embedded_manifest(module_path: &Path) -> Option<String> {
    let data = fs::read(module_path).ok()?;
    extract_embedded_manifest_from_bytes(&data)
}

pub(crate) fn extract_embedded_manifest_from_bytes(data: &[u8]) -> Option<String> {
    let pe = parse_pe_layout(data).ok()?;
    if pe.resource_rva == 0 {
        return None;
    }
    let rsrc_off = rva_to_offset(pe.resource_rva, &pe.sections)?;

    // Level 1: resource type directory; find the RT_MANIFEST ID entry.
    let manifest_dir = find_resource_entry(data, rsrc_off, rsrc_off, Some(RT_MANIFEST))?;
    // Level 2: resource name/ID; take the first entry.
    let lang_dir = find_resource_entry(data, rsrc_off, manifest_dir, None)?;
    // Level 3: language; take the first entry, which must be a data entry.
    let data_entry_off = find_resource_entry(data, rsrc_off, lang_dir, None)?;

    let data_rva = read_u32(data, data_entry_off).ok()?;
    let size = read_u32(data, data_entry_off + 4).ok()? as usize;
    let payload_off = rva_to_offset(data_rva, &pe.sections)?;
    let payload = data.get(payload_off..payload_off.checked_add(size)?)?;
    Some(decode_manifest_text(payload))
}

/// Walks one level of the resource directory at `dir_off`. With `Some(id)`,
/// returns the target offset of the entry whose ID matches; with `None`,
/// returns the target offset of the first entry. Subdirectory targets are
/// resolved relative to `rsrc_off`; data-entry targets likewise.
fn find_resource_entry(
    data: &[u8],
    rsrc_off: usize,
    dir_off: usize,
    id: Option<u32>,
) -> Option<usize> {
    let named = read_u16(data, dir_off + 12).ok()? as usize;
    let id_count = read_u16(data, dir_off + 14).ok()? as usize;
    let entries_off = dir_off + 16;
    let total = named.checked_add(id_count)?;

    for i in 0..total {
        let entry_off = entries_off.checked_add(i.checked_mul(8)?)?;
        let name_or_id = read_u32(data, entry_off).ok()?;
        let offset_to_data = read_u32(data, entry_off + 4).ok()?;

        let matches = match id {
            // ID entries follow named entries; high bit of name field clear.
            Some(want) => name_or_id & 0x8000_0000 == 0 && name_or_id == want,
            None => true,
        };
        if !matches {
            continue;
        }

        let target = rsrc_off.checked_add((offset_to_data & 0x7FFF_FFFF) as usize)?;
        return Some(target);
    }
    None
}

fn decode_manifest_text(payload: &[u8]) -> String {
    if payload.len() >= 2 && payload[0] == 0xFF && payload[1] == 0xFE {
        let units: Vec<u16> = payload[2..]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        return String::from_utf16_lossy(&units);
    }
    let body = if payload.len() >= 3 && payload[0..3] == [0xEF, 0xBB, 0xBF] {
        &payload[3..]
    } else {
        payload
    };
    String::from_utf8_lossy(body).into_owned()
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

/// Shared synthetic-PE builder for unit tests (used by pe tests and the COM
/// mock file system).
#[cfg(test)]
pub(crate) mod testpe {
    pub(crate) const PE_OFFSET: usize = 0x80;
    pub(crate) const OPTIONAL_HEADER_SIZE: u16 = 0xF0;
    pub(crate) const SECTION_TABLE_OFFSET: usize = PE_OFFSET + 24 + OPTIONAL_HEADER_SIZE as usize;
    pub(crate) const OPTIONAL_HEADER_OFFSET: usize = PE_OFFSET + 24;
    pub(crate) const DATA_DIR_START: usize = OPTIONAL_HEADER_OFFSET + 112;
    pub(crate) const IMPORT_DIRECTORY_RVA_OFFSET: usize = DATA_DIR_START + 8;
    pub(crate) const NUMBER_OF_SECTIONS_OFFSET: usize = PE_OFFSET + 6;
    pub(crate) const SIZE_OF_OPTIONAL_HEADER_OFFSET: usize = PE_OFFSET + 20;
    pub(crate) const SECTION_VIRTUAL_ADDRESS: u32 = 0x1000;
    pub(crate) const SECTION_RAW_DATA_PTR: u32 = 0x200;

    pub(crate) struct BuiltPe {
        pub(crate) bytes: Vec<u8>,
        pub(crate) descriptor_offsets: Vec<usize>,
        pub(crate) name_offsets: Vec<usize>,
    }

    pub(crate) fn build_test_pe(imports: &[&str]) -> BuiltPe {
        let descriptor_bytes = (imports.len() + 1) * 20;
        let strings_bytes: usize = imports.iter().map(|name| name.len() + 1).sum();
        let section_size = descriptor_bytes.max(1) + strings_bytes.max(1);
        let total_len = SECTION_RAW_DATA_PTR as usize + section_size;
        let mut bytes = vec![0u8; total_len];

        bytes[0..2].copy_from_slice(b"MZ");
        write_u32(&mut bytes, 0x3C, PE_OFFSET as u32);

        bytes[PE_OFFSET..PE_OFFSET + 4].copy_from_slice(b"PE\0\0");
        write_u16(&mut bytes, PE_OFFSET + 4, 0x8664);
        write_u16(&mut bytes, NUMBER_OF_SECTIONS_OFFSET, 1);
        write_u16(
            &mut bytes,
            SIZE_OF_OPTIONAL_HEADER_OFFSET,
            OPTIONAL_HEADER_SIZE,
        );

        write_u16(&mut bytes, OPTIONAL_HEADER_OFFSET, 0x020B);
        write_u32(
            &mut bytes,
            IMPORT_DIRECTORY_RVA_OFFSET,
            if imports.is_empty() {
                0
            } else {
                SECTION_VIRTUAL_ADDRESS
            },
        );
        write_u32(&mut bytes, DATA_DIR_START + 12, descriptor_bytes as u32);

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

    pub(crate) fn write_u16(bytes: &mut [u8], offset: usize, value: u16) {
        bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }

    pub(crate) fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    pub(crate) fn build_test_pe_with_manifest(xml_payload: &[u8]) -> Vec<u8> {
        let base = build_test_pe(&[]);
        let mut bytes = base.bytes;
        let rsrc_raw = bytes.len();
        let rsrc_va = 0x2000u32;

        // Resource section layout (offsets relative to section start):
        //   0x00 root dir -> RT_MANIFEST subdir at 0x18
        //   0x18 name dir -> language subdir at 0x30
        //   0x30 lang dir -> data entry at 0x48
        //   0x48 data entry -> payload at 0x58
        let mut rsrc = vec![0u8; 0x58];
        write_u16(&mut rsrc, 14, 1);
        write_u32(&mut rsrc, 16, 24);
        write_u32(&mut rsrc, 20, 0x8000_0000 | 0x18);
        write_u16(&mut rsrc, 0x18 + 14, 1);
        write_u32(&mut rsrc, 0x18 + 16, 1);
        write_u32(&mut rsrc, 0x18 + 20, 0x8000_0000 | 0x30);
        write_u16(&mut rsrc, 0x30 + 14, 1);
        write_u32(&mut rsrc, 0x30 + 16, 0x409);
        write_u32(&mut rsrc, 0x30 + 20, 0x48);
        write_u32(&mut rsrc, 0x48, rsrc_va + 0x58);
        write_u32(&mut rsrc, 0x48 + 4, xml_payload.len() as u32);
        rsrc.extend_from_slice(xml_payload);

        let rsrc_len = rsrc.len();
        bytes.extend_from_slice(&rsrc);

        write_u16(&mut bytes, NUMBER_OF_SECTIONS_OFFSET, 2);
        let s2 = SECTION_TABLE_OFFSET + 40;
        bytes[s2..s2 + 5].copy_from_slice(b".rsrc");
        write_u32(&mut bytes, s2 + 8, rsrc_len as u32);
        write_u32(&mut bytes, s2 + 12, rsrc_va);
        write_u32(&mut bytes, s2 + 16, rsrc_len as u32);
        write_u32(&mut bytes, s2 + 20, rsrc_raw as u32);

        write_u32(&mut bytes, DATA_DIR_START + 16, rsrc_va);
        write_u32(&mut bytes, DATA_DIR_START + 20, rsrc_len as u32);
        bytes
    }
}

#[cfg(test)]
mod tests {
    use super::testpe::*;
    use super::*;

    #[test]
    fn machine_type_reports_x64() {
        let pe = build_test_pe(&[]);
        assert_eq!(
            machine_type_from_bytes(&pe.bytes).unwrap(),
            MachineType::X64
        );
    }

    #[test]
    fn machine_type_reports_x86() {
        let mut pe = build_test_pe(&[]);
        write_u16(&mut pe.bytes, PE_OFFSET + 4, 0x014C);
        assert_eq!(
            machine_type_from_bytes(&pe.bytes).unwrap(),
            MachineType::X86
        );
    }

    #[test]
    fn machine_type_reports_unknown_for_other_values() {
        let mut pe = build_test_pe(&[]);
        write_u16(&mut pe.bytes, PE_OFFSET + 4, 0x01C4);
        assert_eq!(
            machine_type_from_bytes(&pe.bytes).unwrap(),
            MachineType::Unknown
        );
    }

    #[test]
    fn machine_type_rejects_non_pe_bytes() {
        assert!(machine_type_from_bytes(&[0u8; 16]).is_err());
    }

    #[test]
    fn manifest_extraction_returns_none_without_resource_section() {
        let pe = build_test_pe(&[]);
        assert_eq!(extract_embedded_manifest_from_bytes(&pe.bytes), None);
    }

    #[test]
    fn manifest_extraction_reads_utf8_payload() {
        let xml = r#"<assembly><comClass clsid="{X}"/></assembly>"#;
        let bytes = build_test_pe_with_manifest(xml.as_bytes());
        assert_eq!(
            extract_embedded_manifest_from_bytes(&bytes).as_deref(),
            Some(xml)
        );
    }

    #[test]
    fn manifest_extraction_strips_utf8_bom() {
        let xml = "<assembly/>";
        let mut payload = vec![0xEF, 0xBB, 0xBF];
        payload.extend_from_slice(xml.as_bytes());
        let bytes = build_test_pe_with_manifest(&payload);
        assert_eq!(
            extract_embedded_manifest_from_bytes(&bytes).as_deref(),
            Some(xml)
        );
    }

    #[test]
    fn manifest_extraction_decodes_utf16_payload() {
        let xml = "<assembly/>";
        let mut payload = vec![0xFF, 0xFE];
        for unit in xml.encode_utf16() {
            payload.extend_from_slice(&unit.to_le_bytes());
        }
        let bytes = build_test_pe_with_manifest(&payload);
        assert_eq!(
            extract_embedded_manifest_from_bytes(&bytes).as_deref(),
            Some(xml)
        );
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
}
