// Enables and restores loader-snaps using the PEB-first and IFEO-fallback v1 flow.

use std::ffi::OsStr;
use std::mem;

use crate::win;

const FLG_SHOW_LDR_SNAPS: u32 = 0x0000_0002;
const IFEO_BASE: &str =
    r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options";
const GLOBAL_FLAG_VALUE: &str = "GlobalFlag";
const PEB_NT_GLOBAL_FLAG_OFFSET_X64: usize = 0xBC;
const ERROR_INVALID_PARAMETER: u32 = 87;
const ERROR_PARTIAL_COPY: u32 = 299;

#[derive(Clone, Copy)]
pub struct PebEnableInfo {
    pub os_version: Option<win::OsVersion>,
    pub ntglobalflag_offset: usize,
}

pub enum PebEnableError {
    UnsupportedWow64,
    Win32 { code: u32, info: PebEnableInfo },
}

pub struct LoaderSnapsGuard {
    key_path: String,
    original_value: Option<(u32, Vec<u8>)>,
    restored: bool,
    test_noop: bool,
}

impl LoaderSnapsGuard {
    pub fn enable_for_image(image_name: &str) -> Result<Self, u32> {
        if image_name.is_empty() {
            return Err(87);
        }

        if let Some(result) = test_ifeo_enable_override() {
            return result;
        }

        let key_path = format!(r"{IFEO_BASE}\{image_name}");
        let key = open_or_create_key(&key_path)?;
        let original_value = query_value_raw(key, GLOBAL_FLAG_VALUE)?;

        let current_flags = original_value
            .as_ref()
            .and_then(|(ty, data)| {
                if *ty == win::REG_DWORD && data.len() >= 4 {
                    let mut raw = [0u8; 4];
                    raw.copy_from_slice(&data[..4]);
                    Some(u32::from_le_bytes(raw))
                } else {
                    None
                }
            })
            .unwrap_or(0);
        let new_flags = current_flags | FLG_SHOW_LDR_SNAPS;

        let set_result = set_value_dword(key, GLOBAL_FLAG_VALUE, new_flags);
        close_key(key);
        set_result?;

        Ok(Self {
            key_path,
            original_value,
            restored: false,
            test_noop: false,
        })
    }

    pub fn restore(&mut self) -> Result<(), u32> {
        if self.restored {
            return Ok(());
        }

        if let Some(code) = test_ifeo_restore_override() {
            return Err(code);
        }

        if self.test_noop {
            self.restored = true;
            return Ok(());
        }

        let key = open_or_create_key(&self.key_path)?;
        let result = match &self.original_value {
            Some((data_type, data)) => set_value_raw(key, GLOBAL_FLAG_VALUE, *data_type, data),
            None => delete_value(key, GLOBAL_FLAG_VALUE),
        };
        close_key(key);

        if result.is_ok() {
            self.restored = true;
        }
        result
    }
}

