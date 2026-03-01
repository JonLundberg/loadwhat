#[cfg(not(windows))]
fn main() {
    eprintln!("loadwhat currently supports Windows only.");
    std::process::exit(22);
}

#[cfg(windows)]
mod cli;
#[cfg(windows)]
mod debug_run;
#[cfg(windows)]
mod emit;
#[cfg(windows)]
mod loader_snaps;
#[cfg(windows)]
mod pe;
#[cfg(windows)]
mod search;
#[cfg(windows)]
mod win;

#[cfg(windows)]
use std::collections::{HashSet, VecDeque};
#[cfg(windows)]
use std::env;
#[cfg(windows)]
use std::ffi::OsString;
#[cfg(windows)]
use std::path::{Path, PathBuf};

#[cfg(windows)]
use cli::{Command, ImportsOptions, RunOptions};
#[cfg(windows)]
use debug_run::{LoadedModule, RunEndKind, RunError, RunOutcome, RuntimeEvent};
#[cfg(windows)]
use emit::{emit, field, hex_u32, hex_usize, quote};
#[cfg(windows)]
use loader_snaps::{LoaderSnapsGuard, PebEnableInfo};
#[cfg(windows)]
use search::{CandidateResult, ResolutionKind, SearchContext};

#[cfg(windows)]
fn main() {
    if !cfg!(target_pointer_width = "64") {
        eprintln!("unsupported architecture: loadwhat v1 supports x64 only.");
        std::process::exit(22);
    }

    let command = match cli::parse() {
        Ok(value) => value,
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(20);
        }
    };

    let code = match command {
        Command::Run(opts) => run_command(opts),
        Command::Imports(opts) => imports_command(opts),
        Command::Help => {
            println!("{}", cli::usage());
            0
        }
    };
    std::process::exit(code);
}

