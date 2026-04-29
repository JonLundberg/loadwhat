use std::fs;
use std::path::Path;

const PE_OFFSET: usize = 0x80;
const OPTIONAL_HEADER_SIZE: u16 = 0xF0;
const OPTIONAL_HEADER_OFFSET: usize = PE_OFFSET + 24;
const DATA_DIR_START: usize = OPTIONAL_HEADER_OFFSET + 112;
const SECTION_TABLE_OFFSET: usize = PE_OFFSET + 24 + OPTIONAL_HEADER_SIZE as usize;
const NUMBER_OF_SECTIONS_OFFSET: usize = PE_OFFSET + 6;
const SIZE_OF_OPTIONAL_HEADER_OFFSET: usize = PE_OFFSET + 20;
const SECTION_VIRTUAL_ADDRESS: u32 = 0x1000;
const SECTION_RAW_DATA_PTR: u32 = 0x200;

pub fn write_import_test_pe(path: &Path, imports: &[&str]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create {}: {e}", parent.display()))?;
    }
    fs::write(path, build_import_test_pe(imports))
        .map_err(|e| format!("failed to write {}: {e}", path.display()))
}

pub fn build_import_test_pe(imports: &[&str]) -> Vec<u8> {
    build_import_test_pe_with_headers(imports, 0x8664, 0x020B)
}

pub fn write_import_test_pe_x86(path: &Path, imports: &[&str]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create {}: {e}", parent.display()))?;
    }
    fs::write(path, build_import_test_pe_x86(imports))
        .map_err(|e| format!("failed to write {}: {e}", path.display()))
}

pub fn build_import_test_pe_x86(imports: &[&str]) -> Vec<u8> {
    build_import_test_pe_with_headers(imports, 0x014C, 0x010B)
}

fn build_import_test_pe_with_headers(imports: &[&str], machine: u16, magic: u16) -> Vec<u8> {
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

    let mut string_cursor = SECTION_RAW_DATA_PTR as usize + descriptor_bytes;
    for (idx, import) in imports.iter().enumerate() {
        let descriptor_offset = SECTION_RAW_DATA_PTR as usize + idx * 20;
        let name_rva =
            SECTION_VIRTUAL_ADDRESS + (string_cursor - SECTION_RAW_DATA_PTR as usize) as u32;
        write_u32(&mut bytes, descriptor_offset + 12, name_rva);
        bytes[string_cursor..string_cursor + import.len()].copy_from_slice(import.as_bytes());
        string_cursor += import.len();
        bytes[string_cursor] = 0;
        string_cursor += 1;
    }

    bytes
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
