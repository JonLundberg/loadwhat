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
    match pe::image_architecture(path) {
        Ok(pe::ImageArchitecture::X64) => ResolutionKind::Found,
        Ok(pe::ImageArchitecture::X86 | pe::ImageArchitecture::Other { .. }) | Err(_) => {
            ResolutionKind::BadImage
        }
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
    use super::{resolve_dll, ResolutionKind, SearchContext};
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEST_ID: AtomicU64 = AtomicU64::new(1);

    const PE_OFFSET: usize = 0x80;
    const OPTIONAL_HEADER_SIZE: u16 = 0xF0;
    const OPTIONAL_HEADER_OFFSET: usize = PE_OFFSET + 24;
    const DATA_DIR_START: usize = OPTIONAL_HEADER_OFFSET + 112;
    const SECTION_TABLE_OFFSET: usize = PE_OFFSET + 24 + OPTIONAL_HEADER_SIZE as usize;
    const NUMBER_OF_SECTIONS_OFFSET: usize = PE_OFFSET + 6;
    const SIZE_OF_OPTIONAL_HEADER_OFFSET: usize = PE_OFFSET + 20;

    fn build_valid_pe() -> Vec<u8> {
        build_pe(0x8664, 0x020B)
    }

    fn build_pe(machine: u16, magic: u16) -> Vec<u8> {
        let mut bytes = vec![0u8; 0x240];
        bytes[0..2].copy_from_slice(b"MZ");
        write_u32(&mut bytes, 0x3C, PE_OFFSET as u32);
        bytes[PE_OFFSET..PE_OFFSET + 4].copy_from_slice(b"PE\0\0");
        write_u16(&mut bytes, PE_OFFSET + 4, machine);
        write_u16(&mut bytes, NUMBER_OF_SECTIONS_OFFSET, 1);
        write_u16(
            &mut bytes,
            SIZE_OF_OPTIONAL_HEADER_OFFSET,
            OPTIONAL_HEADER_SIZE,
        );
        write_u16(&mut bytes, OPTIONAL_HEADER_OFFSET, magic);
        write_u32(&mut bytes, DATA_DIR_START + 8, 0);
        bytes[SECTION_TABLE_OFFSET..SECTION_TABLE_OFFSET + 5].copy_from_slice(b".text");
        write_u32(&mut bytes, SECTION_TABLE_OFFSET + 8, 0x40);
        write_u32(&mut bytes, SECTION_TABLE_OFFSET + 12, 0x1000);
        write_u32(&mut bytes, SECTION_TABLE_OFFSET + 16, 0x40);
        write_u32(&mut bytes, SECTION_TABLE_OFFSET + 20, 0x200);
        bytes
    }

    fn write_u16(bytes: &mut [u8], offset: usize, value: u16) {
        bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }

    fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn unique_temp_dir(name: &str) -> PathBuf {
        let id = NEXT_TEST_ID.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "loadwhat-search-{name}-{}-{id}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("failed to create temp dir");
        dir
    }

    fn temp_context(
        app_dir: PathBuf,
        cwd: PathBuf,
        path_dirs: Vec<PathBuf>,
        safedll: bool,
    ) -> SearchContext {
        let roots = unique_temp_dir("roots");
        let system_dir = roots.join("system32");
        let windows_dir = roots.join("windows");
        fs::create_dir_all(&system_dir).expect("failed to create system dir");
        fs::create_dir_all(&windows_dir).expect("failed to create windows dir");
        SearchContext {
            app_dir,
            cwd,
            path_dirs,
            safedll,
            system_dir,
            windows_dir,
            system16_dir: None,
        }
    }

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

    #[test]
    fn repeated_path_entries_collapse_case_insensitively() {
        let got = ordered_root_strings(sample_context(
            true,
            Some(r"C:\Windows\System"),
            &[r"C:\Path1", r"c:\path1", r"C:\PATH2"],
        ));
        assert_eq!(
            got,
            vec![
                r"C:\app",
                r"C:\Windows\System32",
                r"C:\Windows\System",
                r"C:\Windows",
                r"C:\cwd",
                r"C:\Path1",
                r"C:\PATH2",
            ]
        );
    }

    #[test]
    fn app_dir_and_cwd_equality_do_not_duplicate_roots() {
        let got = ordered_root_strings(SearchContext {
            app_dir: PathBuf::from(r"C:\same"),
            cwd: PathBuf::from(r"c:\SAME"),
            path_dirs: vec![PathBuf::from(r"C:\path1")],
            safedll: false,
            system_dir: PathBuf::from(r"C:\Windows\System32"),
            windows_dir: PathBuf::from(r"C:\Windows"),
            system16_dir: None,
        });
        assert_eq!(
            got,
            vec![
                r"C:\same",
                r"C:\Windows\System32",
                r"C:\Windows",
                r"C:\path1",
            ]
        );
    }

    #[test]
    fn earlier_bad_image_beats_later_valid_candidate() {
        let temp = unique_temp_dir("bad-image-first");
        let app_dir = temp.join("app");
        let path_dir = temp.join("path");
        fs::create_dir_all(&app_dir).expect("failed to create app dir");
        fs::create_dir_all(&path_dir).expect("failed to create path dir");
        fs::write(app_dir.join("foo.dll"), b"not a pe").expect("failed to create bad image");
        fs::write(path_dir.join("foo.dll"), build_valid_pe())
            .expect("failed to create valid image");

        let context = temp_context(
            app_dir.clone(),
            temp.join("cwd"),
            vec![path_dir.clone()],
            true,
        );
        let resolution = resolve_dll("foo.dll", &context);

        assert!(matches!(resolution.kind, ResolutionKind::BadImage));
        assert_eq!(resolution.chosen, Some(app_dir.join("foo.dll")));
        assert_eq!(resolution.candidates.len(), 1);
        assert_eq!(resolution.candidates[0].result, "BAD_IMAGE");

        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn later_bad_image_is_reported_after_misses() {
        let temp = unique_temp_dir("bad-image-late");
        let app_dir = temp.join("app");
        let cwd = temp.join("cwd");
        let path_dir = temp.join("path");
        fs::create_dir_all(&app_dir).expect("failed to create app dir");
        fs::create_dir_all(&cwd).expect("failed to create cwd");
        fs::create_dir_all(&path_dir).expect("failed to create path dir");
        fs::write(path_dir.join("foo.dll"), b"bad image").expect("failed to create bad image");

        let context = temp_context(app_dir, cwd, vec![path_dir.clone()], true);
        let resolution = resolve_dll("foo.dll", &context);

        assert!(matches!(resolution.kind, ResolutionKind::BadImage));
        assert_eq!(resolution.chosen, Some(path_dir.join("foo.dll")));
        assert_eq!(
            resolution
                .candidates
                .iter()
                .map(|candidate| candidate.order)
                .collect::<Vec<usize>>(),
            (1..=resolution.candidates.len()).collect::<Vec<usize>>()
        );
        assert_eq!(
            resolution
                .candidates
                .last()
                .map(|candidate| candidate.result),
            Some("BAD_IMAGE")
        );

        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn absolute_path_checks_only_requested_candidate_for_bad_image() {
        let temp = unique_temp_dir("absolute-bad-image");
        let dll = temp.join("absolute.dll");
        fs::write(&dll, b"bad image").expect("failed to create bad image");

        let context = temp_context(temp.join("app"), temp.join("cwd"), Vec::new(), true);
        let resolution = resolve_dll(&dll.display().to_string(), &context);

        assert!(matches!(resolution.kind, ResolutionKind::BadImage));
        assert_eq!(resolution.candidates.len(), 1);
        assert_eq!(resolution.candidates[0].path, dll);

        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn absolute_path_missing_reports_missing_without_extra_candidates() {
        let temp = unique_temp_dir("absolute-missing");
        let dll = temp.join("missing.dll");
        let context = temp_context(temp.join("app"), temp.join("cwd"), Vec::new(), true);
        let resolution = resolve_dll(&dll.display().to_string(), &context);

        assert!(matches!(resolution.kind, ResolutionKind::Missing));
        assert_eq!(resolution.candidates.len(), 1);
        assert_eq!(resolution.candidates[0].path, dll);
        assert_eq!(resolution.candidates[0].result, "MISS");

        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn x86_pe_candidate_is_bad_image_for_v1_x64_search() {
        let temp = unique_temp_dir("x86-bad-image");
        let app_dir = temp.join("app");
        fs::create_dir_all(&app_dir).expect("failed to create app dir");
        fs::write(app_dir.join("foo.dll"), build_pe(0x014C, 0x010B))
            .expect("failed to create x86 PE");

        let context = temp_context(app_dir.clone(), temp.join("cwd"), Vec::new(), true);
        let resolution = resolve_dll("foo.dll", &context);

        assert!(matches!(resolution.kind, ResolutionKind::BadImage));
        assert_eq!(resolution.chosen, Some(app_dir.join("foo.dll")));
        assert_eq!(resolution.candidates[0].result, "BAD_IMAGE");

        let _ = fs::remove_dir_all(temp);
    }
}
