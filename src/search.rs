// Reconstructs the fixed v1 DLL search order and classifies search candidates deterministically.

use std::collections::HashSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::pe;
use crate::win;

#[derive(Clone)]
pub struct SearchContext {
    pub app_dir: PathBuf,
    pub cwd: PathBuf,
    pub path_dirs: Vec<PathBuf>,
    pub safedll: bool,
    pub system_dir: PathBuf,
    pub windows_dir: PathBuf,
    pub system16_dir: Option<PathBuf>,
}

#[derive(Clone)]
pub enum ResolutionKind {
    Found,
    Missing,
    BadImage,
}

#[derive(Clone)]
pub struct CandidateResult {
    pub order: usize,
    pub path: PathBuf,
    pub result: &'static str,
}

#[derive(Clone)]
pub struct Resolution {
    pub kind: ResolutionKind,
    pub chosen: Option<PathBuf>,
    pub candidates: Vec<CandidateResult>,
}

impl SearchContext {
    pub fn from_environment(
        app_dir: &Path,
        cwd: &Path,
        path_env: Option<OsString>,
    ) -> Result<Self, String> {
        let safedll = win::safe_dll_search_mode();
        let system_dir = win::get_system_directory()?;
        let windows_dir = win::get_windows_directory()?;
        let system16 = windows_dir.join("System");
        let system16_dir = if system16.exists() {
            Some(system16)
        } else {
            None
        };

        let path_dirs = path_env
            .or_else(|| std::env::var_os("PATH"))
            .map(parse_path_dirs)
            .unwrap_or_default();

        Ok(Self {
            app_dir: app_dir.to_path_buf(),
            cwd: cwd.to_path_buf(),
            path_dirs,
            safedll,
            system_dir,
            windows_dir,
            system16_dir,
        })
    }

    pub fn ordered_roots(&self) -> Vec<PathBuf> {
        let mut roots = Vec::new();
        roots.push(self.app_dir.clone());

        let cwd_differs = normalize_cmp(&self.cwd) != normalize_cmp(&self.app_dir);
        if self.safedll {
            roots.push(self.system_dir.clone());
            if let Some(system16) = &self.system16_dir {
                roots.push(system16.clone());
            }
            roots.push(self.windows_dir.clone());
            if cwd_differs {
                roots.push(self.cwd.clone());
            }
        } else {
            if cwd_differs {
                roots.push(self.cwd.clone());
            }
            roots.push(self.system_dir.clone());
            if let Some(system16) = &self.system16_dir {
                roots.push(system16.clone());
            }
            roots.push(self.windows_dir.clone());
        }

        for dir in &self.path_dirs {
            roots.push(dir.clone());
        }

        dedup_case_insensitive(roots)
    }
}

pub fn resolve_dll(dll_name: &str, context: &SearchContext) -> Resolution {
    let mut candidates = Vec::new();
    let input = PathBuf::from(dll_name);

    if input.is_absolute() {
        return resolve_absolute(&input, &mut candidates);
    }

    let roots = context.ordered_roots();
    for (idx, root) in roots.iter().enumerate() {
        let candidate = root.join(dll_name);
        let result = classify_candidate(&candidate);
        let token = match result {
            ResolutionKind::Found => "HIT",
            ResolutionKind::Missing => "MISS",
            ResolutionKind::BadImage => "BAD_IMAGE",
        };
        candidates.push(CandidateResult {
            order: idx + 1,
            path: candidate.clone(),
            result: token,
        });
        match result {
            ResolutionKind::Found => {
                return Resolution {
                    kind: ResolutionKind::Found,
                    chosen: Some(candidate),
                    candidates,
                }
            }
            ResolutionKind::BadImage => {
                return Resolution {
                    kind: ResolutionKind::BadImage,
                    chosen: Some(candidate),
                    candidates,
                }
            }
            ResolutionKind::Missing => {}
        }
    }

    Resolution {
        kind: ResolutionKind::Missing,
        chosen: None,
        candidates,
    }
}