#[cfg(windows)]
fn run_command(opts: RunOptions) -> i32 {
    let test_mode = test_mode_enabled();

    let exe_path = match normalize_existing_run_target(&opts.exe_path) {
        Ok(p) => p,
        Err(err) => {
            eprintln!("{err}");
            return 20;
        }
    };
    let cwd = opts
        .cwd
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    let (outcome, mut snaps_guard) = if opts.loader_snaps {
        match debug_run::run_target(&exe_path, &opts.exe_args, Some(&cwd), opts.timeout_ms, true) {
            Ok(value) => (Ok(value), None),
            Err(RunError::PebLoaderSnapsEnableFailed(peb_info, peb_code)) => {
                let image_name = exe_path
                    .file_name()
                    .map(|v| v.to_string_lossy().to_string())
                    .unwrap_or_default();
                let guard = match LoaderSnapsGuard::enable_for_image(&image_name) {
                    Ok(guard) => guard,
                    Err(code) => {
                        emit(
                            "NOTE",
                            &vec![
                                field("topic", quote("loader-snaps")),
                                field("detail", quote("enable-failed")),
                                field("code", hex_u32(code)),
                            ],
                        );
                        return if test_mode { 10 } else { 21 };
                    }
                };

                if opts.verbose {
                    emit_loader_snaps_peb_note(peb_info);
                    emit(
                        "NOTE",
                        &vec![
                            field("topic", quote("loader-snaps")),
                            field("detail", quote("peb-enable-failed")),
                            field("code", hex_u32(peb_code)),
                        ],
                    );
                }

                (
                    debug_run::run_target(
                        &exe_path,
                        &opts.exe_args,
                        Some(&cwd),
                        opts.timeout_ms,
                        false,
                    ),
                    Some(guard),
                )
            }
            Err(RunError::UnsupportedWow64Target) => {
                emit(
                    "NOTE",
                    &vec![
                        field("topic", quote("loader-snaps")),
                        field("detail", quote("wow64-target-unsupported")),
                        field(
                            "message",
                            quote("WOW64 target support is roadmap-only in v1"),
                        ),
                    ],
                );
                return 22;
            }
            Err(err) => (Err(err), None),
        }
    } else {
        (
            debug_run::run_target(
                &exe_path,
                &opts.exe_args,
                Some(&cwd),
                opts.timeout_ms,
                false,
            ),
            None,
        )
    };

    if let Some(mut guard) = snaps_guard.take() {
        if let Err(code) = guard.restore() {
            emit(
                "NOTE",
                &vec![
                    field("topic", quote("loader-snaps")),
                    field("detail", quote("restore-failed")),
                    field("code", hex_u32(code)),
                ],
            );
        }
    }

    let outcome = match outcome {
        Ok(value) => value,
        Err(RunError::Message(err)) => {
            eprintln!("{err}");
            return if test_mode { 10 } else { 21 };
        }
        Err(RunError::PebLoaderSnapsEnableFailed(_, code)) => {
            eprintln!("loader-snaps PEB enable failed: 0x{code:08X}");
            return if test_mode { 10 } else { 21 };
        }
        Err(RunError::UnsupportedWow64Target) => {
            emit(
                "NOTE",
                &vec![
                    field("topic", quote("loader-snaps")),
                    field("detail", quote("wow64-target-unsupported")),
                    field(
                        "message",
                        quote("WOW64 target support is roadmap-only in v1"),
                    ),
                ],
            );
            return 22;
        }
    };

    if opts.verbose && opts.loader_snaps {
        if let Some(info) = outcome.loader_snaps_peb {
            emit_loader_snaps_peb_note(info);
        }
    }

    if opts.verbose {
        emit_run_events(&exe_path, &cwd, &outcome);
    }

    let loaded_names: HashSet<String> = outcome
        .loaded_modules
        .iter()
        .map(|m| m.dll_name.to_ascii_lowercase())
        .collect();

    let mut first_break = false;
    let mut missing_or_bad = 0usize;
    let mut detected_missing_name: Option<String> = None;
    let mut dynamic_failure_seen = false;
    let loader_exception = outcome
        .exception_code
        .filter(|code| is_loader_related_code(*code));
    let heuristic_early_fail = matches!(outcome.end_kind, RunEndKind::ExitProcess)
        && outcome.exit_code.unwrap_or(0) != 0
        && outcome.elapsed_ms < 1500
        && outcome.loaded_modules.len() <= 6;

    if loader_exception.is_some() || heuristic_early_fail {
        let confidence = if loader_exception.is_some() {
            "HIGH"
        } else {
            "MEDIUM"
        };
        let mode = if opts.verbose {
            StaticEmitMode::Full
        } else {
            StaticEmitMode::FailuresOnly
        };
        let diag =
            diagnose_static_imports(&exe_path, &cwd, &loaded_names, env_path_override(&[]), mode);
        match diag {
            Ok(report) => {
                missing_or_bad = report.missing_or_bad;
                if let Some(issue) = &report.first_issue {
                    first_break = true;
                    if detected_missing_name.is_none() {
                        detected_missing_name = normalize_dll_basename(&issue.dll)
                            .or_else(|| Some(issue.dll.to_ascii_lowercase()));
                    }
                    if opts.verbose {
                        emit(
                            "FIRST_BREAK",
                            &vec![
                                field(
                                    "observed_exit_kind",
                                    quote(match outcome.end_kind {
                                        RunEndKind::ExitProcess => "EXIT_PROCESS",
                                        RunEndKind::Exception => "EXCEPTION",
                                        RunEndKind::Timeout => "TIMEOUT",
                                    }),
                                ),
                                field(
                                    "observed_code",
                                    match loader_exception {
                                        Some(code) => hex_u32(code),
                                        None => hex_u32(outcome.exit_code.unwrap_or(0)),
                                    },
                                ),
                                field("diagnosis", quote(issue.diagnosis)),
                                field("dll", quote(&issue.dll)),
                                field("confidence", quote(confidence)),
                            ],
                        );
                    } else {
                        emit(
                            "SEARCH_ORDER",
                            &vec![field("safedll", if report.safedll { "1" } else { "0" })],
                        );
                        match issue.kind {
                            ResolutionKind::Missing => {
                                let mut fields = vec![
                                    field("module", quote(&issue.module)),
                                    field("dll", quote(&issue.dll)),
                                    field("reason", quote("NOT_FOUND")),
                                ];
                                if issue.depth > 1 {
                                    fields.push(field("via", quote(&issue.via)));
                                    fields.push(field("depth", issue.depth.to_string()));
                                }
                                emit("STATIC_MISSING", &fields);
                            }
                            ResolutionKind::BadImage => {
                                emit(
                                    "STATIC_BAD_IMAGE",
                                    &vec![
                                        field("module", quote(&issue.module)),
                                        field("dll", quote(&issue.dll)),
                                        field("reason", quote("BAD_IMAGE")),
                                    ],
                                );
                            }
                            ResolutionKind::Found => {}
                        }
                        for candidate in &issue.candidates {
                            emit(
                                "SEARCH_PATH",
                                &vec![
                                    field("dll", quote(&issue.dll)),
                                    field("order", candidate.order.to_string()),
                                    field("path", quote(&display_path(&candidate.path))),
                                    field("result", quote(candidate.result)),
                                ],
                            );
                        }
                    }
                }
            }
            Err(err) => {
                if opts.verbose {
                    emit(
                        "NOTE",
                        &vec![field(
                            "detail",
                            quote(&format!("static diagnosis failed: {err}")),
                        )],
                    );
                } else {
                    eprintln!("{err}");
                }
            }
        }
    }

    // Dynamic (LoadLibrary) failures are observed via loader-snaps debug strings.
    if opts.loader_snaps && missing_or_bad == 0 {
        if let Some(dm) = detect_dynamic_missing_from_debug_strings(&outcome) {
            dynamic_failure_seen = true;
            if test_mode {
                if dm.dll.starts_with("lwtest_") {
                    detected_missing_name = Some(dm.dll.clone());
                }
            } else {
                let app_dir = exe_path.parent().unwrap_or_else(|| Path::new("."));
                if let Ok(context) =
                    SearchContext::from_environment(app_dir, &cwd, env_path_override(&[]))
                {
                    emit(
                        "SEARCH_ORDER",
                        &vec![field("safedll", if context.safedll { "1" } else { "0" })],
                    );

                    let mut fields = vec![
                        field("dll", quote(&dm.dll)),
                        field("reason", quote(dm.reason)),
                    ];
                    if let Some(st) = dm.status {
                        fields.push(field("status", hex_u32(st)));
                    }
                    emit("DYNAMIC_MISSING", &fields);

                    let resolution = search::resolve_dll(&dm.dll, &context);
                    for candidate in &resolution.candidates {
                        emit(
                            "SEARCH_PATH",
                            &vec![
                                field("dll", quote(&dm.dll)),
                                field("order", candidate.order.to_string()),
                                field("path", quote(&display_path(&candidate.path))),
                                field("result", quote(candidate.result)),
                            ],
                        );
                    }
                    missing_or_bad = 1;
                } else {
                    let mut fields = vec![
                        field("dll", quote(&dm.dll)),
                        field("reason", quote(dm.reason)),
                    ];
                    if let Some(st) = dm.status {
                        fields.push(field("status", hex_u32(st)));
                    }
                    emit("DYNAMIC_MISSING", &fields);
                    missing_or_bad = 1;
                }
            }
        }
    }

    if test_mode && detected_missing_name.is_none() {
        detected_missing_name = detect_missing_lwtest_dll_from_debug_strings(&outcome);
    }

    if test_mode {
        emit_lwtest_lines(
            &outcome.loaded_modules,
            detected_missing_name.as_deref(),
            outcome.exit_code,
        );
        let load_failure_detected = detected_missing_name.is_some()
            || missing_or_bad > 0
            || dynamic_failure_seen
            || loader_exception.is_some();
        return test_mode_exit_code(&outcome, load_failure_detected);
    }

    if opts.verbose {
        emit(
            "SUMMARY",
            &vec![
                field("first_break", if first_break { "true" } else { "false" }),
                field("missing_static", missing_or_bad.to_string()),
                field("runtime_loaded", outcome.loaded_modules.len().to_string()),
                field("com_issues", "0"),
            ],
        );
    }

    if missing_or_bad > 0 {
        10
    } else {
        match outcome.end_kind {
            RunEndKind::ExitProcess if outcome.exit_code == Some(0) => 0,
            RunEndKind::Timeout if !outcome.loaded_modules.is_empty() => 0,
            RunEndKind::ExitProcess | RunEndKind::Exception | RunEndKind::Timeout => 21,
        }
    }
}