impl Drop for LoaderSnapsGuard {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

pub fn enable_via_peb(process: win::Handle) -> Result<PebEnableInfo, PebEnableError> {
    let os_version = win::rtl_get_version();
    let info = PebEnableInfo {
        os_version,
        ntglobalflag_offset: select_ntglobalflag_offset(os_version),
    };

    if let Some(result) = test_peb_enable_override(info) {
        return result;
    }

    if process.is_null() {
        return Err(PebEnableError::Win32 {
            code: ERROR_INVALID_PARAMETER,
            info,
        });
    }

    match win::is_wow64_process_best_effort(process) {
        Ok(true) => return Err(PebEnableError::UnsupportedWow64),
        Ok(false) => {}
        Err(code) => {
            return Err(PebEnableError::Win32 { code, info });
        }
    }

    let mut pbi: win::ProcessBasicInformation = unsafe { mem::zeroed() };
    let mut return_length: u32 = 0;
    let status = unsafe {
        win::NtQueryInformationProcess(
            process,
            win::PROCESS_BASIC_INFORMATION_CLASS,
            (&mut pbi as *mut win::ProcessBasicInformation).cast::<std::ffi::c_void>(),
            mem::size_of::<win::ProcessBasicInformation>() as u32,
            &mut return_length as *mut u32,
        )
    };
    if status < 0 {
        return Err(PebEnableError::Win32 {
            code: status as u32,
            info,
        });
    }
    if pbi.peb_base_address.is_null() {
        return Err(PebEnableError::Win32 {
            code: ERROR_INVALID_PARAMETER,
            info,
        });
    }

    let nt_global_flag_addr =
        (pbi.peb_base_address as usize + info.ntglobalflag_offset) as win::Lpvoid;
    let current = read_u32(process, nt_global_flag_addr as win::Lpcvoid)
        .map_err(|code| PebEnableError::Win32 { code, info })?;
    let updated = current | FLG_SHOW_LDR_SNAPS;
    if updated != current {
        write_u32(process, nt_global_flag_addr, updated)
            .map_err(|code| PebEnableError::Win32 { code, info })?;
    }

    Ok(info)
}

pub(crate) fn test_peb_enable_override_result() -> Option<Result<PebEnableInfo, PebEnableError>> {
    let os_version = win::rtl_get_version();
    let info = PebEnableInfo {
        os_version,
        ntglobalflag_offset: select_ntglobalflag_offset(os_version),
    };
    test_peb_enable_override(info)
}

fn select_ntglobalflag_offset(os_version: Option<win::OsVersion>) -> usize {
    match os_version {
        Some(v) if v.major >= 10 => PEB_NT_GLOBAL_FLAG_OFFSET_X64,
        Some(_) => PEB_NT_GLOBAL_FLAG_OFFSET_X64,
        None => PEB_NT_GLOBAL_FLAG_OFFSET_X64,
    }
}

fn open_or_create_key(path: &str) -> Result<win::Hkey, u32> {
    let mut key: win::Hkey = 0;
    let path_w = win::to_wide(OsStr::new(path));
    let status = unsafe {
        win::RegCreateKeyExW(
            win::HKEY_LOCAL_MACHINE,
            path_w.as_ptr(),
            0,
            std::ptr::null_mut(),
            win::REG_OPTION_NON_VOLATILE,
            win::KEY_READ | win::KEY_SET_VALUE,
            std::ptr::null_mut(),
            &mut key as *mut win::Hkey,
            std::ptr::null_mut(),
        )
    };
    if status != 0 {
        Err(status as u32)
    } else {
        Ok(key)
    }
}

fn query_value_raw(key: win::Hkey, name: &str) -> Result<Option<(u32, Vec<u8>)>, u32> {
    let name_w = win::to_wide(OsStr::new(name));

    let mut data_type: u32 = 0;
    let mut size: u32 = 0;
    let probe = unsafe {
        win::RegQueryValueExW(
            key,
            name_w.as_ptr(),
            std::ptr::null_mut(),
            &mut data_type as *mut u32,
            std::ptr::null_mut(),
            &mut size as *mut u32,
        )
    };
    if probe as u32 == win::ERROR_FILE_NOT_FOUND {
        return Ok(None);
    }
    if probe != 0 {
        return Err(probe as u32);
    }

    let mut data = vec![0u8; size as usize];
    let mut actual_size = size;
    let query = unsafe {
        win::RegQueryValueExW(
            key,
            name_w.as_ptr(),
            std::ptr::null_mut(),
            &mut data_type as *mut u32,
            if data.is_empty() {
                std::ptr::null_mut()
            } else {
                data.as_mut_ptr()
            },
            &mut actual_size as *mut u32,
        )
    };
    if query != 0 {
        return Err(query as u32);
    }

    data.truncate(actual_size as usize);
    Ok(Some((data_type, data)))
}

fn set_value_dword(key: win::Hkey, name: &str, value: u32) -> Result<(), u32> {
    set_value_raw(key, name, win::REG_DWORD, &value.to_le_bytes())
}

fn set_value_raw(key: win::Hkey, name: &str, value_type: u32, data: &[u8]) -> Result<(), u32> {
    let name_w = win::to_wide(OsStr::new(name));
    let status = unsafe {
        win::RegSetValueExW(
            key,
            name_w.as_ptr(),
            0,
            value_type,
            if data.is_empty() {
                std::ptr::null()
            } else {
                data.as_ptr()
            },
            data.len() as u32,
        )
    };
    if status != 0 {
        Err(status as u32)
    } else {
        Ok(())
    }
}

fn delete_value(key: win::Hkey, name: &str) -> Result<(), u32> {
    let name_w = win::to_wide(OsStr::new(name));
    let status = unsafe { win::RegDeleteValueW(key, name_w.as_ptr()) };
    if status == 0 || status as u32 == win::ERROR_FILE_NOT_FOUND {
        Ok(())
    } else {
        Err(status as u32)
    }
}

fn close_key(key: win::Hkey) {
    if key == 0 {
        return;
    }
    unsafe {
        let _ = win::RegCloseKey(key);
    }
}

fn read_u32(process: win::Handle, address: win::Lpcvoid) -> Result<u32, u32> {
    let mut value = 0u32;
    let mut bytes_read = 0usize;
    let ok = unsafe {
        win::ReadProcessMemory(
            process,
            address,
            (&mut value as *mut u32).cast::<std::ffi::c_void>(),
            mem::size_of::<u32>(),
            &mut bytes_read as *mut usize,
        )
    };
    if ok == 0 {
        return Err(unsafe { win::GetLastError() });
    }
    if bytes_read != mem::size_of::<u32>() {
        return Err(ERROR_PARTIAL_COPY);
    }
    Ok(value)
}

fn write_u32(process: win::Handle, address: win::Lpvoid, value: u32) -> Result<(), u32> {
    let mut bytes_written = 0usize;
    let ok = unsafe {
        win::WriteProcessMemory(
            process,
            address,
            (&value as *const u32).cast::<std::ffi::c_void>(),
            mem::size_of::<u32>(),
            &mut bytes_written as *mut usize,
        )
    };
    if ok == 0 {
        return Err(unsafe { win::GetLastError() });
    }
    if bytes_written != mem::size_of::<u32>() {
        return Err(ERROR_PARTIAL_COPY);
    }
    Ok(())
}

#[cfg(debug_assertions)]
fn test_peb_enable_override(info: PebEnableInfo) -> Option<Result<PebEnableInfo, PebEnableError>> {
    let value = std::env::var("LOADWHAT_TEST_PEB_ENABLE").ok()?;
    let value = value.trim();
    if value.eq_ignore_ascii_case("wow64") {
        return Some(Err(PebEnableError::UnsupportedWow64));
    }

    parse_test_error_code(value)
        .map(|code| Err(PebEnableError::Win32 { code, info }))
        .or_else(|| {
            if value.eq_ignore_ascii_case("ok") {
                Some(Ok(info))
            } else {
                None
            }
        })
}

#[cfg(not(debug_assertions))]
fn test_peb_enable_override(_info: PebEnableInfo) -> Option<Result<PebEnableInfo, PebEnableError>> {
    None
}

#[cfg(debug_assertions)]
fn test_ifeo_enable_override() -> Option<Result<LoaderSnapsGuard, u32>> {
    let value = std::env::var("LOADWHAT_TEST_IFEO_ENABLE").ok()?;
    let value = value.trim();
    if value.eq_ignore_ascii_case("noop") || value.eq_ignore_ascii_case("ok-noop") {
        return Some(Ok(LoaderSnapsGuard {
            key_path: String::new(),
            original_value: None,
            restored: false,
            test_noop: true,
        }));
    }

    parse_test_error_code(value).map(Err)
}

#[cfg(not(debug_assertions))]
fn test_ifeo_enable_override() -> Option<Result<LoaderSnapsGuard, u32>> {
    None
}

#[cfg(debug_assertions)]
fn test_ifeo_restore_override() -> Option<u32> {
    std::env::var("LOADWHAT_TEST_IFEO_RESTORE")
        .ok()
        .and_then(|value| parse_test_error_code(value.trim()))
}

#[cfg(not(debug_assertions))]
fn test_ifeo_restore_override() -> Option<u32> {
    None
}

#[cfg(debug_assertions)]
fn parse_test_error_code(value: &str) -> Option<u32> {
    let trimmed = value.trim();
    let raw = trimmed
        .strip_prefix("fail:")
        .or_else(|| trimmed.strip_prefix("error:"))
        .unwrap_or(trimmed);
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }

    if let Some(hex) = raw.strip_prefix("0x").or_else(|| raw.strip_prefix("0X")) {
        return u32::from_str_radix(hex, 16).ok();
    }

    raw.parse::<u32>().ok()
}

#[cfg(test)]
mod tests {
    use super::{enable_via_peb, LoaderSnapsGuard, PebEnableError};
    use crate::test_util::EnvVarGuard;
    use crate::win::TEST_ENV_LOCK;

    #[test]
    fn enable_for_image_rejects_empty_name() {
        let result = LoaderSnapsGuard::enable_for_image("");
        assert_eq!(result.err(), Some(87));
    }

    #[test]
    fn enable_via_peb_rejects_null_process() {
        let _lock = TEST_ENV_LOCK.lock().expect("test env lock poisoned");
        let _guard = EnvVarGuard::remove("LOADWHAT_TEST_PEB_ENABLE");

        let result = enable_via_peb(std::ptr::null_mut());
        assert!(matches!(
            result,
            Err(PebEnableError::Win32 { code: 87, .. })
        ));
    }

    #[cfg(debug_assertions)]
    #[test]
    fn enable_for_image_uses_noop_override() {
        let _lock = TEST_ENV_LOCK.lock().expect("test env lock poisoned");
        let _guard = EnvVarGuard::set("LOADWHAT_TEST_IFEO_ENABLE", "noop");

        let guard = LoaderSnapsGuard::enable_for_image("app.exe")
            .expect("noop IFEO override should succeed");

        assert!(guard.test_noop);
    }

