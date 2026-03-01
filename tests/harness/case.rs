use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use super::fixture;
use super::paths::HarnessPaths;

static NEXT_CASE_ID: AtomicU64 = AtomicU64::new(1);

pub struct TestCase {
    root: PathBuf,
    fixture_bin_root: PathBuf,
    keep_artifacts: bool,
}

impl TestCase {
    pub fn new(paths: &HarnessPaths, case_name: &str) -> Result<Self, String> {
        let cases_root = paths.test_root.join("cases");
        fs::create_dir_all(&cases_root)
            .map_err(|e| format!("failed to create cases root {}: {e}", cases_root.display()))?;

        let id = NEXT_CASE_ID.fetch_add(1, Ordering::Relaxed);
        let root = cases_root.join(format!("{case_name}-{}-{id}", std::process::id()));
        fs::create_dir_all(&root)
            .map_err(|e| format!("failed to create test case root {}: {e}", root.display()))?;

        Ok(Self {
            root,
            fixture_bin_root: paths.fixture_bin_root.clone(),
            keep_artifacts: paths.keep_artifacts,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn mkdir(&self, relative: &str) -> Result<PathBuf, String> {
        let path = self.root.join(relative);
        fs::create_dir_all(&path)
            .map_err(|e| format!("failed to create directory {}: {e}", path.display()))?;
        Ok(path)
    }

    pub fn copy_fixture(
        &self,
        fixture_name: &str,
        destination_relative: &str,
    ) -> Result<PathBuf, String> {
        let destination = self.root.join(destination_relative);
        fixture::copy_fixture_from_root(&self.fixture_bin_root, fixture_name, &destination)?;
        Ok(destination)
    }

    pub fn copy_fixture_as(
        &self,
        fixture_name: &str,
        destination_directory_relative: &str,
        destination_name: &str,
    ) -> Result<PathBuf, String> {
        let destination = self
            .root
            .join(destination_directory_relative)
            .join(destination_name);
        fixture::copy_fixture_from_root(&self.fixture_bin_root, fixture_name, &destination)?;
        Ok(destination)
    }
}

impl Drop for TestCase {
    fn drop(&mut self) {
        if self.keep_artifacts {
            return;
        }
        let _ = fs::remove_dir_all(&self.root);
    }
}

pub fn os(path: &Path) -> OsString {
    path.as_os_str().to_os_string()
}