#[cfg(windows)]
fn imports_command(opts: ImportsOptions) -> i32 {
    let module_path = match normalize_existing_path(&opts.module_path) {
        Ok(p) => p,
        Err(err) => {
            eprintln!("{err}");
            return 20;
        }
    };
    let cwd = opts
        .cwd
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    let diag = diagnose_static_imports(
        &module_path,
        &cwd,
        &HashSet::new(),
        env_path_override(&[]),
        StaticEmitMode::Full,
    );
    match diag {
        Ok(report) => {
            emit(
                "SUMMARY",
                &vec![
                    field("first_break", "false"),
                    field("missing_static", report.missing_or_bad.to_string()),
                    field("runtime_loaded", "0"),
                    field("com_issues", "0"),
                ],
            );
            if report.missing_or_bad > 0 {
                10
            } else {
                0
            }
        }
        Err(err) => {
            eprintln!("{err}");
            21
        }
    }
}

#[cfg(windows)]
fn emit_run_events(exe_path: &Path, cwd: &Path, outcome: &RunOutcome) {
    emit(
        "RUN_START",
        &vec![
            field("exe", quote(&display_path(exe_path))),
            field("cwd", quote(&display_path(cwd))),
            field("pid", outcome.pid.to_string()),
        ],
    );

    for event in &outcome.runtime_events {
        match event {
            RuntimeEvent::RuntimeLoaded(module) => {
                emit(
                    "RUNTIME_LOADED",
                    &vec![
                        field("pid", outcome.pid.to_string()),
                        field("dll", quote(&module.dll_name)),
                        field(
                            "path",
                            quote(
                                &module
                                    .path
                                    .as_ref()
                                    .map(|p| display_path(p))
                                    .unwrap_or_else(|| "UNKNOWN".to_string()),
                            ),
                        ),
                        field("base", hex_usize(module.base)),
                    ],
                );
            }
            RuntimeEvent::DebugString(debug) => {
                emit(
                    "DEBUG_STRING",
                    &vec![
                        field("pid", debug.pid.to_string()),
                        field("tid", debug.tid.to_string()),
                        field("source", quote("OUTPUT_DEBUG_STRING_EVENT")),
                        field("text", quote(&debug.text)),
                    ],
                );
            }
        }
    }

    let exit_kind = match outcome.end_kind {
        RunEndKind::ExitProcess => "EXIT_PROCESS",
        RunEndKind::Exception => "EXCEPTION",
        RunEndKind::Timeout => "TIMEOUT",
    };
    let code = outcome
        .exception_code
        .or(outcome.exit_code)
        .map(hex_u32)
        .unwrap_or_else(|| "0x00000000".to_string());
    emit(
        "RUN_END",
        &vec![
            field("pid", outcome.pid.to_string()),
            field("exit_kind", quote(exit_kind)),
            field("code", code),
        ],
    );
}

