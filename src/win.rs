use std::ffi::c_void;
use std::ffi::{OsStr, OsString};
use std::mem;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::PathBuf;

pub type Bool = i32;
pub type Byte = u8;
pub type Word = u16;
pub type Dword = u32;
pub type Uint = u32;
pub type UlongPtr = usize;
pub type SizeT = usize;
pub type Handle = *mut c_void;
pub type Hkey = isize;
pub type Regsam = u32;
pub type Lpvoid = *mut c_void;
pub type Lpcvoid = *const c_void;
pub type Lpwstr = *mut u16;
pub type Lpcwstr = *const u16;
pub type Ntstatus = i32;

pub const DEBUG_ONLY_THIS_PROCESS: Dword = 0x00000002;
pub const WAIT_TIMEOUT: Dword = 258;
pub const ERROR_SEM_TIMEOUT: Dword = 121;
pub const DBG_CONTINUE: Dword = 0x00010002;
pub const DBG_EXCEPTION_NOT_HANDLED: Dword = 0x80010001;
pub const INVALID_HANDLE_VALUE: Handle = -1isize as Handle;

pub const EXCEPTION_DEBUG_EVENT: Dword = 1;
pub const CREATE_PROCESS_DEBUG_EVENT: Dword = 3;
pub const EXIT_PROCESS_DEBUG_EVENT: Dword = 5;
pub const LOAD_DLL_DEBUG_EVENT: Dword = 6;
pub const OUTPUT_DEBUG_STRING_EVENT: Dword = 8;
pub const PROCESS_BASIC_INFORMATION_CLASS: u32 = 0;
pub const IMAGE_FILE_MACHINE_UNKNOWN: u16 = 0;

pub const HKEY_LOCAL_MACHINE: Hkey = 0x80000002u32 as isize;
pub const KEY_READ: Regsam = 0x00020019;
pub const KEY_SET_VALUE: Regsam = 0x00000002;
pub const REG_DWORD: Dword = 4;
pub const REG_OPTION_NON_VOLATILE: Dword = 0;
pub const ERROR_FILE_NOT_FOUND: Dword = 2;
pub const ERROR_INVALID_PARAMETER: Dword = 87;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct OsVersionInfoExW {
    pub dw_os_version_info_size: Dword,
    pub dw_major_version: Dword,
    pub dw_minor_version: Dword,
    pub dw_build_number: Dword,
    pub dw_platform_id: Dword,
    pub sz_csd_version: [u16; 128],
    pub w_service_pack_major: Word,
    pub w_service_pack_minor: Word,
    pub w_suite_mask: Word,
    pub w_product_type: u8,
    pub w_reserved: u8,
}

