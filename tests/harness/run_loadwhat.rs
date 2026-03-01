use std::ffi::OsString;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use super::paths::HarnessPaths;

#[derive(Debug)]
pub struct RunResult {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
}

pub fn run(
    paths: &HarnessPaths,
    current_dir: &Path,
    args: &[OsString],
    timeout: Duration,
) -> Result<RunResult, String> {
    let mut command = Command::new(&paths.loadwhat_bin);
    command
        .current_dir(current_dir)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command
        .spawn()
        .map_err(|e| format!("failed to spawn {}: {e}", paths.loadwhat_bin.display()))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "failed to capture child stdout".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "failed to capture child stderr".to_string())?;

    let stdout_thread = thread::spawn(move || read_to_string(stdout));
    let stderr_thread = thread::spawn(move || read_to_string(stderr));

    let start = Instant::now();
    let mut status = None;
    while start.elapsed() < timeout {
        match child.try_wait() {
            Ok(Some(value)) => {
                status = Some(value);
                break;
            }
            Ok(None) => {
                thread::sleep(Duration::from_millis(25));
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("try_wait failed: {e}"));
            }
        }
    }

    let timed_out = status.is_none();
    if timed_out {
        let _ = child.kill();
        status = Some(
            child
                .wait()
                .map_err(|e| format!("wait after kill failed: {e}"))?,
        );
    }

    let stdout = stdout_thread
        .join()
        .map_err(|_| "stdout reader thread panicked".to_string())??;
    let stderr = stderr_thread
        .join()
        .map_err(|_| "stderr reader thread panicked".to_string())??;

    Ok(RunResult {
        code: status.and_then(|s| s.code()),
        stdout,
        stderr,
        timed_out,
    })
}

fn read_to_string<R: Read>(mut reader: R) -> Result<String, String> {
    let mut bytes = Vec::new();
    reader
        .read_to_end(&mut bytes)
        .map_err(|e| format!("pipe read failed: {e}"))?;
    Ok(String::from_utf8_lossy(&bytes).to_string())
}
