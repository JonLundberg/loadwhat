// Reconstructs the fixed v1 DLL search order and classifies search candidates deterministically.

use std::collections::HashSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::pe::{self, MachineType};
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
    /// Architecture of the process whose loader is being modeled.
    pub machine: MachineType,
}

#[derive(Clone)]
pub enum ResolutionKind {
    Found,
    Missing,
    BadImage,
}

/// Verdict on one search candidate. Unlike a corrupt image, a valid image of
/// the wrong architecture does not end the search: the loader skips it and
/// only fails with a bad-image error when nothing compatible is found later.
enum CandidateClass {
    Found,
    Missing,
    BadImage,
    WrongMachine,
}

impl CandidateClass {
    fn as_token(&self) -> &'static str {
        match self {
            CandidateClass::Found => "HIT",
            CandidateClass::Missing => "MISS",
            CandidateClass::BadImage => "BAD_IMAGE",
            CandidateClass::WrongMachine => "WRONG_MACHINE",
        }
    }
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
        machine: MachineType,
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

        let mut context = Self {
            app_dir: app_dir.to_path_buf(),
            cwd: cwd.to_path_buf(),
            path_dirs,
            safedll,
            system_dir,
            windows_dir,
            system16_dir,
            machine,
        };
        if machine == MachineType::X86 {
            context.apply_wow64_redirection(&win::get_system_wow64_directory()?);
        }
        Ok(context)
    }

    /// Rewrites every root the way WOW64 file-system redirection presents it
    /// to a 32-bit process: the native system directory becomes `wow64_dir`.
    fn apply_wow64_redirection(&mut self, wow64_dir: &Path) {
        let native_dir = self.system_dir.clone();
        let redirect = |dir: &Path| redirect_system_dir(dir, &native_dir, wow64_dir);

        self.app_dir = redirect(&self.app_dir);
        self.cwd = redirect(&self.cwd);
        self.path_dirs = self.path_dirs.iter().map(|dir| redirect(dir)).collect();
        self.system_dir = wow64_dir.to_path_buf();
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
        return resolve_absolute(&input, context.machine, &mut candidates);
    }

    let mut first_wrong_machine = None;
    let roots = context.ordered_roots();
    for (idx, root) in roots.iter().enumerate() {
        let candidate = root.join(dll_name);
        let class = classify_candidate(&candidate, context.machine);
        candidates.push(CandidateResult {
            order: idx + 1,
            path: candidate.clone(),
            result: class.as_token(),
        });
        match class {
            CandidateClass::Found => {
                return Resolution {
                    kind: ResolutionKind::Found,
                    chosen: Some(candidate),
                    candidates,
                }
            }
            CandidateClass::BadImage => {
                return Resolution {
                    kind: ResolutionKind::BadImage,
                    chosen: Some(candidate),
                    candidates,
                }
            }
            CandidateClass::WrongMachine => {
                first_wrong_machine.get_or_insert(candidate);
            }
            CandidateClass::Missing => {}
        }
    }

    Resolution {
        kind: if first_wrong_machine.is_some() {
            ResolutionKind::BadImage
        } else {
            ResolutionKind::Missing
        },
        chosen: first_wrong_machine,
        candidates,
    }
}

fn resolve_absolute(
    path: &Path,
    machine: MachineType,
    candidates: &mut Vec<CandidateResult>,
) -> Resolution {
    let class = classify_candidate(path, machine);
    candidates.push(CandidateResult {
        order: 1,
        path: path.to_path_buf(),
        result: class.as_token(),
    });

    let (kind, chosen) = match class {
        CandidateClass::Found => (ResolutionKind::Found, Some(path.to_path_buf())),
        CandidateClass::BadImage | CandidateClass::WrongMachine => {
            (ResolutionKind::BadImage, Some(path.to_path_buf()))
        }
        CandidateClass::Missing => (ResolutionKind::Missing, None),
    };
    Resolution {
        kind,
        chosen,
        candidates: candidates.clone(),
    }
}

fn classify_candidate(path: &Path, machine: MachineType) -> CandidateClass {
    if !path.exists() {
        return CandidateClass::Missing;
    }
    let Ok(data) = std::fs::read(path) else {
        return CandidateClass::BadImage;
    };
    match pe::is_loadable_by(&data, machine) {
        Ok(true) => CandidateClass::Found,
        Ok(false) => CandidateClass::WrongMachine,
        Err(_) => CandidateClass::BadImage,
    }
}