#[derive(Clone, Copy)]
pub struct OsVersion {
    pub major: u32,
    pub minor: u32,
    pub build: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct StartupInfoW {
    pub cb: Dword,
    pub lp_reserved: Lpwstr,
    pub lp_desktop: Lpwstr,
    pub lp_title: Lpwstr,
    pub dw_x: Dword,
    pub dw_y: Dword,
    pub dw_x_size: Dword,
    pub dw_y_size: Dword,
    pub dw_x_count_chars: Dword,
    pub dw_y_count_chars: Dword,
    pub dw_fill_attribute: Dword,
    pub dw_flags: Dword,
    pub w_show_window: Word,
    pub cb_reserved2: Word,
    pub lp_reserved2: *mut Byte,
    pub h_std_input: Handle,
    pub h_std_output: Handle,
    pub h_std_error: Handle,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ProcessInformation {
    pub h_process: Handle,
    pub h_thread: Handle,
    pub dw_process_id: Dword,
    pub dw_thread_id: Dword,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ExceptionRecord {
    pub exception_code: Dword,
    pub exception_flags: Dword,
    pub exception_record: *mut ExceptionRecord,
    pub exception_address: Lpvoid,
    pub number_parameters: Dword,
    pub exception_information: [UlongPtr; 15],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ExceptionDebugInfo {
    pub exception_record: ExceptionRecord,
    pub dw_first_chance: Dword,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct CreateProcessDebugInfo {
    pub h_file: Handle,
    pub h_process: Handle,
    pub h_thread: Handle,
    pub lp_base_of_image: Lpvoid,
    pub dw_debug_info_file_offset: Dword,
    pub n_debug_info_size: Dword,
    pub lp_thread_local_base: Lpvoid,
    pub lp_start_address: Lpvoid,
    pub lp_image_name: Lpvoid,
    pub f_unicode: Word,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ExitProcessDebugInfo {
    pub dw_exit_code: Dword,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct LoadDllDebugInfo {
    pub h_file: Handle,
    pub lp_base_of_dll: Lpvoid,
    pub dw_debug_info_file_offset: Dword,
    pub n_debug_info_size: Dword,
    pub lp_image_name: Lpvoid,
    pub f_unicode: Word,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct OutputDebugStringInfo {
    pub lp_debug_string_data: Lpvoid,
    pub f_unicode: Word,
    pub n_debug_string_length: Word,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ProcessBasicInformation {
    pub reserved1: Lpvoid,
    pub peb_base_address: Lpvoid,
    pub reserved2: [Lpvoid; 2],
    pub unique_process_id: usize,
    pub inherited_from_unique_process_id: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DebugEvent {
    pub dw_debug_event_code: Dword,
    pub dw_process_id: Dword,
    pub dw_thread_id: Dword,
    pub data: [u64; 22],
}

#[link(name = "kernel32")]
extern "system" {
    pub fn CreateProcessW(
        lp_application_name: Lpcwstr,
        lp_command_line: Lpwstr,
        lp_process_attributes: Lpvoid,
        lp_thread_attributes: Lpvoid,
        b_inherit_handles: Bool,
        dw_creation_flags: Dword,
        lp_environment: Lpvoid,
        lp_current_directory: Lpcwstr,
        lp_startup_info: *mut StartupInfoW,
        lp_process_information: *mut ProcessInformation,
    ) -> Bool;

    pub fn WaitForDebugEvent(lp_debug_event: *mut DebugEvent, dw_milliseconds: Dword) -> Bool;

    pub fn ContinueDebugEvent(
        dw_process_id: Dword,
        dw_thread_id: Dword,
        dw_continue_status: Dword,
    ) -> Bool;

    pub fn CloseHandle(h_object: Handle) -> Bool;

    pub fn GetLastError() -> Dword;

    pub fn ReadProcessMemory(
        h_process: Handle,
        lp_base_address: Lpcvoid,
        lp_buffer: Lpvoid,
        n_size: SizeT,
        lp_number_of_bytes_read: *mut SizeT,
    ) -> Bool;

    pub fn WriteProcessMemory(
        h_process: Handle,
        lp_base_address: Lpvoid,
        lp_buffer: Lpcvoid,
        n_size: SizeT,
        lp_number_of_bytes_written: *mut SizeT,
    ) -> Bool;

    pub fn TerminateProcess(h_process: Handle, u_exit_code: Uint) -> Bool;

    pub fn GetFinalPathNameByHandleW(
        h_file: Handle,
        lpsz_file_path: Lpwstr,
        cch_file_path: Dword,
        dw_flags: Dword,
    ) -> Dword;

    pub fn GetSystemDirectoryW(lp_buffer: Lpwstr, u_size: Uint) -> Uint;
    pub fn GetWindowsDirectoryW(lp_buffer: Lpwstr, u_size: Uint) -> Uint;
    pub fn IsWow64Process(h_process: Handle, wow64_process: *mut Bool) -> Bool;
    pub fn GetModuleHandleW(lp_module_name: Lpcwstr) -> Handle;
    pub fn GetProcAddress(h_module: Handle, lp_proc_name: *const u8) -> *const c_void;
}

#[link(name = "advapi32")]
extern "system" {
    pub fn RegOpenKeyExW(
        h_key: Hkey,
        lp_sub_key: Lpcwstr,
        ul_options: Dword,
        sam_desired: Regsam,
        phk_result: *mut Hkey,
    ) -> i32;

    pub fn RegCreateKeyExW(
        h_key: Hkey,
        lp_sub_key: Lpcwstr,
        reserved: Dword,
        lp_class: Lpwstr,
        dw_options: Dword,
        sam_desired: Regsam,
        lp_security_attributes: Lpvoid,
        phk_result: *mut Hkey,
        lpdw_disposition: *mut Dword,
    ) -> i32;

    pub fn RegQueryValueExW(
        h_key: Hkey,
        lp_value_name: Lpcwstr,
        lp_reserved: *mut Dword,
        lp_type: *mut Dword,
        lp_data: *mut Byte,
        lpcb_data: *mut Dword,
    ) -> i32;

    pub fn RegSetValueExW(
        h_key: Hkey,
        lp_value_name: Lpcwstr,
        reserved: Dword,
        dw_type: Dword,
        lp_data: *const Byte,
        cb_data: Dword,
    ) -> i32;

    pub fn RegDeleteValueW(h_key: Hkey, lp_value_name: Lpcwstr) -> i32;

    pub fn RegCloseKey(h_key: Hkey) -> i32;
}

#[link(name = "ntdll")]
extern "system" {
    pub fn NtQueryInformationProcess(
        process_handle: Handle,
        process_information_class: u32,
        process_information: Lpvoid,
        process_information_length: u32,
        return_length: *mut u32,
    ) -> Ntstatus;
    pub fn RtlGetVersion(lp_version_information: *mut OsVersionInfoExW) -> Ntstatus;
}

pub fn to_wide(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(std::iter::once(0)).collect()
}

pub fn utf16_slice_to_string(value: &[u16]) -> String {
    OsString::from_wide(value).to_string_lossy().to_string()
}

pub fn final_path_from_handle(handle: Handle) -> Option<PathBuf> {
    if handle.is_null() || handle == INVALID_HANDLE_VALUE {
        return None;
    }

    let mut buf = vec![0u16; 32768];
    let size =
        unsafe { GetFinalPathNameByHandleW(handle, buf.as_mut_ptr(), buf.len() as Dword, 0) };
    if size == 0 || size as usize >= buf.len() {
        return None;
    }

    let mut raw = utf16_slice_to_string(&buf[..size as usize]);
    if let Some(rest) = raw.strip_prefix(r"\\?\UNC\") {
        raw = format!(r"\\{rest}");
    } else if let Some(rest) = raw.strip_prefix(r"\\?\") {
        raw = rest.to_string();
    }

    Some(PathBuf::from(raw))
}

pub fn get_system_directory() -> Result<PathBuf, String> {
    let mut buf = vec![0u16; 32768];
    let size = unsafe { GetSystemDirectoryW(buf.as_mut_ptr(), buf.len() as Uint) };
    if size == 0 || size as usize >= buf.len() {
        let code = unsafe { GetLastError() };
        return Err(format!("GetSystemDirectoryW failed: 0x{code:08X}"));
    }
    Ok(PathBuf::from(utf16_slice_to_string(&buf[..size as usize])))
}

pub fn get_windows_directory() -> Result<PathBuf, String> {
    let mut buf = vec![0u16; 32768];
    let size = unsafe { GetWindowsDirectoryW(buf.as_mut_ptr(), buf.len() as Uint) };
    if size == 0 || size as usize >= buf.len() {
        let code = unsafe { GetLastError() };
        return Err(format!("GetWindowsDirectoryW failed: 0x{code:08X}"));
    }
    Ok(PathBuf::from(utf16_slice_to_string(&buf[..size as usize])))
}

pub fn safe_dll_search_mode() -> bool {
    let mut key: Hkey = 0;
    let path = to_wide(OsStr::new(
        r"SYSTEM\CurrentControlSet\Control\Session Manager",
    ));

    let open_status = unsafe {
        RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            path.as_ptr(),
            0,
            KEY_READ,
            &mut key as *mut Hkey,
        )
    };
    if open_status != 0 {
        return true;
    }

    let name = to_wide(OsStr::new("SafeDllSearchMode"));
    let mut data: Dword = 1;
    let mut data_type: Dword = 0;
    let mut data_size = std::mem::size_of::<Dword>() as Dword;
    let query_status = unsafe {
        RegQueryValueExW(
            key,
            name.as_ptr(),
            std::ptr::null_mut(),
            &mut data_type as *mut Dword,
            (&mut data as *mut Dword).cast::<Byte>(),
            &mut data_size as *mut Dword,
        )
    };
    unsafe {
        RegCloseKey(key);
    }

    if query_status == 0 && data_type == REG_DWORD {
        data != 0
    } else {
        true
    }
}

pub fn rtl_get_version() -> Option<OsVersion> {
    let mut info: OsVersionInfoExW = unsafe { mem::zeroed() };
    info.dw_os_version_info_size = mem::size_of::<OsVersionInfoExW>() as u32;
    let status = unsafe { RtlGetVersion(&mut info as *mut OsVersionInfoExW) };
    if status < 0 {
        return None;
    }
    Some(OsVersion {
        major: info.dw_major_version,
        minor: info.dw_minor_version,
        build: info.dw_build_number,
    })
}

pub fn is_wow64_process_best_effort(process: Handle) -> Result<bool, u32> {
    if process.is_null() {
        return Err(ERROR_INVALID_PARAMETER);
    }

    if let Some(value) = try_is_wow64_process2(process)? {
        return Ok(value);
    }

    let mut wow64: Bool = 0;
    let ok = unsafe { IsWow64Process(process, &mut wow64 as *mut Bool) };
    if ok == 0 {
        return Err(unsafe { GetLastError() });
    }
    Ok(wow64 != 0)
}

fn try_is_wow64_process2(process: Handle) -> Result<Option<bool>, u32> {
    type IsWow64Process2Fn = unsafe extern "system" fn(Handle, *mut u16, *mut u16) -> Bool;

    let kernel_name = to_wide(OsStr::new("kernel32.dll"));
    let kernel = unsafe { GetModuleHandleW(kernel_name.as_ptr()) };
    if kernel.is_null() {
        return Ok(None);
    }

    let proc = unsafe { GetProcAddress(kernel, b"IsWow64Process2\0".as_ptr()) };
    if proc.is_null() {
        return Ok(None);
    }

    let is_wow64_process2: IsWow64Process2Fn = unsafe { mem::transmute(proc) };
    let mut process_machine: u16 = 0;
    let mut native_machine: u16 = 0;
    let ok = unsafe {
        is_wow64_process2(
            process,
            &mut process_machine as *mut u16,
            &mut native_machine as *mut u16,
        )
    };
    if ok == 0 {
        return Err(unsafe { GetLastError() });
    }

    let is_wow64 = process_machine != IMAGE_FILE_MACHINE_UNKNOWN && native_machine != 0;
    Ok(Some(is_wow64))
}