fn resolve_absolute(path: &Path, candidates: &mut Vec<CandidateResult>) -> Resolution {
    let kind = classify_candidate(path);
    let token = match kind {
        ResolutionKind::Found => "HIT",
        ResolutionKind::Missing => "MISS",
        ResolutionKind::BadImage => "BAD_IMAGE",
    };
    candidates.push(CandidateResult {
        order: 1,
        path: path.to_path_buf(),
        result: token,
    });

    match kind {
        ResolutionKind::Found => Resolution {
            kind: ResolutionKind::Found,
            chosen: Some(path.to_path_buf()),
            candidates: candidates.clone(),
        },
        ResolutionKind::BadImage => Resolution {
            kind: ResolutionKind::BadImage,
            chosen: Some(path.to_path_buf()),
            candidates: candidates.clone(),
        },
        ResolutionKind::Missing => Resolution {
            kind: ResolutionKind::Missing,
            chosen: None,
            candidates: candidates.clone(),
        },
    }
}

fn classify_candidate(path: &Path) -> ResolutionKind {
    if !path.exists() {
        return ResolutionKind::Missing;
    }
    if pe::is_probably_pe_file(path) {
        ResolutionKind::Found
    } else {
        ResolutionKind::BadImage
    }
}

fn parse_path_dirs(path_env: OsString) -> Vec<PathBuf> {
    std::env::split_paths(&path_env)
        .filter(|value| !value.as_os_str().is_empty())
        .collect()
}

fn dedup_case_insensitive(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for path in paths {
        let normalized = normalize_cmp(&path);
        if seen.insert(normalized) {
            out.push(path);
        }
    }
    out
}

fn normalize_cmp(path: &Path) -> String {
    path.as_os_str().to_string_lossy().to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::SearchContext;
    use std::path::PathBuf;

    fn ordered_root_strings(context: SearchContext) -> Vec<String> {
        context
            .ordered_roots()
            .into_iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect()
    }

    fn sample_context(
        safedll: bool,
        system16_dir: Option<&str>,
        path_dirs: &[&str],
    ) -> SearchContext {
        SearchContext {
            app_dir: PathBuf::from(r"C:\app"),
            cwd: PathBuf::from(r"C:\cwd"),
            path_dirs: path_dirs.iter().map(PathBuf::from).collect(),
            safedll,
            system_dir: PathBuf::from(r"C:\Windows\System32"),
            windows_dir: PathBuf::from(r"C:\Windows"),
            system16_dir: system16_dir.map(PathBuf::from),
        }
    }

    #[test]
    fn search_order_safe_mode_enabled_places_cwd_after_windows() {
        let got = ordered_root_strings(sample_context(
            true,
            Some(r"C:\Windows\System"),
            &[r"C:\path1"],
        ));
        assert_eq!(
            got,
            vec![
                r"C:\app",
                r"C:\Windows\System32",
                r"C:\Windows\System",
                r"C:\Windows",
                r"C:\cwd",
                r"C:\path1",
            ]
        );
    }

    #[test]
    fn search_order_safe_mode_disabled_places_cwd_after_app_dir() {
        let got = ordered_root_strings(sample_context(
            false,
            Some(r"C:\Windows\System"),
            &[r"C:\path1"],
        ));
        assert_eq!(
            got,
            vec![
                r"C:\app",
                r"C:\cwd",
                r"C:\Windows\System32",
                r"C:\Windows\System",
                r"C:\Windows",
                r"C:\path1",
            ]
        );
    }

    #[test]
    fn search_order_skips_missing_system16_dir() {
        let got = ordered_root_strings(sample_context(false, None, &[r"C:\path1"]));
        assert_eq!(
            got,
            vec![
                r"C:\app",
                r"C:\cwd",
                r"C:\Windows\System32",
                r"C:\Windows",
                r"C:\path1",
            ]
        );
    }

    #[test]
    fn search_order_preserves_path_entry_order() {
        let got = ordered_root_strings(sample_context(
            true,
            Some(r"C:\Windows\System"),
            &[r"C:\path1", r"C:\path2", r"C:\path3"],
        ));
        assert_eq!(
            got,
            vec![
                r"C:\app",
                r"C:\Windows\System32",
                r"C:\Windows\System",
                r"C:\Windows",
                r"C:\cwd",
                r"C:\path1",
                r"C:\path2",
                r"C:\path3",
            ]
        );
    }
}
