// Parses the public CLI shape and preserves the documented command contract.

use std::env;
use std::ffi::OsString;
use std::path::PathBuf;

#[derive(Debug)]
pub enum Command {
    Run(RunOptions),
    Imports(ImportsOptions),
    Help,
}

#[derive(Debug)]
pub struct RunOptions {
    pub exe_path: PathBuf,
    pub exe_args: Vec<OsString>,
    pub cwd: Option<PathBuf>,
    pub timeout_ms: u32,
    pub loader_snaps: bool,
    pub trace: bool,
    pub verbose: bool,
}

#[derive(Debug)]
pub struct ImportsOptions {
    pub module_path: PathBuf,
    pub cwd: Option<PathBuf>,
}

pub fn parse() -> Result<Command, String> {
    parse_from(env::args_os())
}

pub fn parse_from<I, T>(args: I) -> Result<Command, String>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString>,
{
    let mut values: Vec<OsString> = args.into_iter().map(Into::into).collect();
    if values.is_empty() {
        return Err(usage());
    }

    values.remove(0);
    if values.is_empty() {
        return Ok(Command::Help);
    }

    let sub = values[0].to_string_lossy().to_ascii_lowercase();
    match sub.as_str() {
        "run" => parse_run(&values[1..]),
        "imports" => parse_imports(&values[1..]),
        "-h" | "--help" | "help" => Ok(Command::Help),
        other => Err(format!("unknown command: {other}\n\n{}", usage())),
    }
}

fn parse_run(values: &[OsString]) -> Result<Command, String> {
    if values.is_empty() {
        return Err(format!(
            "error: missing target executable\n\n{}",
            run_usage()
        ));
    }

    let mut cwd = None;
    let mut timeout_ms = 30_000;
    let mut loader_snaps = true;
    let mut trace = false;
    let mut verbose = false;

    let mut i = 0usize;
    while i < values.len() {
        let token = values[i].to_string_lossy().to_string();
        if !looks_like_run_option(&token) {
            break;
        }

        match token.as_str() {
            "--cwd" => {
                i += 1;
                if i >= values.len() {
                    return Err(format!("--cwd requires a value\n\n{}", run_usage()));
                }
                cwd = Some(PathBuf::from(values[i].clone()));
            }
            "--timeout" | "--timeout-ms" => {
                i += 1;
                if i >= values.len() {
                    return Err(format!("{token} requires a value\n\n{}", run_usage()));
                }
                let raw = values[i].to_string_lossy();
                timeout_ms = raw
                    .parse::<u32>()
                    .map_err(|_| format!("invalid {token} value: {raw}\n\n{}", run_usage()))?;
            }
            "--verbose" | "-v" => {
                verbose = true;
                trace = true;
            }
            "--trace" => {
                trace = true;
            }
            "--summary" => {
                trace = false;
            }
            "--loader-snaps" => {
                loader_snaps = true;
            }
            "--no-loader-snaps" => {
                loader_snaps = false;
            }
            "--quiet" => {
                verbose = false;
            }
            "--strict" => {}
            unknown => {
                return Err(format!("unknown run option: {unknown}\n\n{}", run_usage()));
            }
        }

        i += 1;
    }

    if i >= values.len() {
        return Err(format!(
            "error: missing target executable\n\n{}",
            run_usage()
        ));
    }

    let exe_path = PathBuf::from(values[i].clone());
    let exe_args = values[i + 1..].to_vec();

    Ok(Command::Run(RunOptions {
        exe_path,
        exe_args,
        cwd,
        timeout_ms,
        loader_snaps,
        trace,
        verbose,
    }))
}

fn parse_imports(values: &[OsString]) -> Result<Command, String> {
    if values.is_empty() {
        return Err(format!("missing <exe_or_dll>\n\n{}", usage()));
    }

    let module_path = PathBuf::from(values[0].clone());
    let mut cwd = None;

    let mut i = 1usize;
    while i < values.len() {
        let token = values[i].to_string_lossy().to_string();
        match token.as_str() {
            "--cwd" => {
                i += 1;
                if i >= values.len() {
                    return Err(format!("--cwd requires a value\n\n{}", usage()));
                }
                cwd = Some(PathBuf::from(values[i].clone()));
            }
            "--quiet" | "--verbose" | "--strict" => {}
            unknown => {
                return Err(format!("unknown imports option: {unknown}\n\n{}", usage()));
            }
        }

        i += 1;
    }

    Ok(Command::Imports(ImportsOptions { module_path, cwd }))
}