#[cfg(windows)]
fn emit_loader_snaps_peb_note(info: PebEnableInfo) {
    let os = match info.os_version {
        Some(v) => format!("{}.{}.{}", v.major, v.minor, v.build),
        None => "unknown".to_string(),
    };
    emit(
        "NOTE",
        &vec![
            field("topic", quote("loader-snaps")),
            field("detail", quote("peb-ntglobalflag")),
            field("os", quote(&os)),
            field(
                "ntglobalflag_offset",
                format!("0x{:X}", info.ntglobalflag_offset),
            ),
        ],
    );
}

#[cfg(windows)]
struct FirstIssue {
    module: String,
    via: String,
    depth: u32,
    dll: String,
    diagnosis: &'static str,
    kind: ResolutionKind,
    candidates: Vec<CandidateResult>,
}

#[cfg(windows)]
struct StaticReport {
    missing_or_bad: usize,
    first_issue: Option<FirstIssue>,
    safedll: bool,
}

#[cfg(windows)]
#[derive(Clone, Copy)]
enum StaticEmitMode {
    Full,
    FailuresOnly,
}

#[cfg(windows)]
struct WalkNode {
    module_path: PathBuf,
    module_name: String,
    depth: u32,
}

#[cfg(windows)]
fn diagnose_static_imports(
    module_path: &Path,
    cwd: &Path,
    runtime_loaded: &HashSet<String>,
    path_env_override: Option<OsString>,
    emit_mode: StaticEmitMode,
) -> Result<StaticReport, String> {
    let app_dir = module_path.parent().ok_or_else(|| {
        format!(
            "cannot determine app directory for {}",
            module_path.display()
        )
    })?;
    let context = SearchContext::from_environment(app_dir, cwd, path_env_override)?;
    let root_module_name = module_name_lower(module_path);

    if matches!(emit_mode, StaticEmitMode::Full) {
        emit(
            "STATIC_START",
            &vec![
                field("module", quote(&display_path(module_path))),
                field("scope", quote("direct-and-recursive-imports")),
            ],
        );
        emit(
            "SEARCH_ORDER",
            &vec![field("safedll", if context.safedll { "1" } else { "0" })],
        );
    }

    let mut missing = 0usize;
    let mut first_issue = None::<FirstIssue>;
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    let mut max_parent_depth_for_failures = None::<u32>;

    visited.insert(normalize_module_visit_key(module_path));
    queue.push_back(WalkNode {
        module_path: module_path.to_path_buf(),
        module_name: root_module_name,
        depth: 0,
    });

    while let Some(node) = queue.pop_front() {
        if matches!(emit_mode, StaticEmitMode::FailuresOnly) {
            if let Some(limit) = max_parent_depth_for_failures {
                if node.depth > limit {
                    break;
                }
            }
        }

        let imports = pe::direct_imports(&node.module_path)?;
        for dll in imports {
            if is_api_set_dll(&dll) {
                continue;
            }

            if matches!(emit_mode, StaticEmitMode::Full) {
                emit(
                    "STATIC_IMPORT",
                    &vec![
                        field("module", quote(&node.module_name)),
                        field("needs", quote(&dll)),
                    ],
                );
            }

            if node.depth == 0 && runtime_loaded.contains(&dll) {
                if matches!(emit_mode, StaticEmitMode::Full) {
                    emit(
                        "STATIC_FOUND",
                        &vec![
                            field("module", quote(&node.module_name)),
                            field("dll", quote(&dll)),
                            field("reason", quote("RUNTIME_OBSERVED")),
                        ],
                    );
                }
                continue;
            }

            let resolution = search::resolve_dll(&dll, &context);
            if matches!(emit_mode, StaticEmitMode::Full) {
                for candidate in &resolution.candidates {
                    emit(
                        "SEARCH_PATH",
                        &vec![
                            field("dll", quote(&dll)),
                            field("order", candidate.order.to_string()),
                            field("path", quote(&display_path(&candidate.path))),
                            field("result", quote(candidate.result)),
                        ],
                    );
                }
            }

            match &resolution.kind {
                ResolutionKind::Found => {
                    if matches!(emit_mode, StaticEmitMode::Full) {
                        emit(
                            "STATIC_FOUND",
                            &vec![
                                field("module", quote(&node.module_name)),
                                field("dll", quote(&dll)),
                                field(
                                    "path",
                                    quote(
                                        &resolution
                                            .chosen
                                            .as_ref()
                                            .map(|v| display_path(v))
                                            .unwrap_or_else(|| String::from("UNKNOWN")),
                                    ),
                                ),
                            ],
                        );
                    }

                    if let Some(chosen) = resolution.chosen.as_ref() {
                        let key = normalize_module_visit_key(chosen);
                        if visited.insert(key) {
                            queue.push_back(WalkNode {
                                module_path: chosen.clone(),
                                module_name: module_name_lower(chosen),
                                depth: node.depth + 1,
                            });
                        }
                    }
                }
                ResolutionKind::Missing => {
                    missing += 1;
                    let issue = FirstIssue {
                        module: node.module_name.clone(),
                        via: node.module_name.clone(),
                        depth: node.depth + 1,
                        dll: dll.clone(),
                        diagnosis: "MISSING_STATIC_IMPORT",
                        kind: ResolutionKind::Missing,
                        candidates: resolution.candidates.clone(),
                    };
                    consider_first_issue(&mut first_issue, issue);

                    if matches!(emit_mode, StaticEmitMode::Full) {
                        let mut fields = vec![
                            field("module", quote(&node.module_name)),
                            field("dll", quote(&dll)),
                            field("reason", quote("NOT_FOUND")),
                        ];
                        if node.depth + 1 > 1 {
                            fields.push(field("via", quote(&node.module_name)));
                            fields.push(field("depth", (node.depth + 1).to_string()));
                        }
                        emit("STATIC_MISSING", &fields);
                    }

                    if matches!(emit_mode, StaticEmitMode::FailuresOnly) {
                        max_parent_depth_for_failures.get_or_insert(node.depth);
                    }
                }
                ResolutionKind::BadImage => {
                    missing += 1;
                    let issue = FirstIssue {
                        module: node.module_name.clone(),
                        via: node.module_name.clone(),
                        depth: node.depth + 1,
                        dll: dll.clone(),
                        diagnosis: "BAD_STATIC_IMPORT_IMAGE",
                        kind: ResolutionKind::BadImage,
                        candidates: resolution.candidates.clone(),
                    };
                    consider_first_issue(&mut first_issue, issue);

                    if matches!(emit_mode, StaticEmitMode::Full) {
                        emit(
                            "STATIC_BAD_IMAGE",
                            &vec![
                                field("module", quote(&node.module_name)),
                                field("dll", quote(&dll)),
                                field("reason", quote("BAD_IMAGE")),
                            ],
                        );
                    }

                    if matches!(emit_mode, StaticEmitMode::FailuresOnly) {
                        max_parent_depth_for_failures.get_or_insert(node.depth);
                    }
                }
            }
        }
    }

    if matches!(emit_mode, StaticEmitMode::Full) {
        emit(
            "NOTE",
            &vec![field(
                "detail",
                quote("KnownDLLs/SxS/AddDllDirectory not modeled in v1"),
            )],
        );
        emit(
            "STATIC_END",
            &vec![field("module", quote(&display_path(module_path)))],
        );
    }

    Ok(StaticReport {
        missing_or_bad: missing,
        first_issue,
        safedll: context.safedll,
    })
}

