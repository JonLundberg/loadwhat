use std::ffi::OsStr;

use crate::win;

const FLG_SHOW_LDR_SNAPS: u32 = 0x0000_0002;
const IFEO_BASE: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options";
const GLOBAL_FLAG_VALUE: &str = "GlobalFlag";

pub struct LoaderSnapsGuard {
    key_path: String,
    original_value: Option<(u32, Vec<u8>)>,
    restored: bool,
}

impl LoaderSnapsGuard {
    pub fn enable_for_image(image_name: &str) -> Result<Self, u32> {
        if image_name.is_empty() {
            return Err(87);
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
        })
    }

    pub fn restore(&mut self) -> Result<(), u32> {
        if self.restored {
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