pub fn usage() -> String {
    let mut out = String::new();
    out.push_str("loadwhat - diagnose Windows DLL loading failures\n\n");
    out.push_str("Usage:\n");
    out.push_str("  loadwhat run [OPTIONS] <TARGET> [TARGET_ARGS...]\n");
    out.push_str("  loadwhat imports <exe_or_dll> [--cwd <dir>]\n");
    out.push_str("  loadwhat help\n");
    out.push_str("\nRun options:\n");
    out.push_str("  --cwd <path>      Working directory for target process\n");
    out.push_str("  --timeout-ms <ms> Maximum runtime before termination\n");
    out.push_str("  --trace           Print diagnostic trace output\n");
    out.push_str("  --summary         Print summary output (default)\n");
    out.push_str("  -v, --verbose     Print detailed diagnostic output\n");
    out.push_str("  --quiet           Disable verbose runtime detail\n");
    out.push_str("  --no-loader-snaps Disable loader-snaps Phase C search\n");
    out.push_str("\nBehavior:\n");
    out.push_str("  - Loader-snaps Phase C search is enabled by default\n");
    out.push_str("  - Use --no-loader-snaps to disable it\n");
    out.push_str("  - All run options must appear before <TARGET>\n");
    out.push_str("  - Arguments after <TARGET> are passed directly to the target program\n");
    out
}

fn run_usage() -> String {
    let mut out = String::new();
    out.push_str("Usage:\n");
    out.push_str("  loadwhat run [OPTIONS] <TARGET> [TARGET_ARGS...]\n");
    out
}

