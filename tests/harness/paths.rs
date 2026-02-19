use std::env;
use std::path::PathBuf;
use std::process::{Command, Stdio};

#[derive(Clone, Debug)]
pub struct HarnessPaths {
    pub test_root: PathBuf,
    pub fixture_bin_root: PathBuf,
    pub loadwhat_bin: PathBuf,
    pub keep_artifacts: bool,
}

pub fn from_env() -> Option<HarnessPaths> {
    if !cfg!(windows) {
        return None;
    }

    let test_root = env::var_os("LOADWHAT_TEST_ROOT").map(PathBuf::from)?;
    let fixture_bin_root = env::var_os("LOADWHAT_FIXTURE_BIN_ROOT").map(PathBuf::from)?;
    if test_root.as_os_str().is_empty() || fixture_bin_root.as_os_str().is_empty() {
        return None;
    }

    let loadwhat_bin = match env::var_os("CARGO_BIN_EXE_loadwhat") {
        Some(value) => PathBuf::from(value),
        None => {
            let fallback = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("target")
                .join("debug")
                .join("loadwhat.exe");
            if fallback.exists() {
                fallback
            } else {
                return None;
            }
        }
    };

    if !loadwhat_bin.exists() {
        return None;
    }

    if let Err(error) = Command::new(&loadwhat_bin)
        .arg("--help")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
    {
        if error.raw_os_error() == Some(4551) {
            return None;
        }
    }

    Some(HarnessPaths {
        test_root,
        fixture_bin_root,
        loadwhat_bin,
        keep_artifacts: env::var_os("LOADWHAT_KEEP_TEST_ARTIFACTS").is_some(),
    })
}
