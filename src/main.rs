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
use std::collections::{HashMap, HashSet, VecDeque};
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
use emit::{
    emit, field, hex_u32, hex_usize, quote, summary_fields, SummaryCounts, TOKEN_DEBUG_STRING,
    TOKEN_DYNAMIC_MISSING, TOKEN_FIRST_BREAK, TOKEN_NOTE, TOKEN_RUNTIME_LOADED, TOKEN_RUN_END,
    TOKEN_RUN_START, TOKEN_SEARCH_ORDER, TOKEN_SEARCH_PATH, TOKEN_STATIC_BAD_IMAGE,
    TOKEN_STATIC_END, TOKEN_STATIC_FOUND, TOKEN_STATIC_IMPORT, TOKEN_STATIC_MISSING,
    TOKEN_STATIC_START, TOKEN_SUCCESS, TOKEN_SUMMARY,
};
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
    let trace_mode = opts.trace;
    let summary_mode = !trace_mode;

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
                        if trace_mode {
                            emit(
                                TOKEN_NOTE,
                                &[
                                    field("topic", quote("loader-snaps")),
                                    field("detail", quote("enable-failed")),
                                    field("code", hex_u32(code)),
                                ],
                            );
                        }
                        return if test_mode { 10 } else { 21 };
                    }
                };

                if trace_mode && opts.verbose {
                    emit_loader_snaps_peb_note(peb_info);
                    emit(
                        TOKEN_NOTE,
                        &[
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
                if trace_mode {
                    emit(
                        TOKEN_NOTE,
                        &[
                            field("topic", quote("loader-snaps")),
                            field("detail", quote("wow64-target-unsupported")),
                            field(
                                "message",
                                quote("WOW64 target support is roadmap-only in v1"),
                            ),
                        ],
                    );
                }
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
            if trace_mode {
                emit(
                    TOKEN_NOTE,
                    &[
                        field("topic", quote("loader-snaps")),
                        field("detail", quote("restore-failed")),
                        field("code", hex_u32(code)),
                    ],
                );
            }
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
            if trace_mode {
                emit(
                    TOKEN_NOTE,
                    &[
                        field("topic", quote("loader-snaps")),
                        field("detail", quote("wow64-target-unsupported")),
                        field(
                            "message",
                            quote("WOW64 target support is roadmap-only in v1"),
                        ),
                    ],
                );
            }
            return 22;
        }
    };

    if trace_mode && opts.verbose && opts.loader_snaps {
        if let Some(info) = outcome.loader_snaps_peb {
            emit_loader_snaps_peb_note(info);
        }
    }

    if trace_mode && opts.verbose {
        emit_run_events(&exe_path, &cwd, &outcome);
    }

    let mut runtime_loaded: HashSet<String> = HashSet::new();
    let mut runtime_observed: HashMap<String, PathBuf> = HashMap::new();
    for module in &outcome.loaded_modules {
        let dll = module.dll_name.to_ascii_lowercase();
        runtime_loaded.insert(dll.clone());

        let Some(path) = module.path.as_ref() else {
            continue;
        };

        match runtime_observed.get_mut(&dll) {
            Some(existing) => {
                if prefer_runtime_observed_path(path, existing.as_path()) {
                    *existing = path.clone();
                }
            }
            None => {
                runtime_observed.insert(dll, path.clone());
            }
        }
    }

    let mut first_break = false;
    let mut static_missing_count = 0usize;
    let mut static_bad_image_count = 0usize;
    let mut dynamic_missing_count = 0usize;
    let mut detected_missing_name: Option<String> = None;
    let mut dynamic_failure_seen = false;
    let mut summary_line_emitted = false;
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
        let mode = if summary_mode {
            StaticEmitMode::SummaryOnly
        } else if opts.verbose {
            StaticEmitMode::Full
        } else {
            StaticEmitMode::FailuresOnly
        };
        let diag = diagnose_static_imports(
            &exe_path,
            &cwd,
            &runtime_loaded,
            &runtime_observed,
            env_path_override(&[]),
            mode,
        );
        match diag {
            Ok(report) => {
                static_missing_count = report.missing_count;
                static_bad_image_count = report.bad_image_count;
                if let Some(issue) = &report.first_issue {
                    first_break = true;
                    if detected_missing_name.is_none() {
                        detected_missing_name = normalize_dll_basename(&issue.dll)
                            .or_else(|| Some(issue.dll.to_ascii_lowercase()));
                    }
                    if summary_mode {
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
                                emit(TOKEN_STATIC_MISSING, &fields);
                                summary_line_emitted = true;
                            }
                            ResolutionKind::BadImage => {
                                emit(
                                    TOKEN_STATIC_BAD_IMAGE,
                                    &[
                                        field("module", quote(&issue.module)),
                                        field("dll", quote(&issue.dll)),
                                        field("reason", quote("BAD_IMAGE")),
                                    ],
                                );
                                summary_line_emitted = true;
                            }
                            ResolutionKind::Found => {}
                        }
                    } else if opts.verbose {
                        emit(
                            TOKEN_FIRST_BREAK,
                            &[
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
                            TOKEN_SEARCH_ORDER,
                            &[field("safedll", if report.safedll { "1" } else { "0" })],
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
                                emit(TOKEN_STATIC_MISSING, &fields);
                            }
                            ResolutionKind::BadImage => {
                                emit(
                                    TOKEN_STATIC_BAD_IMAGE,
                                    &[
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
                                TOKEN_SEARCH_PATH,
                                &[
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
                if trace_mode && opts.verbose {
                    emit(
                        TOKEN_NOTE,
                        &[field(
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
    if opts.loader_snaps && static_missing_count + static_bad_image_count == 0 {
        let exe_dir = exe_path.parent().unwrap_or_else(|| Path::new("."));
        if let Some(dm) = detect_dynamic_missing_from_debug_strings(&outcome, exe_dir, &cwd) {
            dynamic_failure_seen = true;
            first_break = true;
            if test_mode {
                if dm.dll.starts_with("lwtest_") {
                    detected_missing_name = Some(dm.dll.clone());
                }
            } else if summary_mode {
                let mut fields = vec![
                    field("dll", quote(&dm.dll)),
                    field("reason", quote(dm.reason)),
                ];
                if let Some(st) = dm.status {
                    fields.push(field("status", hex_u32(st)));
                }
                emit(TOKEN_DYNAMIC_MISSING, &fields);
                dynamic_missing_count = 1;
                summary_line_emitted = true;
            } else {
                let app_dir = exe_path.parent().unwrap_or_else(|| Path::new("."));
                if let Ok(context) = dynamic_trace_search_context(app_dir, &cwd) {
                    emit(
                        TOKEN_SEARCH_ORDER,
                        &[field("safedll", if context.safedll { "1" } else { "0" })],
                    );

                    let mut fields = vec![
                        field("dll", quote(&dm.dll)),
                        field("reason", quote(dm.reason)),
                    ];
                    if let Some(st) = dm.status {
                        fields.push(field("status", hex_u32(st)));
                    }
                    emit(TOKEN_DYNAMIC_MISSING, &fields);

                    let resolution = search::resolve_dll(&dm.dll, &context);
                    for candidate in &resolution.candidates {
                        emit(
                            TOKEN_SEARCH_PATH,
                            &[
                                field("dll", quote(&dm.dll)),
                                field("order", candidate.order.to_string()),
                                field("path", quote(&display_path(&candidate.path))),
                                field("result", quote(candidate.result)),
                            ],
                        );
                    }
                    dynamic_missing_count = 1;
                } else {
                    let mut fields = vec![
                        field("dll", quote(&dm.dll)),
                        field("reason", quote(dm.reason)),
                    ];
                    if let Some(st) = dm.status {
                        fields.push(field("status", hex_u32(st)));
                    }
                    emit(TOKEN_DYNAMIC_MISSING, &fields);
                    dynamic_missing_count = 1;
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
            || static_missing_count + static_bad_image_count + dynamic_missing_count > 0
            || dynamic_failure_seen
            || loader_exception.is_some();
        return test_mode_exit_code(&outcome, load_failure_detected);
    }

    if trace_mode && opts.verbose {
        emit(
            TOKEN_SUMMARY,
            &summary_fields(
                first_break,
                SummaryCounts {
                    static_missing: static_missing_count,
                    static_bad_image: static_bad_image_count,
                    dynamic_missing: dynamic_missing_count,
                    runtime_loaded: outcome.loaded_modules.len(),
                    com_issues: 0,
                },
            ),
        );
    }

    let code = run_result_code(
        &outcome,
        static_missing_count + static_bad_image_count + dynamic_missing_count,
    );

    if summary_mode && !summary_line_emitted && code == 0 {
        emit(TOKEN_SUCCESS, &[field("status", "0")]);
    }

    code
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

    let runtime_loaded: HashSet<String> = HashSet::new();
    let runtime_observed: HashMap<String, PathBuf> = HashMap::new();
    let diag = diagnose_static_imports(
        &module_path,
        &cwd,
        &runtime_loaded,
        &runtime_observed,
        env_path_override(&[]),
        StaticEmitMode::Full,
    );
    match diag {
        Ok(report) => {
            emit(
                TOKEN_SUMMARY,
                &summary_fields(
                    false,
                    SummaryCounts {
                        static_missing: report.missing_count,
                        static_bad_image: report.bad_image_count,
                        dynamic_missing: 0,
                        runtime_loaded: 0,
                        com_issues: 0,
                    },
                ),
            );
            if report.missing_count + report.bad_image_count > 0 {
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
        TOKEN_RUN_START,
        &[
            field("exe", quote(&display_path(exe_path))),
            field("cwd", quote(&display_path(cwd))),
            field("pid", outcome.pid.to_string()),
        ],
    );

    for event in &outcome.runtime_events {
        match event {
            RuntimeEvent::RuntimeLoaded(module) => {
                emit(
                    TOKEN_RUNTIME_LOADED,
                    &[
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
                    TOKEN_DEBUG_STRING,
                    &[
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
        TOKEN_RUN_END,
        &[
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
        TOKEN_NOTE,
        &[
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
    missing_count: usize,
    bad_image_count: usize,
    first_issue: Option<FirstIssue>,
    safedll: bool,
}

#[cfg(windows)]
#[derive(Clone, Copy)]
enum StaticEmitMode {
    Full,
    FailuresOnly,
    SummaryOnly,
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
    runtime_observed: &HashMap<String, PathBuf>,
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
            TOKEN_STATIC_START,
            &[
                field("module", quote(&display_path(module_path))),
                field("scope", quote("direct-and-recursive-imports")),
            ],
        );
        emit(
            TOKEN_SEARCH_ORDER,
            &[field("safedll", if context.safedll { "1" } else { "0" })],
        );
    }

    let mut missing_count = 0usize;
    let mut bad_image_count = 0usize;
    let mut first_issue = None::<FirstIssue>;
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    let mut max_parent_depth_for_failures = None::<u32>;

    visited.insert(normalize_module_visit_key(module_path));
    queue.push_back(WalkNode {
        module_path: module_path.to_path_buf(),
        module_name: root_module_name.clone(),
        depth: 0,
    });

    while let Some(node) = queue.pop_front() {
        if matches!(
            emit_mode,
            StaticEmitMode::FailuresOnly | StaticEmitMode::SummaryOnly
        ) {
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
                    TOKEN_STATIC_IMPORT,
                    &[
                        field("module", quote(&node.module_name)),
                        field("needs", quote(&dll)),
                    ],
                );
            }

            if node.depth == 0 && runtime_loaded.contains(&dll) {
                if matches!(emit_mode, StaticEmitMode::Full) {
                    emit(
                        TOKEN_STATIC_FOUND,
                        &[
                            field("module", quote(&node.module_name)),
                            field("dll", quote(&dll)),
                            field("reason", quote("RUNTIME_OBSERVED")),
                        ],
                    );
                }

                let observed_path = runtime_observed
                    .get(&dll)
                    .cloned()
                    .or_else(|| search::resolve_dll(&dll, &context).chosen);
                if let Some(path) = observed_path {
                    queue_module_if_unvisited(&mut visited, &mut queue, &path, node.depth + 1);
                }
                continue;
            }

            let resolution = search::resolve_dll(&dll, &context);
            if matches!(emit_mode, StaticEmitMode::Full) {
                for candidate in &resolution.candidates {
                    emit(
                        TOKEN_SEARCH_PATH,
                        &[
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
                            TOKEN_STATIC_FOUND,
                            &[
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
                        queue_module_if_unvisited(&mut visited, &mut queue, chosen, node.depth + 1);
                    }
                }
                ResolutionKind::Missing => {
                    missing_count += 1;
                    let issue = FirstIssue {
                        module: root_module_name.clone(),
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
                        emit(TOKEN_STATIC_MISSING, &fields);
                    }

                    if matches!(
                        emit_mode,
                        StaticEmitMode::FailuresOnly | StaticEmitMode::SummaryOnly
                    ) {
                        max_parent_depth_for_failures.get_or_insert(node.depth);
                    }
                }
                ResolutionKind::BadImage => {
                    bad_image_count += 1;
                    let issue = FirstIssue {
                        module: root_module_name.clone(),
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
                            TOKEN_STATIC_BAD_IMAGE,
                            &[
                                field("module", quote(&node.module_name)),
                                field("dll", quote(&dll)),
                                field("reason", quote("BAD_IMAGE")),
                            ],
                        );
                    }

                    if matches!(
                        emit_mode,
                        StaticEmitMode::FailuresOnly | StaticEmitMode::SummaryOnly
                    ) {
                        max_parent_depth_for_failures.get_or_insert(node.depth);
                    }
                }
            }
        }
    }

    if matches!(emit_mode, StaticEmitMode::Full) {
        emit(
            TOKEN_STATIC_END,
            &[field("module", quote(&display_path(module_path)))],
        );
    }

    Ok(StaticReport {
        missing_count,
        bad_image_count,
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
fn queue_module_if_unvisited(
    visited: &mut HashSet<String>,
    queue: &mut VecDeque<WalkNode>,
    module_path: &Path,
    depth: u32,
) -> bool {
    let key = normalize_module_visit_key(module_path);
    if !visited.insert(key) {
        return false;
    }

    queue.push_back(WalkNode {
        module_path: module_path.to_path_buf(),
        module_name: module_name_lower(module_path),
        depth,
    });
    true
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
fn prefer_runtime_observed_path(candidate: &Path, existing: &Path) -> bool {
    let candidate_key = normalize_module_visit_key(candidate);
    let existing_key = normalize_module_visit_key(existing);
    (candidate_key.len(), candidate_key.as_str()) < (existing_key.len(), existing_key.as_str())
}

#[cfg(windows)]
fn run_result_code(outcome: &RunOutcome, diagnosis_count: usize) -> i32 {
    if diagnosis_count > 0 {
        return 10;
    }

    match outcome.end_kind {
        RunEndKind::ExitProcess if outcome.exit_code == Some(0) => 0,
        RunEndKind::Timeout if !outcome.loaded_modules.is_empty() => 0,
        RunEndKind::ExitProcess | RunEndKind::Exception | RunEndKind::Timeout => 21,
    }
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
            &[field("name", dll_name), field("path", display_path(path))],
        );
    }

    if let Some(name) = missing_name {
        emit(
            "LWTEST:RESULT",
            &[
                field("kind", "missing_dll"),
                field("name", name.to_ascii_lowercase()),
            ],
        );
    }

    if let Some(code) = exit_code {
        emit("LWTEST:TARGET", &[field("exit_code", code.to_string())]);
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
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    detect_dynamic_missing_from_debug_strings(outcome, Path::new("."), &cwd).and_then(|v| {
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
#[derive(Debug, Clone)]
struct DynamicLoadContext {
    dll: String,
    path: Option<PathBuf>,
}

#[cfg(windows)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum DynamicCandidateKind {
    Other,
    SearchPathFailure,
    InitializeProcessFailure,
    LoadDllFailed,
    UnableToLoadDll,
}

#[cfg(windows)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DynamicFailureKind {
    NameBased,
    FullPathProbe,
}

#[cfg(windows)]
#[derive(Debug, Clone)]
struct DynamicCandidate {
    event_idx: usize,
    tid: u32,
    dll: String,
    status: Option<u32>,
    reason: &'static str,
    score: i32,
    kind: DynamicCandidateKind,
    app_local_hint: bool,
    framework_or_os_hint: bool,
    failure_kind: DynamicFailureKind,
    resolved: bool,
    thread_correlated: bool,
}

#[cfg(windows)]
fn detect_dynamic_missing_from_debug_strings(
    outcome: &RunOutcome,
    exe_dir: &Path,
    cwd: &Path,
) -> Option<DynamicMissing> {
    let mut last_load_by_tid: HashMap<u32, DynamicLoadContext> = HashMap::new();
    let mut candidates: Vec<DynamicCandidate> = Vec::new();
    let mut last_failure_candidate_by_tid: HashMap<u32, usize> = HashMap::new();
    let mut latest_success_idx_by_basename: HashMap<String, usize> = HashMap::new();

    for (idx, event) in outcome.runtime_events.iter().enumerate() {
        match event {
            RuntimeEvent::RuntimeLoaded(module) => {
                let basename = module_name_lower(Path::new(&module.dll_name));
                record_dynamic_success(&mut latest_success_idx_by_basename, &basename, idx);
            }
            RuntimeEvent::DebugString(debug) => {
                let raw = debug.text.trim();
                if raw.is_empty() {
                    continue;
                }

                let lower = raw.to_ascii_lowercase();
                let dlls = extract_dll_basenames(&lower);
                let explicit_path = extract_candidate_path(raw);

                if !dlls.is_empty() && looks_like_load_attempt(&lower) {
                    if let Some(candidate) = pick_best_dll(&dlls).or_else(|| dlls.first().cloned())
                    {
                        last_load_by_tid.insert(
                            debug.tid,
                            DynamicLoadContext {
                                dll: candidate,
                                path: explicit_path.clone(),
                            },
                        );
                    }
                }

                if let Some(status) = extract_load_return_status(&lower) {
                    if status == 0 {
                        let success_dll = pick_best_dll(&dlls)
                            .or_else(|| dlls.first().cloned())
                            .or_else(|| {
                                last_load_by_tid.get(&debug.tid).map(|ctx| ctx.dll.clone())
                            });
                        if let Some(dll) = success_dll {
                            record_dynamic_success(&mut latest_success_idx_by_basename, &dll, idx);
                        }
                    } else if is_loader_related_code(status) {
                        if let Some(candidate_idx) =
                            last_failure_candidate_by_tid.get(&debug.tid).copied()
                        {
                            if let Some(candidate) = candidates.get_mut(candidate_idx) {
                                if candidate.status.is_none() {
                                    candidate.status = Some(status);
                                    candidate.reason =
                                        classify_dynamic_reason(&lower, Some(status));
                                }
                            }
                        }
                    }

                    last_failure_candidate_by_tid.remove(&debug.tid);
                    last_load_by_tid.remove(&debug.tid);
                }

                if is_ignored_probe_line(&lower) {
                    continue;
                }

                let Some(kind) = classify_dynamic_candidate_kind(&lower) else {
                    continue;
                };
                let score = failure_score(&lower);
                if score <= 0 {
                    continue;
                }

                let status = extract_first_hex_u32(&lower);
                let from_line = pick_best_dll(&dlls).or_else(|| dlls.first().cloned());
                let from_thread = last_load_by_tid.get(&debug.tid).cloned();
                let explicit = extract_unable_to_load_dll(&lower);
                let candidate = explicit
                    .clone()
                    .or_else(|| from_line.clone())
                    .or_else(|| from_thread.as_ref().map(|ctx| ctx.dll.clone()));
                let Some(dll) = candidate else {
                    continue;
                };

                let reason = classify_dynamic_reason(&lower, status);

                let dll = if is_noise_dll(&dll) {
                    pick_best_dll(&dlls)
                        .or_else(|| from_thread.as_ref().map(|ctx| ctx.dll.clone()))
                        .unwrap_or(dll)
                } else {
                    dll
                };

                let candidate_path = explicit_path
                    .clone()
                    .or_else(|| from_thread.as_ref().and_then(|ctx| ctx.path.clone()));
                let app_local_hint = candidate_path
                    .as_ref()
                    .map(|path| is_app_local_path(path, exe_dir, cwd))
                    .unwrap_or(false);
                let framework_or_os_hint = candidate_path
                    .as_ref()
                    .map(|path| is_windows_or_gac_path(path))
                    .unwrap_or(false)
                    || is_noise_dll(&dll);
                let thread_correlated =
                    explicit.is_none() && from_line.is_none() && from_thread.is_some();

                let detected = DynamicCandidate {
                    event_idx: idx,
                    tid: debug.tid,
                    dll,
                    reason,
                    status,
                    score,
                    kind,
                    app_local_hint,
                    framework_or_os_hint,
                    failure_kind: classify_failure_kind(kind, candidate_path.as_deref()),
                    resolved: false,
                    thread_correlated,
                };
                candidates.push(detected);
                if !thread_correlated {
                    last_failure_candidate_by_tid.insert(debug.tid, candidates.len() - 1);
                }
            }
        }
    }

    for candidate in &mut candidates {
        let success_idx = latest_success_idx_by_basename.get(&candidate.dll).copied();
        candidate.resolved = success_idx
            .map(|idx| idx > candidate.event_idx)
            .unwrap_or(false)
            || matches!(candidate.failure_kind, DynamicFailureKind::FullPathProbe)
                && success_idx.is_some();
    }

    candidates.retain(|candidate| !candidate.resolved);
    if candidates.is_empty() {
        return None;
    }

    // This ordering encodes the Phase C selection rules from the v1 spec:
    // terminal unresolved failures first, then app-local relevance, then
    // deterministic earliest/tie-break ordering.
    candidates.sort_by(|a, b| {
        b.kind
            .cmp(&a.kind)
            .then_with(|| b.score.cmp(&a.score))
            .then_with(|| b.app_local_hint.cmp(&a.app_local_hint))
            .then_with(|| a.framework_or_os_hint.cmp(&b.framework_or_os_hint))
            .then_with(|| b.thread_correlated.cmp(&a.thread_correlated))
            .then_with(|| a.event_idx.cmp(&b.event_idx))
            .then_with(|| a.dll.cmp(&b.dll))
            .then_with(|| a.tid.cmp(&b.tid))
    });

    let best = candidates.into_iter().next()?;
    Some(DynamicMissing {
        dll: best.dll,
        reason: best.reason,
        status: best.status,
    })
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
        || text_lower.contains("ldrpreporterror - warning: locating export")
}

#[cfg(windows)]
fn has_loader_failure_code(text_lower: &str) -> bool {
    text_lower.contains("0xc0000135")
        || text_lower.contains("0x8007007e")
        || text_lower.contains("0xc000007b")
        || text_lower.contains("0xc000012f")
        || text_lower.contains("0x800700c1")
        || text_lower.contains("0xc0000139")
        || text_lower.contains("0xc0000142")
}

#[cfg(windows)]
fn classify_dynamic_reason(text_lower: &str, status: Option<u32>) -> &'static str {
    match status {
        Some(0xC0000135) | Some(0x8007007E) => "NOT_FOUND",
        Some(0xC000007B) | Some(0x800700C1) | Some(0xC000012F) => "BAD_IMAGE",
        _ => {
            if text_lower.contains("not found")
                || text_lower.contains("could not be found")
                || text_lower.contains("file not found")
            {
                "NOT_FOUND"
            } else if text_lower.contains("bad image")
                || text_lower.contains("invalid image")
                || text_lower.contains("not a valid win32 application")
            {
                "BAD_IMAGE"
            } else {
                "OTHER"
            }
        }
    }
}

#[cfg(windows)]
fn extract_load_return_status(text_lower: &str) -> Option<u32> {
    let is_return = text_lower.contains("ldrloaddll - return")
        || text_lower.contains("ldrploaddllinternal - return");
    if !is_return {
        return None;
    }
    extract_first_hex_u32(text_lower)
}

#[cfg(windows)]
fn classify_failure_kind(kind: DynamicCandidateKind, path: Option<&Path>) -> DynamicFailureKind {
    match path {
        Some(value)
            if value.is_absolute()
                && matches!(
                    kind,
                    DynamicCandidateKind::SearchPathFailure
                        | DynamicCandidateKind::LoadDllFailed
                        | DynamicCandidateKind::UnableToLoadDll
                ) =>
        {
            DynamicFailureKind::FullPathProbe
        }
        _ => DynamicFailureKind::NameBased,
    }
}

#[cfg(windows)]
fn record_dynamic_success(
    latest_success_idx_by_basename: &mut HashMap<String, usize>,
    dll: &str,
    idx: usize,
) {
    latest_success_idx_by_basename
        .entry(dll.to_string())
        .and_modify(|existing| *existing = (*existing).max(idx))
        .or_insert(idx);
}

#[cfg(windows)]
fn failure_score(text_lower: &str) -> i32 {
    if text_lower.contains("ldrpprocesswork - error: unable to load dll") {
        return 100;
    }
    if text_lower.contains("- error: unable to load dll") {
        return 95;
    }
    if text_lower.contains("ldrpinitializenode - error: init routine")
        && text_lower.contains("failed during dll_process_attach")
    {
        return 92;
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
    if has_loader_failure_code(text_lower)
        && (text_lower.contains("failed") || text_lower.contains("error"))
        && looks_like_loader_failure_context(text_lower)
    {
        return 60;
    }
    0
}

#[cfg(windows)]
fn classify_dynamic_candidate_kind(text_lower: &str) -> Option<DynamicCandidateKind> {
    if text_lower.contains("ldrpprocesswork - error: unable to load dll")
        || text_lower.contains("- error: unable to load dll")
    {
        return Some(DynamicCandidateKind::UnableToLoadDll);
    }
    if text_lower.contains("ldrpinitializenode - error: init routine")
        && text_lower.contains("failed during dll_process_attach")
    {
        return Some(DynamicCandidateKind::InitializeProcessFailure);
    }
    if text_lower.contains("ldrloaddll") && text_lower.contains("failed") {
        return Some(DynamicCandidateKind::LoadDllFailed);
    }
    if text_lower.contains("process initialization failed")
        || text_lower.contains("_ldrpinitialize - error")
        || text_lower.contains("walking the import tables")
    {
        return Some(DynamicCandidateKind::InitializeProcessFailure);
    }
    if text_lower.contains("ldrpsearchpath - return") && has_loader_failure_code(text_lower) {
        return Some(DynamicCandidateKind::SearchPathFailure);
    }
    if has_loader_failure_code(text_lower)
        && (text_lower.contains("failed") || text_lower.contains("error"))
        && looks_like_loader_failure_context(text_lower)
    {
        return Some(DynamicCandidateKind::Other);
    }
    None
}

#[cfg(windows)]
fn looks_like_loader_failure_context(text_lower: &str) -> bool {
    text_lower.contains("ldrloaddll")
        || text_lower.contains("ldrploadllinternal")
        || text_lower.contains("loadlibrary")
        || text_lower.contains("unable to load dll")
        || text_lower.contains("dll name:")
        || text_lower.contains("ldrpsearchpath")
        || text_lower.contains("process initialization failed")
        || text_lower.contains("_ldrpinitialize")
        || text_lower.contains("walking the import tables")
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
fn extract_candidate_path(text: &str) -> Option<PathBuf> {
    let lower = text.to_ascii_lowercase();
    let marker = ".dll";
    let mut offset = 0usize;
    while let Some(rel) = lower[offset..].find(marker) {
        let dll_end = offset + rel + marker.len();
        let bytes = text.as_bytes();
        let mut start = offset + rel;
        while start > 0 {
            let c = bytes[start - 1] as char;
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
        let token =
            text[start..dll_end].trim_matches(|c: char| c == '"' || c == '\'' || c.is_whitespace());
        if token.contains('\\') || token.contains('/') || token.contains(':') {
            return Some(PathBuf::from(token));
        }
        offset = dll_end;
    }
    None
}

#[cfg(windows)]
fn is_windows_or_gac_path(path: &Path) -> bool {
    let normalized = display_path(path).replace('/', "\\").to_ascii_lowercase();
    path_is_under_dir(&normalized, &windows_dir_candidates())
        || normalized.contains(r"\windows\microsoft.net\")
        || normalized.contains(r"\assembly\gac")
}

#[cfg(windows)]
fn is_app_local_path(path: &Path, exe_dir: &Path, cwd: &Path) -> bool {
    let candidate = display_path(path).replace('/', "\\").to_ascii_lowercase();
    let exe = display_path(exe_dir)
        .replace('/', "\\")
        .to_ascii_lowercase();
    let cwd_norm = display_path(cwd).replace('/', "\\").to_ascii_lowercase();
    path_is_under_dir(&candidate, std::slice::from_ref(&exe))
        || path_is_under_dir(&candidate, std::slice::from_ref(&cwd_norm))
        || !path_is_under_dir(&candidate, &windows_dir_candidates())
}

#[cfg(windows)]
fn path_is_under_dir(candidate: &str, dirs: &[String]) -> bool {
    dirs.iter().any(|dir| {
        let normalized_dir = dir.trim_end_matches('\\');
        candidate == normalized_dir
            || candidate
                .strip_prefix(normalized_dir)
                .map(|rest| rest.starts_with('\\'))
                .unwrap_or(false)
    })
}

#[cfg(windows)]
fn windows_dir_candidates() -> Vec<String> {
    let mut dirs = Vec::new();
    if let Some(windir) = std::env::var_os("WINDIR") {
        dirs.push(
            display_path(Path::new(&windir))
                .replace('/', "\\")
                .to_ascii_lowercase(),
        );
    }
    let default = r"c:\windows".to_string();
    if !dirs.iter().any(|dir| dir == &default) {
        dirs.push(default);
    }
    dirs
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

        if !basename.is_empty() && basename.ends_with(".dll") && !out.iter().any(|v| v == &basename)
        {
            out.push(basename);
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
fn dynamic_trace_search_context(app_dir: &Path, cwd: &Path) -> Result<SearchContext, String> {
    // Keep this hook scoped to dynamic trace emission so static diagnosis stays
    // on the normal production path.
    if env::var("LOADWHAT_TEST_FORCE_DYNAMIC_SEARCH_CONTEXT_FAIL")
        .map(|value| value.trim() == "1")
        .unwrap_or(false)
    {
        return Err("forced dynamic search context failure".to_string());
    }

    SearchContext::from_environment(app_dir, cwd, env_path_override(&[]))
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
mod static_diagnosis_tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEST_ID: AtomicU64 = AtomicU64::new(1);

    fn missing_issue(via: &str, depth: u32, dll: &str) -> FirstIssue {
        FirstIssue {
            module: "host.exe".to_string(),
            via: via.to_string(),
            depth,
            dll: dll.to_string(),
            diagnosis: "MISSING_STATIC_IMPORT",
            kind: ResolutionKind::Missing,
            candidates: Vec::new(),
        }
    }

    fn unique_temp_dir(name: &str) -> PathBuf {
        let id = NEXT_TEST_ID.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("loadwhat-{name}-{}-{id}", std::process::id()));
        fs::create_dir_all(&dir).expect("failed to create temp dir");
        dir
    }

    #[test]
    fn lowest_depth_wins() {
        let mut current = None;
        consider_first_issue(&mut current, missing_issue("b.dll", 3, "missing_x.dll"));
        consider_first_issue(&mut current, missing_issue("a.dll", 2, "missing_y.dll"));

        let selected = current.expect("expected selected issue");
        assert_eq!(selected.depth, 2);
        assert_eq!(selected.via, "a.dll");
        assert_eq!(selected.dll, "missing_y.dll");
    }

    #[test]
    fn same_depth_tie_breaks_by_via_then_dll() {
        let mut current = None;
        consider_first_issue(&mut current, missing_issue("z.dll", 2, "missing_z.dll"));
        consider_first_issue(&mut current, missing_issue("a.dll", 2, "missing_y.dll"));
        consider_first_issue(&mut current, missing_issue("a.dll", 2, "missing_x.dll"));

        let selected = current.expect("expected selected issue");
        assert_eq!(selected.depth, 2);
        assert_eq!(selected.via, "a.dll");
        assert_eq!(selected.dll, "missing_x.dll");
    }

    #[test]
    fn visited_set_dedups_normalized_absolute_paths() {
        let temp_dir = unique_temp_dir("visited-set");
        let module_path = temp_dir.join("lwtest_shared.dll");
        fs::write(&module_path, b"fixture").expect("failed to create temp module");

        let duplicate_path = temp_dir.join(".").join("LWTEST_SHARED.DLL");
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();

        assert!(queue_module_if_unvisited(
            &mut visited,
            &mut queue,
            &module_path,
            1
        ));
        assert!(!queue_module_if_unvisited(
            &mut visited,
            &mut queue,
            &duplicate_path,
            1
        ));
        assert_eq!(queue.len(), 1);

        let queued = queue.pop_front().expect("expected queued module");
        assert_eq!(queued.depth, 1);
        assert_eq!(queued.module_name, "lwtest_shared.dll");

        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn ignores_api_set_import_names() {
        assert!(is_api_set_dll("api-ms-win-core-file-l1-2-0.dll"));
        assert!(is_api_set_dll("ext-ms-win-ntuser-window-l1-1-0.dll"));
    }

    #[test]
    fn does_not_ignore_normal_import_names() {
        assert!(!is_api_set_dll("kernel32.dll"));
        assert!(!is_api_set_dll("lwtest_a.dll"));
    }
}

#[cfg(all(test, windows))]
mod run_result_tests {
    use super::*;

    fn outcome(end_kind: RunEndKind, loaded_modules: usize, exit_code: Option<u32>) -> RunOutcome {
        RunOutcome {
            pid: 1,
            runtime_events: Vec::new(),
            loaded_modules: (0..loaded_modules)
                .map(|idx| LoadedModule {
                    dll_name: format!("mod{idx}.dll"),
                    path: None,
                    base: idx,
                })
                .collect(),
            loader_snaps_peb: None,
            end_kind,
            exit_code,
            exception_code: None,
            elapsed_ms: 1,
        }
    }

    #[test]
    fn timeout_without_runtime_progress_returns_non_diagnostic_failure() {
        assert_eq!(
            run_result_code(&outcome(RunEndKind::Timeout, 0, None), 0),
            21
        );
    }

    #[test]
    fn timeout_after_runtime_progress_keeps_current_success_like_exit_code() {
        assert_eq!(
            run_result_code(&outcome(RunEndKind::Timeout, 1, None), 0),
            0
        );
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

    fn debug_line(tid: u32, text: &str) -> RuntimeEvent {
        RuntimeEvent::DebugString(DebugStringEvent {
            pid: 1,
            tid,
            text: text.to_string(),
        })
    }

    fn runtime_loaded(dll_name: &str) -> RuntimeEvent {
        RuntimeEvent::RuntimeLoaded(LoadedModule {
            dll_name: dll_name.to_string(),
            path: None,
            base: 0,
        })
    }

    fn outcome_with_events(events: Vec<RuntimeEvent>) -> RunOutcome {
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

    fn detect_for_tests(outcome: &RunOutcome) -> Option<DynamicMissing> {
        let exe_dir = Path::new(r"C:\App");
        let cwd = Path::new(r"C:\App");
        detect_dynamic_missing_from_debug_strings(outcome, exe_dir, cwd)
    }

    #[test]
    fn detects_dynamic_missing_on_single_failure_line() {
        let outcome = outcome_with_debug_lines(&[
            r#"LdrLoadDll failed for C:\App\foo.dll Status: 0xC0000135"#,
        ]);
        let detected = detect_for_tests(&outcome).expect("expected dynamic missing");
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
        let detected = detect_for_tests(&outcome).expect("expected dynamic missing");
        assert_eq!(detected.dll, "bar.dll");
        assert_eq!(detected.reason, "NOT_FOUND");
    }

    #[test]
    fn prefers_non_noise_dll() {
        let outcome = outcome_with_debug_lines(&[
            r#"LdrLoadDll failed for api-ms-win-core-file-l1-2-0.dll while loading mydep.dll Status: 0xC0000135"#,
        ]);
        let detected = detect_for_tests(&outcome).expect("expected dynamic missing");
        assert_eq!(detected.dll, "mydep.dll");
    }

    #[test]
    fn transitive_terminal_failure_prefers_unable_to_load_dll_line() {
        let outcome = outcome_with_debug_lines(&[
            r#"LdrpFindKnownDll - RETURN: Status: 0xc0000135"#,
            r#"LdrpProcessWork - ERROR: Unable to load DLL: "lwtest_b.dll", Parent Module: "C:\App\lwtest_a.dll", Status: 0xc0000135"#,
        ]);
        let detected = detect_for_tests(&outcome).expect("expected dynamic missing");
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
        assert!(detect_for_tests(&outcome).is_none());
    }

    #[test]
    fn export_lookup_warnings_do_not_trigger_dynamic_missing() {
        let outcome = outcome_with_debug_lines(&[
            r#"LdrLoadDll - ENTER: DLL name: NTDLL.DLL"#,
            r#"LdrpReportError - WARNING: Locating export "DllGetActivationFactory" for DLL "Unknown" failed with status: 0xc0000139."#,
        ]);
        assert!(detect_for_tests(&outcome).is_none());
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

    #[test]
    fn ignores_candidate_that_later_loads_successfully() {
        let outcome = outcome_with_events(vec![
            debug_line(
                1,
                r#"LdrpProcessWork - ERROR: Unable to load DLL: "C:\App\UcpClientCppApiD.dll", Status: 0xc0000135"#,
            ),
            debug_line(
                1,
                r#"LdrpProcessWork - ERROR: Unable to load DLL: "C:\Windows\System32\UIAutomationCore.dll", Status: 0xc0000135"#,
            ),
            runtime_loaded("UIAutomationCore.dll"),
        ]);
        let detected = detect_for_tests(&outcome).expect("expected dynamic missing");
        assert_eq!(detected.dll, "ucpclientcppapid.dll");
    }

    #[test]
    fn earliest_unresolved_equal_score_candidate_wins() {
        let outcome = outcome_with_events(vec![
            debug_line(
                1,
                r#"LdrpProcessWork - ERROR: Unable to load DLL: "C:\App\first.dll", Status: 0xc0000135"#,
            ),
            debug_line(
                1,
                r#"LdrpProcessWork - ERROR: Unable to load DLL: "C:\App\second.dll", Status: 0xc0000135"#,
            ),
        ]);
        let detected = detect_for_tests(&outcome).expect("expected dynamic missing");
        assert_eq!(detected.dll, "first.dll");
    }

    #[test]
    fn thread_local_fallback_does_not_cross_contaminate() {
        let outcome = outcome_with_events(vec![
            debug_line(10, r#"LdrLoadDll - ENTER: DLL name: C:\App\foo.dll"#),
            debug_line(
                20,
                r#"LdrpInitializeProcess - ERROR: Walking the import tables failed with status 0xc0000135"#,
            ),
            debug_line(
                10,
                r#"LdrpInitializeProcess - ERROR: Walking the import tables failed with status 0xc0000135"#,
            ),
        ]);
        let detected = detect_for_tests(&outcome).expect("expected dynamic missing");
        assert_eq!(detected.dll, "foo.dll");
    }

    #[test]
    fn app_local_failure_beats_later_framework_noise() {
        let outcome = outcome_with_events(vec![
            debug_line(
                1,
                r#"LdrpProcessWork - ERROR: Unable to load DLL: "C:\App\myplugin.dll", Status: 0xc0000135"#,
            ),
            debug_line(
                1,
                r#"LdrpProcessWork - ERROR: Unable to load DLL: "C:\Windows\System32\UIAutomationCore.dll", Status: 0xc0000135"#,
            ),
        ]);
        let detected = detect_for_tests(&outcome).expect("expected dynamic missing");
        assert_eq!(detected.dll, "myplugin.dll");
    }

    #[test]
    fn thread_correlated_candidate_beats_uncorrelated_when_otherwise_equal() {
        let outcome = outcome_with_events(vec![
            debug_line(10, r#"LdrLoadDll - ENTER: DLL name: C:\App\corr.dll"#),
            debug_line(20, r#"LdrLoadDll failed for uncorr.dll Status: 0xc0000135"#),
            debug_line(10, r#"LdrLoadDll failed Status: 0xc0000135"#),
        ]);
        let detected = detect_for_tests(&outcome).expect("expected dynamic missing");
        assert_eq!(detected.dll, "corr.dll");
    }

    #[test]
    fn returns_none_when_all_candidates_later_loaded() {
        let outcome = outcome_with_events(vec![
            debug_line(
                1,
                r#"LdrpProcessWork - ERROR: Unable to load DLL: "C:\App\one.dll", Status: 0xc0000135"#,
            ),
            runtime_loaded("one.dll"),
            debug_line(
                1,
                r#"LdrpProcessWork - ERROR: Unable to load DLL: "C:\App\two.dll", Status: 0xc0000135"#,
            ),
            runtime_loaded("two.dll"),
        ]);
        assert!(detect_for_tests(&outcome).is_none());
    }

    #[test]
    fn full_path_probe_is_cleared_by_earlier_success_for_same_basename() {
        let outcome = outcome_with_events(vec![
            runtime_loaded("resolved.dll"),
            debug_line(
                1,
                r#"LdrpProcessWork - ERROR: Unable to load DLL: "C:\Missing\resolved.dll", Status: 0xc0000135"#,
            ),
        ]);
        assert!(detect_for_tests(&outcome).is_none());
    }

    #[test]
    fn successful_return_clears_previous_candidate_for_same_basename() {
        let outcome = outcome_with_events(vec![
            debug_line(1, r#"LdrLoadDll - ENTER: DLL name: C:\Missing\retry.dll"#),
            debug_line(
                1,
                r#"LdrpProcessWork - ERROR: Unable to load DLL: "C:\Missing\retry.dll", Status: 0xc0000135"#,
            ),
            debug_line(1, r#"LdrLoadDll - ENTER: DLL name: C:\Good\retry.dll"#),
            debug_line(1, r#"LdrLoadDll - RETURN: Status: 0x00000000"#),
        ]);
        assert!(detect_for_tests(&outcome).is_none());
    }

    #[test]
    fn init_routine_failure_is_not_cleared_by_earlier_success() {
        let outcome = outcome_with_events(vec![
            runtime_loaded("initfail.dll"),
            debug_line(
                7,
                r#"LdrpInitializeNode - ERROR: Init routine 00007FFFECEF10F0 for DLL "C:\App\initfail.dll" failed during DLL_PROCESS_ATTACH"#,
            ),
            debug_line(7, r#"LdrpLoadDllInternal - RETURN: Status: 0xC0000142"#),
        ]);
        let detected = detect_for_tests(&outcome).expect("expected dynamic missing");
        assert_eq!(detected.dll, "initfail.dll");
        assert_eq!(detected.reason, "OTHER");
        assert_eq!(detected.status, Some(0xC0000142));
    }

    #[test]
    fn earlier_loaded_full_path_probe_does_not_beat_real_unresolved_candidate() {
        let outcome = outcome_with_events(vec![
            runtime_loaded("resolved.dll"),
            debug_line(
                1,
                r#"LdrpProcessWork - ERROR: Unable to load DLL: "C:\Missing\resolved.dll", Status: 0xc0000135"#,
            ),
            debug_line(
                1,
                r#"LdrpProcessWork - ERROR: Unable to load DLL: "C:\App\required.dll", Status: 0xc0000135"#,
            ),
        ]);
        let detected = detect_for_tests(&outcome).expect("expected dynamic missing");
        assert_eq!(detected.dll, "required.dll");
    }

    #[test]
    fn detects_bad_image_reason_from_status_code() {
        let outcome = outcome_with_debug_lines(&[
            r#"LdrLoadDll failed for C:\App\bad.dll Status: 0xC000007B"#,
        ]);
        let detected = detect_for_tests(&outcome).expect("expected dynamic missing");
        assert_eq!(detected.dll, "bad.dll");
        assert_eq!(detected.reason, "BAD_IMAGE");
        assert_eq!(detected.status, Some(0xC000007B));
    }

    #[test]
    fn detects_bad_image_reason_from_invalid_image_status() {
        let outcome = outcome_with_debug_lines(&[
            r#"LdrLoadDll failed for C:\App\bad.dll Status: 0xC000012F"#,
        ]);
        let detected = detect_for_tests(&outcome).expect("expected dynamic missing");
        assert_eq!(detected.dll, "bad.dll");
        assert_eq!(detected.reason, "BAD_IMAGE");
        assert_eq!(detected.status, Some(0xC000012F));
    }

    #[test]
    fn detects_other_reason_from_unknown_status_code() {
        let outcome = outcome_with_debug_lines(&[
            r#"LdrLoadDll failed for C:\App\odd.dll Status: 0xDEADBEEF"#,
        ]);
        let detected = detect_for_tests(&outcome).expect("expected dynamic missing");
        assert_eq!(detected.dll, "odd.dll");
        assert_eq!(detected.reason, "OTHER");
        assert_eq!(detected.status, Some(0xDEADBEEF));
    }

    #[test]
    fn search_path_failure_uses_thread_local_load_context() {
        let outcome = outcome_with_events(vec![
            debug_line(7, r#"LdrLoadDll - ENTER: DLL name: C:\App\spath.dll"#),
            debug_line(7, r#"LdrpSearchPath - RETURN: Status: 0xc0000135"#),
        ]);
        let detected = detect_for_tests(&outcome).expect("expected dynamic missing");
        assert_eq!(detected.dll, "spath.dll");
        assert_eq!(detected.reason, "NOT_FOUND");
    }

    #[test]
    fn init_routine_attach_failure_reports_target_dll() {
        let outcome = outcome_with_debug_lines(&[
            r#"LdrpInitializeNode - ERROR: Init routine 00007FFFECEF10F0 for DLL "C:\App\initfail.dll" failed during DLL_PROCESS_ATTACH"#,
        ]);
        let detected = detect_for_tests(&outcome).expect("expected dynamic missing");
        assert_eq!(detected.dll, "initfail.dll");
        assert_eq!(detected.reason, "OTHER");
    }

    #[test]
    fn init_routine_attach_failure_captures_following_status() {
        let outcome = outcome_with_events(vec![
            debug_line(
                7,
                r#"LdrpInitializeNode - ERROR: Init routine 00007FFFECEF10F0 for DLL "C:\App\initfail.dll" failed during DLL_PROCESS_ATTACH"#,
            ),
            debug_line(7, r#"LdrpLoadDllInternal - RETURN: Status: 0xC0000142"#),
        ]);
        let detected = detect_for_tests(&outcome).expect("expected dynamic missing");
        assert_eq!(detected.dll, "initfail.dll");
        assert_eq!(detected.reason, "OTHER");
        assert_eq!(detected.status, Some(0xC0000142));
    }

    #[test]
    fn windows_and_gac_paths_are_classified_as_framework_or_os() {
        assert!(is_windows_or_gac_path(Path::new(
            r"C:\Windows\System32\foo.dll"
        )));
        assert!(is_windows_or_gac_path(Path::new(
            r"C:\Windows\Microsoft.NET\Framework64\v4.0.30319\bar.dll"
        )));
        assert!(is_windows_or_gac_path(Path::new(
            r"C:\Windows\assembly\GAC_MSIL\baz.dll"
        )));
        assert!(!is_windows_or_gac_path(Path::new(
            r"C:\App\plugins\mine.dll"
        )));
    }

    #[test]
    fn path_is_under_dir_requires_directory_boundary() {
        assert!(path_is_under_dir(
            r"c:\app\plugin.dll",
            &[r"c:\app".to_string()]
        ));
        assert!(!path_is_under_dir(
            r"c:\apptools\plugin.dll",
            &[r"c:\app".to_string()]
        ));
    }
}
