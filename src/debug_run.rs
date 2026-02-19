use std::ffi::{OsStr, OsString};
use std::mem;
use std::os::windows::ffi::OsStringExt;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::{loader_snaps, win};

const STATUS_BREAKPOINT: u32 = 0x8000_0003;
const STATUS_SINGLE_STEP: u32 = 0x8000_0004;

#[derive(Clone)]
pub struct LoadedModule {
    pub dll_name: String,
    pub path: Option<PathBuf>,
    pub base: usize,
}

#[derive(Clone)]
pub struct DebugStringEvent {
    pub pid: u32,
    pub tid: u32,
    pub text: String,
}

#[derive(Clone)]
pub enum RuntimeEvent {
    RuntimeLoaded(LoadedModule),
    DebugString(DebugStringEvent),
}

#[derive(Clone, Copy)]
pub enum RunEndKind {
    ExitProcess,
    Exception,
    Timeout,
}

pub struct RunOutcome {
    pub pid: u32,
    pub runtime_events: Vec<RuntimeEvent>,
    pub loaded_modules: Vec<LoadedModule>,
    pub end_kind: RunEndKind,
    pub exit_code: Option<u32>,
    pub exception_code: Option<u32>,
    pub elapsed_ms: u128,
}

pub enum RunError {
    Message(String),
    PebLoaderSnapsEnableFailed(u32),
}

pub fn run_target(
    exe_path: &Path,
    exe_args: &[OsString],
    cwd: Option<&Path>,
    timeout_ms: u32,
    enable_loader_snaps_peb: bool,
) -> Result<RunOutcome, RunError> {
    if !exe_path.exists() {
        return Err(RunError::Message(format!(
            "target does not exist: {}",
            exe_path.display()
        )));
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
        return Err(RunError::Message(format!(
            "CreateProcessW failed for {}: 0x{code:08X}",
            exe_path.display()
        )));
    }

    if enable_loader_snaps_peb {
        if let Err(code) = loader_snaps::enable_via_peb(pi.h_process) {
            unsafe {
                let _ = win::TerminateProcess(pi.h_process, code);
            }
            close_if_needed(pi.h_thread);
            close_if_needed(pi.h_process);
            return Err(RunError::PebLoaderSnapsEnableFailed(code));
        }
    }

    let start = Instant::now();
    let mut runtime_events = Vec::new();
    let mut loaded_modules = Vec::new();
    let mut exit_code = None;
    let mut exception_code = None;
    let mut saw_terminal_exception = false;
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
            return Err(RunError::Message(format!(
                "WaitForDebugEvent failed: 0x{code:08X}"
            )));
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

                let module = LoadedModule {
                    dll_name,
                    path,
                    base: info.lp_base_of_dll as usize,
                };
                loaded_modules.push(module.clone());
                runtime_events.push(RuntimeEvent::RuntimeLoaded(module));
                close_if_needed(info.h_file);
            }
            win::OUTPUT_DEBUG_STRING_EVENT => {
                let info = unsafe { event_data::<win::OutputDebugStringInfo>(&event) };
                let text = read_output_debug_string(
                    pi.h_process,
                    info.lp_debug_string_data,
                    info.f_unicode != 0,
                    info.n_debug_string_length,
                )
                .unwrap_or_else(|| "UNREADABLE".to_string());

                runtime_events.push(RuntimeEvent::DebugString(DebugStringEvent {
                    pid: event.dw_process_id,
                    tid: event.dw_thread_id,
                    text,
                }));
            }
            win::EXCEPTION_DEBUG_EVENT => {
                let info = unsafe { event_data::<win::ExceptionDebugInfo>(&event) };
                let code = info.exception_record.exception_code;
                if code == STATUS_BREAKPOINT || code == STATUS_SINGLE_STEP {
                    continue_status = win::DBG_CONTINUE;
                } else {
                    if info.dw_first_chance == 0 {
                        exception_code = Some(code);
                        saw_terminal_exception = true;
                    }
                    continue_status = win::DBG_EXCEPTION_NOT_HANDLED;
                }
            }
            win::EXIT_PROCESS_DEBUG_EVENT => {
                let info = unsafe { event_data::<win::ExitProcessDebugInfo>(&event) };
                exit_code = Some(info.dw_exit_code);
                if exception_code.is_none() && (info.dw_exit_code & 0x8000_0000) != 0 {
                    exception_code = Some(info.dw_exit_code);
                }
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
        if saw_terminal_exception {
            RunEndKind::Exception
        } else {
            RunEndKind::ExitProcess
        }
    } else if timeout_hit {
        RunEndKind::Timeout
    } else if saw_terminal_exception {
        RunEndKind::Exception
    } else {
        RunEndKind::Timeout
    };

    Ok(RunOutcome {
        pid: pi.dw_process_id,
        runtime_events,
        loaded_modules,
        end_kind,
        exit_code,
        exception_code,
        elapsed_ms: start.elapsed().as_millis(),
    })
}

