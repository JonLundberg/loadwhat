use std::fs;
use std::path::{Path, PathBuf};

pub const HOST_STATIC_IMPORTS_A_EXE: &str = "host_static_imports_a.exe";
pub const HOST_STATIC_IMPORTS_MISSING_EXE: &str = "host_static_imports_missing.exe";
pub const HOST_STATIC_A_DEPENDS_ON_B_EXE: &str = "host_static_a_depends_on_b.exe";
pub const HOST_DYNAMIC_LOADLIBRARY_NAME_EXE: &str = "host_dynamic_loadlibrary_name.exe";
pub const HOST_DYNAMIC_LOADLIBRARY_FULLPATH_EXE: &str = "host_dynamic_loadlibrary_fullpath.exe";

pub const DLL_LWTEST_A: &str = "lwtest_a.dll";
pub const DLL_LWTEST_A_V1: &str = "lwtest_a_v1.dll";
pub const DLL_LWTEST_A_V2: &str = "lwtest_a_v2.dll";
pub const DLL_LWTEST_B: &str = "lwtest_b.dll";

pub fn fixture_path_from_root(
    fixture_bin_root: &Path,
    fixture_name: &str,
) -> Result<PathBuf, String> {
    let path = fixture_bin_root.join(fixture_name);
    if path.exists() {
        Ok(path)
    } else {
        Err(format!("fixture binary not found: {}", path.display()))
    }
}

pub fn copy_fixture_from_root(
    fixture_bin_root: &Path,
    fixture_name: &str,
    destination: &Path,
) -> Result<(), String> {
    let source = fixture_path_from_root(fixture_bin_root, fixture_name)?;
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            format!(
                "failed to create fixture destination directory {}: {e}",
                parent.display()
            )
        })?;
    }
    fs::copy(&source, destination).map_err(|e| {
        format!(
            "failed to copy fixture {} -> {}: {e}",
            source.display(),
            destination.display()
        )
    })?;
    Ok(())
}