    #[cfg(debug_assertions)]
    #[test]
    fn enable_for_image_uses_ok_noop_override() {
        let _lock = TEST_ENV_LOCK.lock().expect("test env lock poisoned");
        let _guard = EnvVarGuard::set("LOADWHAT_TEST_IFEO_ENABLE", "ok-noop");

        let guard = LoaderSnapsGuard::enable_for_image("app.exe")
            .expect("ok-noop IFEO override should succeed");

        assert!(guard.test_noop);
    }

    #[cfg(debug_assertions)]
    #[test]
    fn enable_for_image_returns_decimal_override_error() {
        let _lock = TEST_ENV_LOCK.lock().expect("test env lock poisoned");
        let _guard = EnvVarGuard::set("LOADWHAT_TEST_IFEO_ENABLE", "fail:123");

        assert_eq!(
            LoaderSnapsGuard::enable_for_image("app.exe").err(),
            Some(123)
        );
    }

    #[cfg(debug_assertions)]
    #[test]
    fn enable_for_image_returns_hex_override_error() {
        let _lock = TEST_ENV_LOCK.lock().expect("test env lock poisoned");
        let _guard = EnvVarGuard::set("LOADWHAT_TEST_IFEO_ENABLE", "fail:0x0000007B");

        assert_eq!(
            LoaderSnapsGuard::enable_for_image("app.exe").err(),
            Some(123)
        );
    }

    #[cfg(debug_assertions)]
    #[test]
    fn restore_succeeds_for_test_noop_guard() {
        let _lock = TEST_ENV_LOCK.lock().expect("test env lock poisoned");
        let _enable_guard = EnvVarGuard::set("LOADWHAT_TEST_IFEO_ENABLE", "noop");
        let _restore_guard = EnvVarGuard::remove("LOADWHAT_TEST_IFEO_RESTORE");
        let mut guard = LoaderSnapsGuard::enable_for_image("app.exe")
            .expect("noop IFEO override should succeed");

        assert_eq!(guard.restore(), Ok(()));
        assert!(guard.restored);
    }

