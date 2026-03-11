use std::env;
use std::fmt;
use std::path::PathBuf;
use std::process::{Command, Stdio};

#[derive(Clone, Debug)]
pub struct HarnessPaths {
    pub test_root: PathBuf,
    pub fixture_bin_root: PathBuf,
    pub loadwhat_bin: PathBuf,
    pub keep_artifacts: bool,
}

#[derive(Clone, Debug)]
pub enum HarnessSetupError {
    NotWindows,
    MissingEnvVar(&'static str),
    EmptyEnvVar(&'static str),
    LoadwhatBinaryMissing(PathBuf),
    LoadwhatProbeBlocked { raw_os_error: i32 },
    LoadwhatProbeFailed(String),
}

impl fmt::Display for HarnessSetupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotWindows => write!(f, "integration harness is Windows-only"),
            Self::MissingEnvVar(name) => write!(f, "missing environment variable {name}"),
            Self::EmptyEnvVar(name) => write!(f, "environment variable {name} is empty"),
            Self::LoadwhatBinaryMissing(path) => {
                write!(f, "loadwhat.exe was not found at {}", path.display())
            }
            Self::LoadwhatProbeBlocked { raw_os_error } => write!(
                f,
                "Windows blocked executing loadwhat.exe (raw_os_error={raw_os_error})"
            ),
            Self::LoadwhatProbeFailed(message) => write!(f, "{message}"),
        }
    }
}

#[allow(dead_code)]
pub fn from_env() -> Option<HarnessPaths> {
    try_from_env().ok()
}

pub fn try_from_env() -> Result<HarnessPaths, HarnessSetupError> {
    if !cfg!(windows) {
        return Err(HarnessSetupError::NotWindows);
    }

    let test_root = required_env_path("LOADWHAT_TEST_ROOT")?;
    let fixture_bin_root = required_env_path("LOADWHAT_FIXTURE_BIN_ROOT")?;

    let loadwhat_bin = match env::var_os("CARGO_BIN_EXE_loadwhat") {
        Some(value) => PathBuf::from(value),
        None => fallback_loadwhat_bin_path(),
    };

    if !loadwhat_bin.exists() {
        return Err(HarnessSetupError::LoadwhatBinaryMissing(loadwhat_bin));
    }

    match Command::new(&loadwhat_bin)
        .arg("--help")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
    {
        Ok(status) if status.success() => {}
        Ok(status) => {
            return Err(HarnessSetupError::LoadwhatProbeFailed(format!(
                "{} --help exited with status {}",
                loadwhat_bin.display(),
                status
                    .code()
                    .map(|code| code.to_string())
                    .unwrap_or_else(|| "terminated".to_string())
            )));
        }
        Err(error) if error.raw_os_error() == Some(4551) => {
            return Err(HarnessSetupError::LoadwhatProbeBlocked { raw_os_error: 4551 });
        }
        Err(error) => {
            return Err(HarnessSetupError::LoadwhatProbeFailed(format!(
                "failed to execute {} --help: {error}",
                loadwhat_bin.display()
            )));
        }
    }

    Ok(HarnessPaths {
        test_root,
        fixture_bin_root,
        loadwhat_bin,
        keep_artifacts: env::var_os("LOADWHAT_KEEP_TEST_ARTIFACTS").is_some(),
    })
}

pub fn require_from_env() -> HarnessPaths {
    match try_from_env() {
        Ok(paths) => paths,
        Err(error) => panic!("{}", format_require_message(&error)),
    }
}

fn required_env_path(name: &'static str) -> Result<PathBuf, HarnessSetupError> {
    let value = env::var_os(name).ok_or(HarnessSetupError::MissingEnvVar(name))?;
    if value.is_empty() {
        return Err(HarnessSetupError::EmptyEnvVar(name));
    }
    Ok(PathBuf::from(value))
}

fn fallback_loadwhat_bin_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("debug")
        .join("loadwhat.exe")
}

fn format_require_message(error: &HarnessSetupError) -> String {
    let mut message = String::new();
    message.push_str("test harness setup failed: ");
    message.push_str(&error.to_string());
    message.push_str("\n\n");
    message.push_str("Harness-dependent integration tests must be run with `cargo xtask test`.\n");
    message.push_str("Required harness environment:\n");
    message.push_str("- LOADWHAT_TEST_ROOT\n");
    message.push_str("- LOADWHAT_FIXTURE_BIN_ROOT\n");
    message.push_str("- LOADWHAT_TEST_MODE=1\n");
    message.push_str("\n");

    match error {
        HarnessSetupError::MissingEnvVar(name) | HarnessSetupError::EmptyEnvVar(name) => {
            message.push_str(&format!(
                "{name} was not provided by the harness. Re-run the suite with `cargo xtask test`.\n"
            ));
        }
        HarnessSetupError::LoadwhatBinaryMissing(path) => {
            message.push_str(&format!(
                "Expected loadwhat.exe at {}. Build and run through `cargo xtask test` so the harness sets the binary path correctly.\n",
                path.display()
            ));
        }
        HarnessSetupError::LoadwhatProbeBlocked { raw_os_error } => {
            message.push_str(&format!(
                "Windows blocked executing loadwhat.exe (raw_os_error={raw_os_error}). Check Smart App Control, Defender, and Mark-of-the-Web, then unblock or rebuild the binary and run `cargo xtask test` again.\n"
            ));
        }
        HarnessSetupError::LoadwhatProbeFailed(reason) => {
            message.push_str("The harness could not execute `loadwhat.exe --help`.\n");
            message.push_str(reason);
            message.push('\n');
            message.push_str("Rebuild with `cargo xtask test` and verify the binary is runnable on this machine.\n");
        }
        HarnessSetupError::NotWindows => {
            message.push_str("These integration tests are Windows-only.\n");
        }
    }

    message
}