fn looks_like_run_option(token: &str) -> bool {
    token.starts_with('-') && token.len() > 1
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::path::PathBuf;

    use super::{parse_from, Command, ImportsOptions, RunOptions};

    fn parse_run(args: &[&str]) -> RunOptions {
        let mut values = vec!["loadwhat", "run"];
        values.extend_from_slice(args);
        match parse_from(values).unwrap() {
            Command::Run(opts) => opts,
            _ => panic!("expected run command"),
        }
    }

    fn parse_run_err(args: &[&str]) -> String {
        let mut values = vec!["loadwhat", "run"];
        values.extend_from_slice(args);
        parse_from(values).unwrap_err()
    }

    fn parse_imports(args: &[&str]) -> ImportsOptions {
        let mut values = vec!["loadwhat", "imports"];
        values.extend_from_slice(args);
        match parse_from(values).unwrap() {
            Command::Imports(opts) => opts,
            _ => panic!("expected imports command"),
        }
    }

    #[test]
    fn parses_run_target_args_without_separator() {
        let opts = parse_run(&["--timeout-ms", "1234", r"C:\tool\app.exe", "--flag"]);
        assert_eq!(opts.timeout_ms, 1234);
        assert_eq!(opts.exe_args, vec![OsString::from("--flag")]);
    }

    #[test]
    fn parses_run_verbose_short_flag() {
        let opts = parse_run(&["-v", "notepad.exe"]);
        assert!(opts.verbose);
        assert!(opts.trace);
    }

    #[test]
    fn run_defaults_loader_snaps_on() {
        let opts = parse_run(&["notepad.exe"]);
        assert!(opts.loader_snaps);
        assert_eq!(opts.timeout_ms, 30_000);
        assert!(!opts.trace);
        assert!(!opts.verbose);
    }

    #[test]
    fn parses_run_no_loader_snaps_flag() {
        let opts = parse_run(&["--no-loader-snaps", "notepad.exe"]);
        assert!(!opts.loader_snaps);
    }

    #[test]
    fn parses_run_trace_flag() {
        let opts = parse_run(&["--trace", "notepad.exe"]);
        assert!(opts.trace);
    }

    #[test]
    fn target_like_option_after_target_is_passed_through() {
        let opts = parse_run(&["notepad.exe", "--verbose"]);
        assert!(!opts.verbose);
        assert_eq!(opts.exe_args, vec![OsString::from("--verbose")]);
    }

    #[test]
    fn missing_target_reports_usage() {
        let err = parse_run_err(&[]);
        assert!(err.contains("error: missing target executable"));
        assert!(err.contains("loadwhat run [OPTIONS] <TARGET> [TARGET_ARGS...]"));
    }

    #[test]
    fn missing_cwd_value_reports_error() {
        let err = parse_run_err(&["--cwd"]);
        assert!(err.contains("--cwd requires a value"));
    }

    #[test]
    fn missing_timeout_value_reports_error() {
        let err = parse_run_err(&["--timeout-ms"]);
        assert!(err.contains("--timeout-ms requires a value"));
    }

    #[test]
    fn invalid_timeout_value_reports_error() {
        let err = parse_run_err(&["--timeout-ms", "nope", "notepad.exe"]);
        assert!(err.contains("invalid --timeout-ms value: nope"));
    }

    #[test]
    fn unknown_pre_target_long_option_reports_error() {
        let err = parse_run_err(&["--bogus", "notepad.exe"]);
        assert!(err.contains("unknown run option: --bogus"));
    }

    #[test]
    fn unknown_pre_target_short_option_reports_error() {
        let err = parse_run_err(&["-z", "notepad.exe"]);
        assert!(err.contains("unknown run option: -z"));
    }

    #[test]
    fn summary_then_verbose_enables_trace() {
        let opts = parse_run(&["--summary", "-v", "notepad.exe"]);
        assert!(opts.verbose);
        assert!(opts.trace);
    }

    #[test]
    fn verbose_then_summary_keeps_verbose_but_disables_trace() {
        let opts = parse_run(&["-v", "--summary", "notepad.exe"]);
        assert!(opts.verbose);
        assert!(!opts.trace);
    }

    #[test]
    fn quiet_does_not_clear_explicit_trace() {
        let opts = parse_run(&["--trace", "--quiet", "notepad.exe"]);
        assert!(!opts.verbose);
        assert!(opts.trace);
    }

    #[test]
    fn trace_after_quiet_reenables_trace() {
        let opts = parse_run(&["--quiet", "--trace", "notepad.exe"]);
        assert!(!opts.verbose);
        assert!(opts.trace);
    }

    #[test]
    fn repeated_cwd_uses_last_value() {
        let opts = parse_run(&["--cwd", r"C:\one", "--cwd", r"C:\two", "notepad.exe"]);
        assert_eq!(opts.cwd, Some(PathBuf::from(r"C:\two")));
    }

    #[test]
    fn repeated_timeout_uses_last_value() {
        let opts = parse_run(&["--timeout-ms", "1", "--timeout", "2", "notepad.exe"]);
        assert_eq!(opts.timeout_ms, 2);
    }

    #[test]
    fn repeated_loader_snaps_flags_use_last_value() {
        let opts = parse_run(&[
            "--no-loader-snaps",
            "--loader-snaps",
            "--no-loader-snaps",
            "notepad.exe",
        ]);
        assert!(!opts.loader_snaps);
    }

    #[test]
    fn later_loader_snaps_flag_can_reenable_loader_snaps() {
        let opts = parse_run(&["--no-loader-snaps", "--loader-snaps", "notepad.exe"]);
        assert!(opts.loader_snaps);
    }

    #[test]
    fn imports_parses_module_only() {
        let opts = parse_imports(&[r"C:\tool\app.exe"]);
        assert_eq!(opts.module_path, PathBuf::from(r"C:\tool\app.exe"));
        assert_eq!(opts.cwd, None);
    }

    #[test]
    fn imports_parses_optional_cwd() {
        let opts = parse_imports(&[r"C:\tool\app.exe", "--cwd", r"C:\work"]);
        assert_eq!(opts.cwd, Some(PathBuf::from(r"C:\work")));
    }

    #[test]
    fn imports_missing_cwd_value_reports_error() {
        let err = parse_from(["loadwhat", "imports", r"C:\tool\app.exe", "--cwd"]).unwrap_err();
        assert!(err.contains("--cwd requires a value"));
    }

    #[test]
    fn imports_unknown_option_reports_error() {
        let err =
            parse_from(["loadwhat", "imports", r"C:\tool\app.exe", "--bogus"]).unwrap_err();
        assert!(err.contains("unknown imports option: --bogus"));
    }

    #[test]
    fn imports_ignores_quiet_verbose_and_strict() {
        let opts = parse_imports(&[
            r"C:\tool\app.exe",
            "--quiet",
            "--verbose",
            "--strict",
            "--cwd",
            r"C:\work",
        ]);
        assert_eq!(opts.cwd, Some(PathBuf::from(r"C:\work")));
    }

    #[test]
    fn usage_describes_new_run_interface() {
        let help = super::usage();
        assert!(help.contains("loadwhat run [OPTIONS] <TARGET> [TARGET_ARGS...]"));
        assert!(help.contains("--timeout-ms <ms>"));
        assert!(help.contains("--no-loader-snaps"));
        assert!(help.contains("Loader-snaps Phase C search is enabled by default"));
    }
}
