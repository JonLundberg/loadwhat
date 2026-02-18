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
use std::collections::HashSet;
#[cfg(windows)]
use std::ffi::OsString;
#[cfg(windows)]
use std::path::{Path, PathBuf};

#[cfg(windows)]
use cli::{Command, ImportsOptions, RunOptions};
#[cfg(windows)]
use debug_run::{RunEndKind, RunOutcome, RuntimeEvent};
#[cfg(windows)]
use emit::{emit, field, hex_u32, hex_usize, quote};
#[cfg(windows)]
use loader_snaps::LoaderSnapsGuard;
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

    let mut snaps_guard = if opts.loader_snaps {
        let image_name = exe_path
            .file_name()
            .map(|v| v.to_string_lossy().to_string())
            .unwrap_or_default();
        match LoaderSnapsGuard::enable_for_image(&image_name) {
            Ok(guard) => Some(guard),
            Err(code) => {
                emit(
                    "NOTE",
                    &vec![
                        field("topic", quote("loader-snaps")),
                        field("detail", quote("enable-failed")),
                        field("code", hex_u32(code)),
                    ],
                );
                return 21;
            }
        }
    } else {
        None
    };

    let outcome = debug_run::run_target(&exe_path, &opts.exe_args, Some(&cwd), opts.timeout_ms);

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
        Err(err) => {
            eprintln!("{err}");
            return 21;
        }
    };

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
                                emit(
                                    "STATIC_MISSING",
                                    &vec![
                                        field("module", quote(&issue.module)),
                                        field("dll", quote(&issue.dll)),
                                        field("reason", quote("NOT_FOUND")),
                                    ],
                                );
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
struct FirstIssue {
    module: String,
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
    let imports = pe::direct_imports(module_path)?;
    let context = SearchContext::from_environment(app_dir, cwd, path_env_override)?;
    let module_name = module_path
        .file_name()
        .map(|v| v.to_string_lossy().to_string())
        .unwrap_or_else(|| module_path.display().to_string());

    if matches!(emit_mode, StaticEmitMode::Full) {
        emit(
            "STATIC_START",
            &vec![
                field("module", quote(&display_path(module_path))),
                field("scope", quote("direct-imports")),
            ],
        );
        emit(
            "SEARCH_ORDER",
            &vec![field("safedll", if context.safedll { "1" } else { "0" })],
        );
    }

    let mut missing = 0usize;
    let mut first_issue = None;

    for dll in imports {
        if matches!(emit_mode, StaticEmitMode::Full) {
            emit(
                "STATIC_IMPORT",
                &vec![
                    field("module", quote(&module_name)),
                    field("needs", quote(&dll)),
                ],
            );
        }

        if runtime_loaded.contains(&dll) {
            if matches!(emit_mode, StaticEmitMode::Full) {
                emit(
                    "STATIC_FOUND",
                    &vec![
                        field("module", quote(&module_name)),
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
                            field("module", quote(&module_name)),
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
            }
            ResolutionKind::Missing => {
                missing += 1;
                if first_issue.is_none() {
                    first_issue = Some(FirstIssue {
                        module: module_name.clone(),
                        dll: dll.clone(),
                        diagnosis: "MISSING_STATIC_IMPORT",
                        kind: ResolutionKind::Missing,
                        candidates: resolution.candidates.clone(),
                    });
                }
                if matches!(emit_mode, StaticEmitMode::Full) {
                    emit(
                        "STATIC_MISSING",
                        &vec![
                            field("module", quote(&module_name)),
                            field("dll", quote(&dll)),
                            field("reason", quote("NOT_FOUND")),
                        ],
                    );
                }
            }
            ResolutionKind::BadImage => {
                missing += 1;
                if first_issue.is_none() {
                    first_issue = Some(FirstIssue {
                        module: module_name.clone(),
                        dll: dll.clone(),
                        diagnosis: "BAD_STATIC_IMPORT_IMAGE",
                        kind: ResolutionKind::BadImage,
                        candidates: resolution.candidates.clone(),
                    });
                }
                if matches!(emit_mode, StaticEmitMode::Full) {
                    emit(
                        "STATIC_BAD_IMAGE",
                        &vec![
                            field("module", quote(&module_name)),
                            field("dll", quote(&dll)),
                            field("reason", quote("BAD_IMAGE")),
                        ],
                    );
                }
            }
        }

        if matches!(emit_mode, StaticEmitMode::FailuresOnly) && first_issue.is_some() {
            break;
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
