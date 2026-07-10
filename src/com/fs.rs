// Injectable file-system abstraction for COM server validation. Production
// wraps std::fs plus the v1 static dependency walk; tests inject a mock.

/// Classification of a failing dependency found while walking a COM server.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DepStatus {
    Missing,
    BadImage,
}

impl DepStatus {
    pub fn as_token(&self) -> &'static str {
        match self {
            DepStatus::Missing => "MISSING",
            DepStatus::BadImage => "BAD_IMAGE",
        }
    }
}

/// One search candidate tried while resolving a failing dependency;
/// used to reconstruct SEARCH_PATH trace lines.
#[derive(Clone, Debug)]
pub struct DepCandidate {
    pub order: usize,
    pub path: String,
    pub result: &'static str,
}

/// A failing dependency discovered during a COM server dependency walk.
#[derive(Clone, Debug)]
pub struct DepFailure {
    pub dll: String,
    pub via: String,
    pub depth: u32,
    pub status: DepStatus,
    pub candidates: Vec<DepCandidate>,
}

/// Result of a dependency walk over a COM server binary.
#[derive(Clone, Debug, Default)]
pub struct DepWalkReport {
    pub failures: Vec<DepFailure>,
    pub safedll: bool,
}

/// Fixed v1 DLL search inputs used while walking a COM server's imports.
#[derive(Clone, Debug)]
pub struct DepSearchContext {
    pub app_dir: String,
    pub cwd: String,
}

/// Abstraction over the file-system checks needed by COM diagnosis.
pub trait ComFileSystem {
    /// Check whether a file exists at the given path.
    fn file_exists(&self, path: &str) -> bool;

    /// Read up to `max_bytes` of the file for PE validation.
    /// Returns None if the file does not exist or is unreadable.
    fn read_file_header(&self, path: &str, max_bytes: usize) -> Option<Vec<u8>>;

    /// Run the deterministic transitive dependency walk over a server binary.
    fn walk_dependencies(
        &self,
        path: &str,
        context: &DepSearchContext,
    ) -> Result<DepWalkReport, String>;

    /// Extract the embedded RT_MANIFEST resource of a PE file, if any.
    fn embedded_manifest(&self, path: &str) -> Option<String>;
}

#[cfg(test)]
pub use mock::MockFileSystem;

#[cfg(test)]
mod mock {
    use super::{ComFileSystem, DepFailure, DepSearchContext, DepStatus, DepWalkReport};
    use std::collections::{HashMap, HashSet};

    /// In-memory file system mock. Paths are stored lowercased.
    #[derive(Default)]
    pub struct MockFileSystem {
        files: HashMap<String, Vec<u8>>,
        manifests: HashMap<String, String>,
    }

    impl MockFileSystem {
        pub fn new() -> Self {
            Self::default()
        }

        /// Add a raw file (for bad-image and sidecar-manifest testing).
        pub fn add_raw(&mut self, path: &str, content: Vec<u8>) {
            self.files.insert(path.to_ascii_lowercase(), content);
        }

        /// Add a minimal valid x64 PE with the given import table.
        pub fn add_pe(&mut self, path: &str, imports: &[&str]) {
            self.add_raw(path, crate::pe::testpe::build_test_pe(imports).bytes);
        }

        /// Add a minimal PE with an explicit machine type for bitness testing.
        pub fn add_pe_with_machine(&mut self, path: &str, machine: u16, imports: &[&str]) {
            let mut pe = crate::pe::testpe::build_test_pe(imports).bytes;
            crate::pe::testpe::write_u16(&mut pe, crate::pe::testpe::PE_OFFSET + 4, machine);
            self.add_raw(path, pe);
        }

        /// Attach an embedded manifest to a path (mock stand-in for RT_MANIFEST).
        pub fn set_embedded_manifest(&mut self, path: &str, xml: &str) {
            self.manifests
                .insert(path.to_ascii_lowercase(), xml.to_string());
        }
    }

    impl ComFileSystem for MockFileSystem {
        fn file_exists(&self, path: &str) -> bool {
            self.files.contains_key(&path.to_ascii_lowercase())
        }

        fn read_file_header(&self, path: &str, max_bytes: usize) -> Option<Vec<u8>> {
            self.files
                .get(&path.to_ascii_lowercase())
                .map(|data| data[..data.len().min(max_bytes)].to_vec())
        }

        /// Mock walk: dependencies resolve in the supplied application directory.
        fn walk_dependencies(
            &self,
            path: &str,
            context: &DepSearchContext,
        ) -> Result<DepWalkReport, String> {
            let root_lower = path.to_ascii_lowercase();
            let app_dir = context.app_dir.to_ascii_lowercase();
            let root_name = root_lower
                .rsplit('\\')
                .next()
                .unwrap_or(root_lower.as_str())
                .to_string();

            let mut failures = Vec::new();
            let mut visited: HashSet<String> = HashSet::new();
            let mut queue: Vec<(String, String, u32)> = Vec::new();
            visited.insert(root_lower.clone());
            queue.push((root_lower.clone(), root_name, 0));

            while let Some((module_path, module_name, depth)) = queue.pop() {
                let data = self
                    .files
                    .get(&module_path)
                    .ok_or_else(|| format!("mock file vanished: {module_path}"))?;
                let imports = crate::pe::direct_imports_from_bytes(data)
                    .map_err(|e| format!("failed to parse {module_path}: {e}"))?;
                for dll in imports {
                    if dll.starts_with("api-ms-win-") || dll.starts_with("ext-ms-win-") {
                        continue;
                    }
                    let candidate = format!("{app_dir}\\{dll}");
                    match self.files.get(&candidate) {
                        None => failures.push(DepFailure {
                            dll: dll.clone(),
                            via: module_name.clone(),
                            depth: depth + 1,
                            status: DepStatus::Missing,
                            candidates: Vec::new(),
                        }),
                        Some(bytes) => {
                            if crate::pe::direct_imports_from_bytes(bytes).is_err() {
                                failures.push(DepFailure {
                                    dll: dll.clone(),
                                    via: module_name.clone(),
                                    depth: depth + 1,
                                    status: DepStatus::BadImage,
                                    candidates: Vec::new(),
                                });
                            } else if visited.insert(candidate.clone()) {
                                queue.push((candidate, dll.clone(), depth + 1));
                            }
                        }
                    }
                }
            }

            Ok(DepWalkReport {
                failures,
                safedll: true,
            })
        }

        fn embedded_manifest(&self, path: &str) -> Option<String> {
            self.manifests.get(&path.to_ascii_lowercase()).cloned()
        }
    }
}