fn read_output_debug_string(
    process: win::Handle,
    data_ptr: win::Lpvoid,
    unicode: bool,
    length_chars: u16,
) -> Option<String> {
    if process.is_null() || data_ptr.is_null() {
        return None;
    }

    let max_chars = 16 * 1024usize;
    let chars = usize::from(length_chars);
    if chars == 0 {
        if unicode {
            return read_remote_utf16(process, data_ptr as *const std::ffi::c_void);
        }
        return read_remote_ansi(process, data_ptr as *const std::ffi::c_void);
    }
    let chars = chars.min(max_chars);

    if unicode {
        let bytes_to_read = chars.checked_mul(2)?;
        let mut data = vec![0u16; chars];
        let mut bytes_read = 0usize;
        let ok = unsafe {
            win::ReadProcessMemory(
                process,
                data_ptr as win::Lpcvoid,
                data.as_mut_ptr().cast::<std::ffi::c_void>(),
                bytes_to_read,
                &mut bytes_read as *mut usize,
            )
        };
        if ok == 0 || bytes_read == 0 {
            return None;
        }

        let units = bytes_read / std::mem::size_of::<u16>();
        let content = &data[..units.min(data.len())];
        let end = content
            .iter()
            .position(|v| *v == 0)
            .unwrap_or(content.len());
        return Some(OsString::from_wide(&content[..end]).to_string_lossy().to_string());
    }

    let mut data = vec![0u8; chars];
    let mut bytes_read = 0usize;
    let ok = unsafe {
        win::ReadProcessMemory(
            process,
            data_ptr as win::Lpcvoid,
            data.as_mut_ptr().cast::<std::ffi::c_void>(),
            chars,
            &mut bytes_read as *mut usize,
        )
    };
    if ok == 0 || bytes_read == 0 {
        return None;
    }

    data.truncate(bytes_read.min(data.len()));
    let end = data.iter().position(|v| *v == 0).unwrap_or(data.len());
    Some(String::from_utf8_lossy(&data[..end]).to_string())
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

#[cfg(test)]
mod tests {
    use super::{run_target, RunError, RuntimeEvent};
    use std::ffi::OsString;
    use std::path::PathBuf;

    fn system_exe(name: &str) -> PathBuf {
        let windir = std::env::var_os("WINDIR").unwrap_or_else(|| "C:\\Windows".into());
        PathBuf::from(windir).join("System32").join(name)
    }

    #[test]
    fn run_target_missing_path_returns_message_error() {
        let missing = PathBuf::from(r"C:\__loadwhat_tests__\definitely_missing.exe");
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let result = run_target(&missing, &[], Some(&cwd), 1000, false);
        match result {
            Err(RunError::Message(msg)) => {
                assert!(msg.contains("target does not exist"));
            }
            _ => panic!("expected missing-path RunError::Message"),
        }
    }

    #[test]
    fn run_target_loader_snaps_peb_captures_debug_strings() {
        let exe = system_exe("cmd.exe");
        assert!(exe.exists(), "expected {} to exist", exe.display());

        let args = vec![
            OsString::from("/C"),
            OsString::from("exit"),
            OsString::from("0"),
        ];
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let result = run_target(&exe, &args, Some(&cwd), 10_000, true);
        let outcome = match result {
            Ok(value) => value,
            Err(RunError::PebLoaderSnapsEnableFailed(code)) => {
                panic!("PEB loader-snaps enable failed: 0x{code:08X}")
            }
            Err(RunError::Message(msg)) => panic!("run_target failed: {msg}"),
        };

        let debug_count = outcome
            .runtime_events
            .iter()
            .filter(|event| matches!(event, RuntimeEvent::DebugString(_)))
            .count();
        assert!(
            debug_count > 0,
            "expected at least one DEBUG_STRING event with loader-snaps enabled"
        );
    }
}
