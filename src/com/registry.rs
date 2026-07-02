// Injectable registry abstraction for COM diagnosis. Production reads the
// real Windows registry with explicit WOW64 view flags; tests inject a mock.

use super::{Hive, RegView};

/// Result of reading a single registry value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RegValue {
    String(String),
    ExpandString(String),
    Dword(u32),
    Binary(Vec<u8>),
    NotFound,
    AccessDenied,
    Error(u32),
}

/// A registry hive + view combination.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RegLocation {
    Hklm64,
    Hklm32,
    Hkcu64,
    Hkcu32,
}

impl RegLocation {
    pub fn of(hive: Hive, view: RegView) -> Self {
        match (hive, view) {
            (Hive::Hklm, RegView::V64) => RegLocation::Hklm64,
            (Hive::Hklm, RegView::V32) => RegLocation::Hklm32,
            (Hive::Hkcu, RegView::V64) => RegLocation::Hkcu64,
            (Hive::Hkcu, RegView::V32) => RegLocation::Hkcu32,
        }
    }
}

/// Abstraction over registry reads. Production code uses the real Windows
/// registry. Tests inject a mock with controlled data.
pub trait ComRegistry {
    /// Read a value from a specific hive/view. `name` of "" reads the default value.
    fn read_value(&self, location: RegLocation, subkey: &str, name: &str) -> RegValue;

    /// Check whether a subkey exists (without reading a value).
    fn key_exists(&self, location: RegLocation, subkey: &str) -> bool;

    /// Enumerate subkey names under a given key. Err carries the Win32 error code.
    fn enum_subkeys(&self, location: RegLocation, subkey: &str) -> Result<Vec<String>, u32>;
}

#[cfg(windows)]
pub use windows_impl::WindowsRegistry;

#[cfg(windows)]
mod windows_impl {
    use super::{ComRegistry, RegLocation, RegValue};
    use crate::win;
    use std::ffi::OsStr;

    /// Production registry reader over the Win32 registry FFI.
    pub struct WindowsRegistry;

    fn root_and_flag(location: RegLocation) -> (win::Hkey, win::Regsam) {
        match location {
            RegLocation::Hklm64 => (win::HKEY_LOCAL_MACHINE, win::KEY_WOW64_64KEY),
            RegLocation::Hklm32 => (win::HKEY_LOCAL_MACHINE, win::KEY_WOW64_32KEY),
            RegLocation::Hkcu64 => (win::HKEY_CURRENT_USER, win::KEY_WOW64_64KEY),
            RegLocation::Hkcu32 => (win::HKEY_CURRENT_USER, win::KEY_WOW64_32KEY),
        }
    }

    fn open_key(location: RegLocation, subkey: &str) -> Result<win::Hkey, u32> {
        let (root, flag) = root_and_flag(location);
        let wide = win::to_wide(OsStr::new(subkey));
        let mut key: win::Hkey = 0;
        let status =
            unsafe { win::RegOpenKeyExW(root, wide.as_ptr(), 0, win::KEY_READ | flag, &mut key) };
        if status == 0 {
            Ok(key)
        } else {
            Err(status as u32)
        }
    }

    impl ComRegistry for WindowsRegistry {
        fn read_value(&self, location: RegLocation, subkey: &str, name: &str) -> RegValue {
            let key = match open_key(location, subkey) {
                Ok(key) => key,
                Err(win::ERROR_FILE_NOT_FOUND) => return RegValue::NotFound,
                Err(win::ERROR_ACCESS_DENIED) => return RegValue::AccessDenied,
                Err(code) => return RegValue::Error(code),
            };

            let name_wide = win::to_wide(OsStr::new(name));
            let mut value_type: win::Dword = 0;
            let mut size: win::Dword = 0;
            let mut status = unsafe {
                win::RegQueryValueExW(
                    key,
                    name_wide.as_ptr(),
                    std::ptr::null_mut(),
                    &mut value_type,
                    std::ptr::null_mut(),
                    &mut size,
                )
            };

            let mut data = vec![0u8; size as usize];
            if status == 0 {
                let mut actual = size;
                status = unsafe {
                    win::RegQueryValueExW(
                        key,
                        name_wide.as_ptr(),
                        std::ptr::null_mut(),
                        &mut value_type,
                        data.as_mut_ptr(),
                        &mut actual,
                    )
                };
                data.truncate(actual as usize);
            }
            unsafe {
                win::RegCloseKey(key);
            }

            match status as u32 {
                0 => {}
                win::ERROR_FILE_NOT_FOUND => return RegValue::NotFound,
                win::ERROR_ACCESS_DENIED => return RegValue::AccessDenied,
                code => return RegValue::Error(code),
            }

            match value_type {
                win::REG_SZ | win::REG_EXPAND_SZ => {
                    let units: Vec<u16> = data
                        .chunks_exact(2)
                        .map(|c| u16::from_le_bytes([c[0], c[1]]))
                        .take_while(|&u| u != 0)
                        .collect();
                    let text = String::from_utf16_lossy(&units);
                    if value_type == win::REG_SZ {
                        RegValue::String(text)
                    } else {
                        RegValue::ExpandString(text)
                    }
                }
                win::REG_DWORD if data.len() >= 4 => {
                    RegValue::Dword(u32::from_le_bytes([data[0], data[1], data[2], data[3]]))
                }
                _ => RegValue::Binary(data),
            }
        }