#[cfg(windows)]
fn consider_first_issue(current: &mut Option<FirstIssue>, candidate: FirstIssue) {
    let replace = match current {
        None => true,
        Some(existing) => {
            (
                candidate.depth,
                candidate.via.as_str(),
                candidate.dll.as_str(),
            ) < (existing.depth, existing.via.as_str(), existing.dll.as_str())
        }
    };
    if replace {
        *current = Some(candidate);
    }
}

#[cfg(windows)]
fn is_api_set_dll(dll: &str) -> bool {
    let lower = dll.to_ascii_lowercase();
    lower.starts_with("api-ms-win-") || lower.starts_with("ext-ms-win-")
}

#[cfg(windows)]
fn module_name_lower(path: &Path) -> String {
    path.file_name()
        .map(|value| value.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_else(|| path.display().to_string().to_ascii_lowercase())
}

#[cfg(windows)]
fn normalize_module_visit_key(path: &Path) -> String {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    };
    let canonical = std::fs::canonicalize(path).unwrap_or(absolute);
    display_path(&canonical)
        .replace('/', "\\")
        .to_ascii_lowercase()
}

#[cfg(windows)]
fn normalize_existing_path(path: &Path) -> Result<PathBuf, String> {
    if !path.exists() {
        return Err(format!("path does not exist: {}", path.display()));
    }
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        let base = std::env::current_dir()
            .map_err(|e| format!("failed to read current directory: {e}"))?;
        Ok(base.join(path))
    }
}

#[cfg(windows)]
fn normalize_existing_run_target(path: &Path) -> Result<PathBuf, String> {
    if path.exists() {
        return normalize_existing_path(path);
    }

    if path.components().count() == 1 {
        let mut names = Vec::new();
        names.push(path.as_os_str().to_os_string());
        if path.extension().is_none() {
            let mut with_exe = path.as_os_str().to_os_string();
            with_exe.push(".exe");
            names.push(with_exe);
        }

        if let Some(path_env) = std::env::var_os("PATH") {
            for dir in std::env::split_paths(&path_env) {
                for name in &names {
                    let candidate = dir.join(name);
                    if candidate.exists() {
                        return Ok(candidate);
                    }
                }
            }
        }
    }

    Err(format!("path does not exist: {}", path.display()))
}

