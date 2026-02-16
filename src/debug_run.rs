use std::ffi::{OsStr, OsString};
use std::mem;
use std::os::windows::ffi::OsStringExt;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::win;

#[derive(Clone)]
pub struct LoadedModule {
    pub dll_name: String,
    pub path: Option<PathBuf>,
    pub base: usize,
}

#[derive(Clone, Copy)]
pub enum RunEndKind {
    ExitProcess,
    Exception,
    Timeout,
}

pub struct RunOutcome {
    pub pid: u32,
    pub loaded_modules: Vec<LoadedModule>,
    pub end_kind: RunEndKind,
    pub exit_code: Option<u32>,
    pub exception_code: Option<u32>,
    pub elapsed_ms: u128,
}

pub fn run_target(
    exe_path: &Path,
    exe_args: &[OsString],
    cwd: Option<&Path>,
    timeout_ms: u32,
) -> Result<RunOutcome, String> {
    if !exe_path.exists() {
        return Err(format!("target does not exist: {}", exe_path.display()));
    }

    let app_w = win::to_wide(exe_path.as_os_str());
    let command_line = build_command_line(exe_path, exe_args);
    let mut cmd_w = win::to_wide(OsStr::new(&command_line));
    let cwd_w = cwd.map(|v| win::to_wide(v.as_os_str()));

    let mut si: win::StartupInfoW = unsafe { mem::zeroed() };
    si.cb = mem::size_of::<win::StartupInfoW>() as u32;
    let mut pi: win::ProcessInformation = unsafe { mem::zeroed() };

    let created = unsafe {
        win::CreateProcessW(
            app_w.as_ptr(),
            cmd_w.as_mut_ptr(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            0,
            win::DEBUG_ONLY_THIS_PROCESS,
            std::ptr::null_mut(),
            cwd_w
                .as_ref()
                .map(|v| v.as_ptr())
                .unwrap_or(std::ptr::null()),
            &mut si as *mut win::StartupInfoW,
            &mut pi as *mut win::ProcessInformation,
        )
    };

    if created == 0 {
        let code = unsafe { win::GetLastError() };
        return Err(format!(
            "CreateProcessW failed for {}: 0x{code:08X}",
            exe_path.display()
        ));
    }

    let start = Instant::now();
    let mut loaded_modules = Vec::new();
    let mut exit_code = None;
    let mut exception_code = None;
    let mut saw_exit = false;
    let mut timeout_hit = false;

    loop {
        let elapsed = start.elapsed().as_millis();
        if timeout_ms != 0 && elapsed >= timeout_ms as u128 {
            timeout_hit = true;
            break;
        }

        let remaining = timeout_ms.saturating_sub(elapsed as u32);
        let wait_ms = if timeout_ms == 0 {
            250
        } else {
            remaining.min(250)
        };

        let mut event: win::DebugEvent = unsafe { mem::zeroed() };
        let ok = unsafe { win::WaitForDebugEvent(&mut event as *mut win::DebugEvent, wait_ms) };
        if ok == 0 {
            let code = unsafe { win::GetLastError() };
            if code == win::WAIT_TIMEOUT || code == win::ERROR_SEM_TIMEOUT {
                continue;
            }
            close_if_needed(pi.h_thread);
            close_if_needed(pi.h_process);
            return Err(format!("WaitForDebugEvent failed: 0x{code:08X}"));
        }

        let mut continue_status = win::DBG_CONTINUE;
        match event.dw_debug_event_code {
            win::CREATE_PROCESS_DEBUG_EVENT => {
                let info = unsafe { event_data::<win::CreateProcessDebugInfo>(&event) };
                close_if_needed(info.h_file);
            }
            win::LOAD_DLL_DEBUG_EVENT => {
                let info = unsafe { event_data::<win::LoadDllDebugInfo>(&event) };
                let path_from_file = win::final_path_from_handle(info.h_file);
                let path_from_name =
                    read_remote_image_name(pi.h_process, info.lp_image_name, info.f_unicode != 0)
                        .map(PathBuf::from);
                let path = path_from_file.or(path_from_name);

                let dll_name = path
                    .as_ref()
                    .and_then(|v| v.file_name())
                    .map(|v| v.to_string_lossy().to_string())
                    .unwrap_or_else(|| format!("UNKNOWN_{:016X}", info.lp_base_of_dll as usize));

                loaded_modules.push(LoadedModule {
                    dll_name,
                    path,
                    base: info.lp_base_of_dll as usize,
                });
                close_if_needed(info.h_file);
            }
            win::EXCEPTION_DEBUG_EVENT => {
                let info = unsafe { event_data::<win::ExceptionDebugInfo>(&event) };
                if info.dw_first_chance == 0 {
                    exception_code = Some(info.exception_record.exception_code);
                }
                continue_status = win::DBG_EXCEPTION_NOT_HANDLED;
            }
            win::EXIT_PROCESS_DEBUG_EVENT => {
                let info = unsafe { event_data::<win::ExitProcessDebugInfo>(&event) };
                exit_code = Some(info.dw_exit_code);
                saw_exit = true;
            }
            _ => {}
        }

        let _ = unsafe {
            win::ContinueDebugEvent(event.dw_process_id, event.dw_thread_id, continue_status)
        };

        if saw_exit {
            break;
        }
    }

    close_if_needed(pi.h_thread);
    close_if_needed(pi.h_process);

    let end_kind = if saw_exit {
        if exception_code.is_some() {
            RunEndKind::Exception
        } else {
            RunEndKind::ExitProcess
        }
    } else if timeout_hit {
        RunEndKind::Timeout
    } else if exception_code.is_some() {
        RunEndKind::Exception
    } else {
        RunEndKind::Timeout
    };

    Ok(RunOutcome {
        pid: pi.dw_process_id,
        loaded_modules,
        end_kind,
        exit_code,
        exception_code,
        elapsed_ms: start.elapsed().as_millis(),
    })
}

unsafe fn event_data<T: Copy>(event: &win::DebugEvent) -> T {
    (event.data.as_ptr() as *const T).read_unaligned()
}

fn close_if_needed(handle: win::Handle) {
    if handle.is_null() || handle == win::INVALID_HANDLE_VALUE {
        return;
    }
    unsafe {
        let _ = win::CloseHandle(handle);
    }
}

fn build_command_line(exe_path: &Path, exe_args: &[OsString]) -> String {
    let mut parts = Vec::with_capacity(exe_args.len() + 1);
    parts.push(quote_cmd_arg(
        exe_path.as_os_str().to_string_lossy().as_ref(),
    ));
    for arg in exe_args {
        parts.push(quote_cmd_arg(&arg.to_string_lossy()));
    }
    parts.join(" ")
}

fn quote_cmd_arg(arg: &str) -> String {
    if !arg.contains([' ', '\t', '"']) {
        return arg.to_string();
    }

    let mut out = String::new();
    out.push('"');
    let mut slashes = 0usize;
    for c in arg.chars() {
        if c == '\\' {
            slashes += 1;
            continue;
        }
        if c == '"' {
            out.push_str(&"\\".repeat(slashes * 2 + 1));
            out.push('"');
            slashes = 0;
            continue;
        }
        if slashes > 0 {
            out.push_str(&"\\".repeat(slashes));
            slashes = 0;
        }
        out.push(c);
    }
    if slashes > 0 {
        out.push_str(&"\\".repeat(slashes * 2));
    }
    out.push('"');
    out
}

fn read_remote_image_name(
    process: win::Handle,
    image_name_ptr: win::Lpvoid,
    unicode: bool,
) -> Option<String> {
    if process.is_null() || image_name_ptr.is_null() {
        return None;
    }

    let mut remote_ptr: usize = 0;
    let mut bytes_read: usize = 0;
    let pointer_ok = unsafe {
        win::ReadProcessMemory(
            process,
            image_name_ptr as win::Lpcvoid,
            (&mut remote_ptr as *mut usize).cast::<std::ffi::c_void>(),
            std::mem::size_of::<usize>(),
            &mut bytes_read as *mut usize,
        )
    };
    if pointer_ok == 0 || remote_ptr == 0 || bytes_read != std::mem::size_of::<usize>() {
        return None;
    }

    if unicode {
        read_remote_utf16(process, remote_ptr as *const std::ffi::c_void)
    } else {
        read_remote_ansi(process, remote_ptr as *const std::ffi::c_void)
    }
}

fn read_remote_utf16(process: win::Handle, mut ptr: *const std::ffi::c_void) -> Option<String> {
    let mut data = Vec::new();
    for _ in 0..2048 {
        let mut ch: u16 = 0;
        let mut bytes_read = 0usize;
        let ok = unsafe {
            win::ReadProcessMemory(
                process,
                ptr,
                (&mut ch as *mut u16).cast::<std::ffi::c_void>(),
                std::mem::size_of::<u16>(),
                &mut bytes_read as *mut usize,
            )
        };
        if ok == 0 || bytes_read != std::mem::size_of::<u16>() {
            return None;
        }
        if ch == 0 {
            break;
        }
        data.push(ch);
        ptr = (ptr as usize + 2) as *const std::ffi::c_void;
    }
    if data.is_empty() {
        None
    } else {
        Some(OsString::from_wide(&data).to_string_lossy().to_string())
    }
}

fn read_remote_ansi(process: win::Handle, mut ptr: *const std::ffi::c_void) -> Option<String> {
    let mut data = Vec::new();
    for _ in 0..2048 {
        let mut ch: u8 = 0;
        let mut bytes_read = 0usize;
        let ok = unsafe {
            win::ReadProcessMemory(
                process,
                ptr,
                (&mut ch as *mut u8).cast::<std::ffi::c_void>(),
                std::mem::size_of::<u8>(),
                &mut bytes_read as *mut usize,
            )
        };
        if ok == 0 || bytes_read != std::mem::size_of::<u8>() {
            return None;
        }
        if ch == 0 {
            break;
        }
        data.push(ch);
        ptr = (ptr as usize + 1) as *const std::ffi::c_void;
    }
    if data.is_empty() {
        None
    } else {
        Some(String::from_utf8_lossy(&data).to_string())
    }
}