        fn key_exists(&self, location: RegLocation, subkey: &str) -> bool {
            match open_key(location, subkey) {
                Ok(key) => {
                    unsafe {
                        win::RegCloseKey(key);
                    }
                    true
                }
                Err(_) => false,
            }
        }

        fn enum_subkeys(&self, location: RegLocation, subkey: &str) -> Result<Vec<String>, u32> {
            let key = open_key(location, subkey)?;
            let mut names = Vec::new();
            let mut index: win::Dword = 0;
            loop {
                let mut buffer = [0u16; 256];
                let mut len: win::Dword = buffer.len() as win::Dword;
                let status = unsafe {
                    win::RegEnumKeyExW(
                        key,
                        index,
                        buffer.as_mut_ptr(),
                        &mut len,
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                    )
                };
                match status as u32 {
                    0 => {
                        names.push(win::utf16_slice_to_string(&buffer[..len as usize]));
                        index += 1;
                    }
                    win::ERROR_NO_MORE_ITEMS => break,
                    win::ERROR_MORE_DATA => {
                        // Registry key names are capped at 255 characters, so a
                        // 256-unit buffer cannot legitimately overflow; skip defensively.
                        index += 1;
                    }
                    code => {
                        unsafe {
                            win::RegCloseKey(key);
                        }
                        return Err(code);
                    }
                }
            }
            unsafe {
                win::RegCloseKey(key);
            }
            Ok(names)
        }
    }
}

#[cfg(test)]
pub use mock::MockRegistry;

#[cfg(test)]
mod mock {
    use super::{ComRegistry, RegLocation, RegValue};
    use std::collections::{HashMap, HashSet};

    /// In-memory registry mock. Keys are stored as
    /// (RegLocation, subkey_lower, value_name_lower) -> RegValue.
    #[derive(Default)]
    pub struct MockRegistry {
        values: HashMap<(RegLocation, String, String), RegValue>,
        subkeys: HashMap<(RegLocation, String), Vec<String>>,
        denied: HashSet<(RegLocation, String)>,
    }

    impl MockRegistry {
        pub fn new() -> Self {
            Self::default()
        }

        /// Insert a value; subkey uses backslash separators. Parent key
        /// enumerations are auto-populated so key_exists/enum_subkeys work
        /// without separate setup.
        pub fn set(&mut self, loc: RegLocation, subkey: &str, name: &str, value: RegValue) {
            self.values.insert(
                (loc, subkey.to_ascii_lowercase(), name.to_ascii_lowercase()),
                value,
            );
            let mut current = subkey.to_string();
            while let Some((parent, child)) = current.rsplit_once('\\') {
                let entry = self
                    .subkeys
                    .entry((loc, parent.to_ascii_lowercase()))
                    .or_default();
                if !entry.iter().any(|e| e.eq_ignore_ascii_case(child)) {
                    entry.push(child.to_string());
                }
                current = parent.to_string();
            }
        }

        /// Mark a key as access-denied; reads at or under it fail.
        pub fn deny_access(&mut self, loc: RegLocation, subkey: &str) {
            self.denied.insert((loc, subkey.to_ascii_lowercase()));
        }

        fn is_denied(&self, loc: RegLocation, subkey: &str) -> bool {
            let lower = subkey.to_ascii_lowercase();
            let mut current = lower.as_str();
            loop {
                if self.denied.contains(&(loc, current.to_string())) {
                    return true;
                }
                match current.rsplit_once('\\') {
                    Some((parent, _)) => current = parent,
                    None => return false,
                }
            }
        }
    }

    impl ComRegistry for MockRegistry {
        fn read_value(&self, location: RegLocation, subkey: &str, name: &str) -> RegValue {
            if self.is_denied(location, subkey) {
                return RegValue::AccessDenied;
            }
            self.values
                .get(&(
                    location,
                    subkey.to_ascii_lowercase(),
                    name.to_ascii_lowercase(),
                ))
                .cloned()
                .unwrap_or(RegValue::NotFound)
        }

        fn key_exists(&self, location: RegLocation, subkey: &str) -> bool {
            if self.is_denied(location, subkey) {
                return false;
            }
            let lower = subkey.to_ascii_lowercase();
            self.values
                .keys()
                .any(|(l, k, _)| *l == location && *k == lower)
                || self.subkeys.contains_key(&(location, lower))
        }

        fn enum_subkeys(&self, location: RegLocation, subkey: &str) -> Result<Vec<String>, u32> {
            if self.is_denied(location, subkey) {
                return Err(crate::win::ERROR_ACCESS_DENIED);
            }
            let mut names = self
                .subkeys
                .get(&(location, subkey.to_ascii_lowercase()))
                .cloned()
                .unwrap_or_default();
            names.sort();
            Ok(names)
        }
    }
}