#[cfg(windows)]
fn test_mode_enabled() -> bool {
    env::var("LOADWHAT_TEST_MODE")
        .map(|v| v.trim() == "1")
        .unwrap_or(false)
}

#[cfg(windows)]
fn emit_lwtest_lines(modules: &[LoadedModule], missing_name: Option<&str>, exit_code: Option<u32>) {
    for module in modules {
        let dll_name = module.dll_name.to_ascii_lowercase();
        if !dll_name.starts_with("lwtest_") {
            continue;
        }
        let Some(path) = module.path.as_ref() else {
            continue;
        };
        emit(
            "LWTEST:LOAD",
            &vec![field("name", dll_name), field("path", display_path(path))],
        );
    }

    if let Some(name) = missing_name {
        emit(
            "LWTEST:RESULT",
            &vec![
                field("kind", "missing_dll"),
                field("name", name.to_ascii_lowercase()),
            ],
        );
    }

    if let Some(code) = exit_code {
        emit("LWTEST:TARGET", &vec![field("exit_code", code.to_string())]);
    }
}

#[cfg(windows)]
fn test_mode_exit_code(outcome: &RunOutcome, load_failure_detected: bool) -> i32 {
    if matches!(outcome.end_kind, RunEndKind::Timeout) {
        return 3;
    }
    if load_failure_detected || matches!(outcome.end_kind, RunEndKind::Exception) {
        return 2;
    }
    0
}

#[cfg(windows)]
fn detect_missing_lwtest_dll_from_debug_strings(outcome: &RunOutcome) -> Option<String> {
    detect_dynamic_missing_from_debug_strings(outcome).and_then(|v| {
        if v.dll.starts_with("lwtest_") {
            Some(v.dll)
        } else {
            None
        }
    })
}

#[cfg(windows)]
#[derive(Debug, Clone)]
struct DynamicMissing {
    dll: String,
    reason: &'static str,
    status: Option<u32>,
}

#[cfg(windows)]
fn detect_dynamic_missing_from_debug_strings(outcome: &RunOutcome) -> Option<DynamicMissing> {
    let mut last_load_candidate: Option<String> = None;
    let mut best: Option<(i32, usize, DynamicMissing)> = None;

    for (idx, event) in outcome.runtime_events.iter().enumerate() {
        let RuntimeEvent::DebugString(debug) = event else {
            continue;
        };

        let raw = debug.text.trim();
        if raw.is_empty() {
            continue;
        }

        let lower = raw.to_ascii_lowercase();
        let dlls = extract_dll_basenames(&lower);
        if !dlls.is_empty() && looks_like_load_attempt(&lower) {
            last_load_candidate = pick_best_dll(&dlls).or_else(|| dlls.first().cloned());
        }

        if is_ignored_probe_line(&lower) {
            continue;
        }

        let score = failure_score(&lower);
        if score <= 0 {
            continue;
        }

        let status = extract_first_hex_u32(&lower);
        let candidate = extract_unable_to_load_dll(&lower)
            .or_else(|| pick_best_dll(&dlls).or_else(|| dlls.first().cloned()))
            .or_else(|| last_load_candidate.clone());
        let Some(dll) = candidate else {
            continue;
        };

        let reason = match status {
            Some(0xC0000135) | Some(0x8007007E) => "NOT_FOUND",
            Some(0xC000007B) | Some(0x800700C1) => "BAD_IMAGE",
            _ => {
                if lower.contains("not found")
                    || lower.contains("could not be found")
                    || lower.contains("file not found")
                {
                    "NOT_FOUND"
                } else if lower.contains("bad image") || lower.contains("invalid image") {
                    "BAD_IMAGE"
                } else {
                    "OTHER"
                }
            }
        };

        let dll = if is_noise_dll(&dll) {
            pick_best_dll(&dlls)
                .or_else(|| last_load_candidate.clone())
                .unwrap_or(dll)
        } else {
            dll
        };

        let detected = DynamicMissing {
            dll,
            reason,
            status,
        };
        let replace = match &best {
            None => true,
            Some((best_score, best_idx, _)) => {
                score > *best_score || (score == *best_score && idx > *best_idx)
            }
        };
        if replace {
            best = Some((score, idx, detected));
        }
    }

    best.map(|(_, _, dm)| dm)
}

#[cfg(windows)]
fn looks_like_load_attempt(text_lower: &str) -> bool {
    text_lower.contains(".dll")
        && (text_lower.contains("dll name:")
            || text_lower.contains("ldrloaddll - enter")
            || text_lower.contains("loadlibrary"))
}