/// Maps `dir` to its WOW64 view when it is `native_dir` or a subdirectory.
fn redirect_system_dir(dir: &Path, native_dir: &Path, wow64_dir: &Path) -> PathBuf {
    let native = normalize_cmp(native_dir);
    let lower = normalize_cmp(dir);
    match lower.strip_prefix(&native) {
        Some(rest) if rest.is_empty() || rest.starts_with('\\') => {
            let original = dir.as_os_str().to_string_lossy().replace('/', "\\");
            // ASCII lowercasing keeps byte offsets aligned with the original.
            let tail = original[native.len()..].trim_end_matches('\\');
            PathBuf::from(format!("{}{tail}", wow64_dir.display()))
        }
        _ => dir.to_path_buf(),
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
    path.as_os_str()
        .to_string_lossy()
        .replace('/', "\\")
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::{redirect_system_dir, resolve_dll, ResolutionKind, SearchContext};
    use crate::pe::MachineType;
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
        let mut bytes = vec![0u8; 0x240];
        bytes[0..2].copy_from_slice(b"MZ");
        write_u32(&mut bytes, 0x3C, PE_OFFSET as u32);
        bytes[PE_OFFSET..PE_OFFSET + 4].copy_from_slice(b"PE\0\0");
        write_u16(&mut bytes, PE_OFFSET + 4, 0x8664);
        write_u16(&mut bytes, NUMBER_OF_SECTIONS_OFFSET, 1);
        write_u16(
            &mut bytes,
            SIZE_OF_OPTIONAL_HEADER_OFFSET,
            OPTIONAL_HEADER_SIZE,
        );
        write_u16(&mut bytes, OPTIONAL_HEADER_OFFSET, 0x020B);
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
            machine: MachineType::X64,
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
            machine: MachineType::X64,
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
            machine: MachineType::X64,
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

    fn write_pe_with_machine(path: &std::path::Path, machine: u16) {
        let mut bytes = build_valid_pe();
        write_u16(&mut bytes, PE_OFFSET + 4, machine);
        fs::write(path, bytes).expect("failed to write PE");
    }

    const MACHINE_X86: u16 = 0x014C;
    const MACHINE_X64: u16 = 0x8664;

    fn results(resolution: &super::Resolution) -> Vec<&'static str> {
        resolution.candidates.iter().map(|c| c.result).collect()
    }

    #[test]
    fn wrong_machine_candidate_does_not_stop_the_search() {
        let app = unique_temp_dir("wm-app");
        let path_dir = unique_temp_dir("wm-path");
        write_pe_with_machine(&app.join("dep.dll"), MACHINE_X64);
        write_pe_with_machine(&path_dir.join("dep.dll"), MACHINE_X86);

        let mut context = temp_context(app.clone(), app.clone(), vec![path_dir.clone()], true);
        context.machine = MachineType::X86;
        let resolution = resolve_dll("dep.dll", &context);

        assert!(matches!(resolution.kind, ResolutionKind::Found));
        assert_eq!(resolution.chosen, Some(path_dir.join("dep.dll")));
        let got = results(&resolution);
        assert_eq!(got.first(), Some(&"WRONG_MACHINE"));
        assert_eq!(got.last(), Some(&"HIT"));
    }

    #[test]
    fn only_wrong_machine_candidates_resolve_as_bad_image() {
        let app = unique_temp_dir("wm-only");
        write_pe_with_machine(&app.join("dep.dll"), MACHINE_X86);

        let context = temp_context(app.clone(), app.clone(), Vec::new(), true);
        let resolution = resolve_dll("dep.dll", &context);

        assert!(matches!(resolution.kind, ResolutionKind::BadImage));
        assert_eq!(resolution.chosen, Some(app.join("dep.dll")));
        assert_eq!(results(&resolution).first(), Some(&"WRONG_MACHINE"));
    }

    #[test]
    fn absolute_wrong_machine_path_resolves_as_bad_image() {
        let app = unique_temp_dir("wm-abs");
        let dll = app.join("dep.dll");
        write_pe_with_machine(&dll, MACHINE_X86);

        let context = temp_context(app.clone(), app.clone(), Vec::new(), true);
        let resolution = resolve_dll(&dll.to_string_lossy(), &context);

        assert!(matches!(resolution.kind, ResolutionKind::BadImage));
        assert_eq!(results(&resolution), vec!["WRONG_MACHINE"]);
    }

    #[test]
    fn corrupt_candidate_still_stops_the_search() {
        let app = unique_temp_dir("wm-corrupt");
        let path_dir = unique_temp_dir("wm-corrupt-path");
        fs::write(app.join("dep.dll"), b"not a pe").expect("failed to write corrupt DLL");
        write_pe_with_machine(&path_dir.join("dep.dll"), MACHINE_X64);

        let context = temp_context(app.clone(), app.clone(), vec![path_dir], true);
        let resolution = resolve_dll("dep.dll", &context);

        assert!(matches!(resolution.kind, ResolutionKind::BadImage));
        assert_eq!(resolution.chosen, Some(app.join("dep.dll")));
    }

    fn redirect(dir: &str) -> PathBuf {
        redirect_system_dir(
            &PathBuf::from(dir),
            &PathBuf::from(r"C:\Windows\system32"),
            &PathBuf::from(r"C:\Windows\SysWOW64"),
        )
    }

    #[test]
    fn redirect_system_dir_maps_directory_and_children_case_insensitively() {
        assert_eq!(
            redirect(r"C:\WINDOWS\System32"),
            PathBuf::from(r"C:\Windows\SysWOW64")
        );
        assert_eq!(
            redirect(r"c:\windows\system32\"),
            PathBuf::from(r"C:\Windows\SysWOW64")
        );
        assert_eq!(
            redirect(r"C:\Windows\System32\Wbem"),
            PathBuf::from(r"C:\Windows\SysWOW64\Wbem")
        );
    }

    #[test]
    fn redirect_system_dir_maps_forward_and_mixed_separators() {
        for dir in [
            "C:/WINDOWS/System32/Wbem/",
            r"C:\Windows\System32/Wbem",
            r"C:/Windows/System32\Wbem",
        ] {
            assert_eq!(redirect(dir), PathBuf::from(r"C:\Windows\SysWOW64\Wbem"));
        }
        assert_eq!(
            redirect("C:/Windows/System32/"),
            PathBuf::from(r"C:\Windows\SysWOW64")
        );
        assert_eq!(
            redirect("C:/Windows/System32Extra/Wbem"),
            PathBuf::from("C:/Windows/System32Extra/Wbem")
        );
    }

    #[test]
    fn wow64_redirection_resolves_dependency_from_forward_slash_path() {
        let temp = unique_temp_dir("wow64-forward-slash");
        let app = temp.join("app");
        let native = temp.join("System32");
        let wow64 = temp.join("SysWOW64");
        for dir in [&app, &native.join("Extra"), &wow64.join("Extra")] {
            fs::create_dir_all(dir).expect("failed to create search directory");
        }
        write_pe_with_machine(&native.join("Extra\\dep.dll"), MACHINE_X64);
        let expected = wow64.join("Extra\\dep.dll");
        write_pe_with_machine(&expected, MACHINE_X86);
        let path_dir = PathBuf::from(native.join("Extra").to_string_lossy().replace('\\', "/"));
        let mut context = temp_context(app.clone(), app, vec![path_dir], true);
        context.machine = MachineType::X86;
        context.system_dir = native;
        context.apply_wow64_redirection(&wow64);

        let resolution = resolve_dll("dep.dll", &context);

        assert!(matches!(resolution.kind, ResolutionKind::Found));
        assert_eq!(resolution.chosen, Some(expected));
        assert_eq!(results(&resolution).last(), Some(&"HIT"));
        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn redirect_system_dir_leaves_other_directories_alone() {
        assert_eq!(redirect(r"C:\App"), PathBuf::from(r"C:\App"));
        assert_eq!(
            redirect(r"C:\Windows\System32Extra"),
            PathBuf::from(r"C:\Windows\System32Extra")
        );
    }

    #[test]
    fn wow64_redirection_rewrites_and_dedups_search_roots() {
        let mut context = SearchContext {
            app_dir: PathBuf::from(r"C:\App"),
            cwd: PathBuf::from(r"C:\Windows\System32"),
            path_dirs: vec![
                PathBuf::from(r"C:\Windows\System32"),
                PathBuf::from("C:/Windows/SysWOW64"),
                PathBuf::from(r"C:\Windows\System32\Wbem"),
                PathBuf::from("C:/Windows/System32/Wbem"),
                PathBuf::from(r"C:\Tools"),
            ],
            safedll: true,
            system_dir: PathBuf::from(r"C:\Windows\System32"),
            windows_dir: PathBuf::from(r"C:\Windows"),
            system16_dir: None,
            machine: MachineType::X86,
        };
        context.apply_wow64_redirection(&PathBuf::from(r"C:\Windows\SysWOW64"));

        assert_eq!(
            ordered_root_strings(context),
            vec![
                r"C:\App",
                r"C:\Windows\SysWOW64",
                r"C:\Windows",
                r"C:\Windows\SysWOW64\Wbem",
                r"C:\Tools",
            ]
        );
    }
}