    #[cfg(debug_assertions)]
    #[test]
    fn restore_is_idempotent() {
        let _lock = TEST_ENV_LOCK.lock().expect("test env lock poisoned");
        let _enable_guard = EnvVarGuard::set("LOADWHAT_TEST_IFEO_ENABLE", "noop");
        let _restore_guard = EnvVarGuard::remove("LOADWHAT_TEST_IFEO_RESTORE");
        let mut guard = LoaderSnapsGuard::enable_for_image("app.exe")
            .expect("noop IFEO override should succeed");

        assert_eq!(guard.restore(), Ok(()));
        assert_eq!(guard.restore(), Ok(()));
        assert!(guard.restored);
    }

    #[cfg(debug_assertions)]
    #[test]
    fn restore_returns_override_error() {
        let _lock = TEST_ENV_LOCK.lock().expect("test env lock poisoned");
        let _enable_guard = EnvVarGuard::set("LOADWHAT_TEST_IFEO_ENABLE", "noop");
        let _restore_guard = EnvVarGuard::set("LOADWHAT_TEST_IFEO_RESTORE", "error:99");
        let mut guard = LoaderSnapsGuard::enable_for_image("app.exe")
            .expect("noop IFEO override should succeed");

        assert_eq!(guard.restore(), Err(99));
        assert!(!guard.restored);
    }

    #[cfg(debug_assertions)]
    #[test]
    fn enable_via_peb_override_reports_unsupported_wow64() {
        let _lock = TEST_ENV_LOCK.lock().expect("test env lock poisoned");
        let _guard = EnvVarGuard::set("LOADWHAT_TEST_PEB_ENABLE", "wow64");

        assert!(matches!(
            enable_via_peb(std::ptr::null_mut()),
            Err(PebEnableError::UnsupportedWow64)
        ));
    }

    #[cfg(debug_assertions)]
    #[test]
    fn enable_via_peb_override_returns_ok_info() {
        let _lock = TEST_ENV_LOCK.lock().expect("test env lock poisoned");
        let _guard = EnvVarGuard::set("LOADWHAT_TEST_PEB_ENABLE", "ok");

        let info = match enable_via_peb(std::ptr::null_mut()) {
            Ok(info) => info,
            Err(_) => panic!("PEB ok override should bypass process access"),
        };

        assert_eq!(info.ntglobalflag_offset, 0xBC);
    }

    #[cfg(debug_assertions)]
    #[test]
    fn enable_via_peb_override_returns_decimal_error() {
        let _lock = TEST_ENV_LOCK.lock().expect("test env lock poisoned");
        let _guard = EnvVarGuard::set("LOADWHAT_TEST_PEB_ENABLE", "fail:123");

        assert!(matches!(
            enable_via_peb(std::ptr::null_mut()),
            Err(PebEnableError::Win32 { code: 123, .. })
        ));
    }

    #[cfg(debug_assertions)]
    #[test]
    fn enable_via_peb_override_returns_hex_error() {
        let _lock = TEST_ENV_LOCK.lock().expect("test env lock poisoned");
        let _guard = EnvVarGuard::set("LOADWHAT_TEST_PEB_ENABLE", "fail:0x0000007B");

        assert!(matches!(
            enable_via_peb(std::ptr::null_mut()),
            Err(PebEnableError::Win32 { code: 123, .. })
        ));
    }

    #[cfg(debug_assertions)]
    #[test]
    fn parse_test_error_code_accepts_fail_prefix() {
        assert_eq!(super::parse_test_error_code("fail:99"), Some(99));
    }

    #[cfg(debug_assertions)]
    #[test]
    fn parse_test_error_code_accepts_error_prefix() {
        assert_eq!(super::parse_test_error_code("error:99"), Some(99));
    }

    #[cfg(debug_assertions)]
    #[test]
    fn parse_test_error_code_accepts_hex() {
        assert_eq!(super::parse_test_error_code("0x57"), Some(87));
    }

    #[cfg(debug_assertions)]
    #[test]
    fn parse_test_error_code_trims_whitespace() {
        assert_eq!(super::parse_test_error_code("  fail: 123  "), Some(123));
    }

    #[cfg(debug_assertions)]
    #[test]
    fn parse_test_error_code_rejects_invalid_text() {
        assert_eq!(super::parse_test_error_code("not-a-code"), None);
    }
}