#[cfg(windows)]
fn is_ignored_probe_line(text_lower: &str) -> bool {
    text_lower.contains("ldrpfindknowndll - return")
        || text_lower.contains("ldrpresolvedllname - return")
        || text_lower.contains("ldrpresolvefilename - return")
        || text_lower.contains("ldrpfindloadeddllinternal - return")
}

#[cfg(windows)]
fn has_loader_failure_code(text_lower: &str) -> bool {
    text_lower.contains("0xc0000135")
        || text_lower.contains("0x8007007e")
        || text_lower.contains("0xc000007b")
        || text_lower.contains("0x800700c1")
        || text_lower.contains("0xc0000139")
        || text_lower.contains("0xc0000142")
}

#[cfg(windows)]
fn failure_score(text_lower: &str) -> i32 {
    if text_lower.contains("ldrpprocesswork - error: unable to load dll") {
        return 100;
    }
    if text_lower.contains("- error: unable to load dll") {
        return 95;
    }
    if text_lower.contains("walking the import tables") {
        return 90;
    }
    if text_lower.contains("process initialization failed")
        || text_lower.contains("_ldrpinitialize - error")
    {
        return 85;
    }
    if text_lower.contains("ldrloaddll") && text_lower.contains("failed") {
        return 80;
    }
    if text_lower.contains("ldrpsearchpath - return") && has_loader_failure_code(text_lower) {
        return 70;
    }
    0
}

#[cfg(windows)]
fn extract_unable_to_load_dll(text_lower: &str) -> Option<String> {
    let marker = "unable to load dll:";
    let idx = text_lower.find(marker)?;
    let rest = text_lower[idx + marker.len()..].trim_start();
    if rest.is_empty() {
        return None;
    }

    let candidate_text = if let Some(stripped) = rest.strip_prefix('"') {
        stripped.split('"').next().unwrap_or(stripped)
    } else if let Some(stripped) = rest.strip_prefix('\'') {
        stripped.split('\'').next().unwrap_or(stripped)
    } else {
        rest.split(',').next().unwrap_or(rest)
    };

    extract_dll_basenames(candidate_text).into_iter().next()
}

#[cfg(windows)]
fn pick_best_dll(dlls: &[String]) -> Option<String> {
    for dll in dlls {
        if !is_noise_dll(dll) {
            return Some(dll.clone());
        }
    }
    None
}

#[cfg(windows)]
fn is_noise_dll(dll_lower_basename: &str) -> bool {
    let d = dll_lower_basename;
    d.starts_with("api-ms-win-")
        || d.starts_with("ext-ms-")
        || matches!(
            d,
            "ntdll.dll"
                | "kernel32.dll"
                | "kernelbase.dll"
                | "user32.dll"
                | "gdi32.dll"
                | "advapi32.dll"
                | "sechost.dll"
                | "msvcrt.dll"
                | "ucrtbase.dll"
        )
}

#[cfg(windows)]
fn extract_first_hex_u32(text_lower: &str) -> Option<u32> {
    let bytes = text_lower.as_bytes();
    let mut i = 0usize;
    while i + 10 <= bytes.len() {
        if bytes[i] == b'0' && bytes[i + 1] == b'x' {
            let slice = &text_lower[i + 2..i + 10];
            if slice.chars().all(|c| c.is_ascii_hexdigit()) {
                if let Ok(v) = u32::from_str_radix(slice, 16) {
                    return Some(v);
                }
            }
        }
        i += 1;
    }
    None
}

#[cfg(windows)]
fn extract_dll_basenames(text_lower: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut offset = 0usize;

    while let Some(rel) = text_lower[offset..].find(".dll") {
        let dll_end = offset + rel + 4;

        let mut start = offset + rel;
        while start > 0 {
            let c = text_lower.as_bytes()[start - 1] as char;
            let ok = c.is_ascii_alphanumeric()
                || c == '_'
                || c == '.'
                || c == '-'
                || c == '\\'
                || c == '/'
                || c == ':';
            if !ok {
                break;
            }
            start -= 1;
        }

        let token = text_lower[start..dll_end]
            .trim_matches(|c: char| c == '"' || c == '\'' || c.is_whitespace());

        let basename = token
            .rsplit(['\\', '/'])
            .next()
            .unwrap_or(token)
            .trim_matches(|c: char| {
                !(c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-')
            })
            .to_string();

        if !basename.is_empty() && basename.ends_with(".dll") {
            if !out.iter().any(|v| v == &basename) {
                out.push(basename);
            }
        }

        offset = dll_end;
    }

    out
}

#[cfg(windows)]
fn normalize_dll_basename(value: &str) -> Option<String> {
    let trimmed = value.trim_matches(|c: char| c == '"' || c == '\'' || c.is_whitespace());
    let basename = trimmed.rsplit(['\\', '/']).next().unwrap_or(trimmed);
    let cleaned = basename
        .trim_matches(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-'));
    if cleaned.is_empty() {
        return None;
    }

    let lower = cleaned.to_ascii_lowercase();
    if lower.starts_with("lwtest_") && lower.ends_with(".dll") {
        Some(lower)
    } else {
        None
    }
}

#[cfg(windows)]
fn is_loader_related_code(code: u32) -> bool {
    matches!(
        code,
        0xC0000135 | 0xC0000139 | 0xC000007B | 0xC0000142 | 0xC000001D | 0x8007007E | 0x800700C1
    )
}

#[cfg(windows)]
fn env_path_override(_overrides: &[String]) -> Option<OsString> {
    None
}

#[cfg(windows)]
fn display_path(path: &Path) -> String {
    let raw = path.display().to_string();
    if let Some(rest) = raw.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = raw.strip_prefix(r"\\?\") {
        rest.to_string()
    } else {
        raw
    }
}

#[cfg(all(test, windows))]
mod dynamic_missing_tests {
    use super::*;
    use debug_run::DebugStringEvent;

    fn outcome_with_debug_lines(lines: &[&str]) -> RunOutcome {
        let events = lines
            .iter()
            .map(|line| {
                RuntimeEvent::DebugString(DebugStringEvent {
                    pid: 1,
                    tid: 1,
                    text: (*line).to_string(),
                })
            })
            .collect();
        RunOutcome {
            pid: 1,
            runtime_events: events,
            loaded_modules: Vec::new(),
            loader_snaps_peb: None,
            end_kind: RunEndKind::ExitProcess,
            exit_code: Some(0),
            exception_code: None,
            elapsed_ms: 1,
        }
    }

    #[test]
    fn detects_dynamic_missing_on_single_failure_line() {
        let outcome = outcome_with_debug_lines(&[
            r#"LdrLoadDll failed for C:\App\foo.dll Status: 0xC0000135"#,
        ]);
        let detected =
            detect_dynamic_missing_from_debug_strings(&outcome).expect("expected dynamic missing");
        assert_eq!(detected.dll, "foo.dll");
        assert_eq!(detected.reason, "NOT_FOUND");
        assert_eq!(detected.status, Some(0xC0000135));
    }

    #[test]
    fn uses_last_load_attempt_when_failure_line_has_no_dll() {
        let outcome = outcome_with_debug_lines(&[
            r#"LdrLoadDll - ENTER: DLL name: C:\App\bar.dll"#,
            r#"LdrpInitializeProcess - ERROR: Walking the import tables of the executable and its static imports failed with status 0xc0000135"#,
        ]);
        let detected =
            detect_dynamic_missing_from_debug_strings(&outcome).expect("expected dynamic missing");
        assert_eq!(detected.dll, "bar.dll");
        assert_eq!(detected.reason, "NOT_FOUND");
    }

    #[test]
    fn prefers_non_noise_dll() {
        let outcome = outcome_with_debug_lines(&[
            r#"LdrLoadDll failed for api-ms-win-core-file-l1-2-0.dll while loading mydep.dll Status: 0xC0000135"#,
        ]);
        let detected =
            detect_dynamic_missing_from_debug_strings(&outcome).expect("expected dynamic missing");
        assert_eq!(detected.dll, "mydep.dll");
    }

    #[test]
    fn transitive_terminal_failure_prefers_unable_to_load_dll_line() {
        let outcome = outcome_with_debug_lines(&[
            r#"LdrpFindKnownDll - RETURN: Status: 0xc0000135"#,
            r#"LdrpProcessWork - ERROR: Unable to load DLL: "lwtest_b.dll", Parent Module: "C:\App\lwtest_a.dll", Status: 0xc0000135"#,
        ]);
        let detected =
            detect_dynamic_missing_from_debug_strings(&outcome).expect("expected dynamic missing");
        assert_eq!(detected.dll, "lwtest_b.dll");
        assert_eq!(detected.reason, "NOT_FOUND");
        assert_eq!(detected.status, Some(0xC0000135));
    }

    #[test]
    fn probe_lines_alone_do_not_trigger_dynamic_missing() {
        let outcome = outcome_with_debug_lines(&[
            r#"LdrpFindKnownDll - RETURN: Status: 0xc0000135"#,
            r#"LdrpFindLoadedDllInternal - RETURN: Status: 0xc0000135"#,
            r#"LdrpResolveDllName - RETURN: Status: 0xc0000135"#,
        ]);
        assert!(detect_dynamic_missing_from_debug_strings(&outcome).is_none());
    }

    #[test]
    fn lwtest_wrapper_filters_non_fixture_dlls() {
        let outcome = outcome_with_debug_lines(&[
            r#"LdrLoadDll failed for C:\App\foo.dll Status: 0xC0000135"#,
        ]);
        assert_eq!(detect_missing_lwtest_dll_from_debug_strings(&outcome), None);

        let outcome = outcome_with_debug_lines(&[
            r#"LdrLoadDll failed for C:\App\lwtest_b.dll Status: 0xC0000135"#,
        ]);
        assert_eq!(
            detect_missing_lwtest_dll_from_debug_strings(&outcome),
            Some("lwtest_b.dll".to_string())
        );
    }
}
