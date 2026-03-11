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

    if verbose {
        trace = true;
    }

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
    out.push_str("  --timeout <ms>    Maximum runtime before termination\n");
    out.push_str("  --trace           Print diagnostic trace output\n");
    out.push_str("  --summary         Print summary output (default)\n");
    out.push_str("  -v, --verbose     Print detailed diagnostic output\n");
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
    use super::{parse_from, Command};

    #[test]
    fn parses_run_target_args_without_separator() {
        let cmd = parse_from([
            "loadwhat",
            "run",
            "--timeout-ms",
            "1234",
            r"C:\tool\app.exe",
            "--flag",
        ])
        .unwrap();

        match cmd {
            Command::Run(opts) => {
                assert_eq!(opts.timeout_ms, 1234);
                assert_eq!(opts.exe_args.len(), 1);
            }
            _ => panic!("expected run command"),
        }
    }

    #[test]
    fn parses_run_verbose_short_flag() {
        let cmd = parse_from(["loadwhat", "run", "-v", "notepad.exe"]).unwrap();
        match cmd {
            Command::Run(opts) => {
                assert!(opts.verbose);
            }
            _ => panic!("expected run command"),
        }
    }

    #[test]
    fn run_defaults_loader_snaps_on() {
        let cmd = parse_from(["loadwhat", "run", "notepad.exe"]).unwrap();
        match cmd {
            Command::Run(opts) => {
                assert!(opts.loader_snaps);
            }
            _ => panic!("expected run command"),
        }
    }

    #[test]
    fn parses_run_no_loader_snaps_flag() {
        let cmd = parse_from(["loadwhat", "run", "--no-loader-snaps", "notepad.exe"]).unwrap();
        match cmd {
            Command::Run(opts) => {
                assert!(!opts.loader_snaps);
            }
            _ => panic!("expected run command"),
        }
    }

    #[test]
    fn parses_run_trace_flag() {
        let cmd = parse_from(["loadwhat", "run", "--trace", "notepad.exe"]).unwrap();
        match cmd {
            Command::Run(opts) => {
                assert!(opts.trace);
            }
            _ => panic!("expected run command"),
        }
    }

    #[test]
    fn verbose_implies_trace() {
        let cmd = parse_from(["loadwhat", "run", "--summary", "-v", "notepad.exe"]).unwrap();
        match cmd {
            Command::Run(opts) => {
                assert!(opts.verbose);
                assert!(opts.trace);
            }
            _ => panic!("expected run command"),
        }
    }

    #[test]
    fn target_like_option_after_target_is_passed_through() {
        let cmd = parse_from(["loadwhat", "run", "notepad.exe", "--verbose"]).unwrap();
        match cmd {
            Command::Run(opts) => {
                assert!(!opts.verbose);
                assert_eq!(opts.exe_args, vec![std::ffi::OsString::from("--verbose")]);
            }
            _ => panic!("expected run command"),
        }
    }

    #[test]
    fn missing_target_uses_new_error_message() {
        let err = parse_from(["loadwhat", "run", "--verbose"]).unwrap_err();
        assert!(err.contains("error: missing target executable"));
        assert!(err.contains("loadwhat run [OPTIONS] <TARGET> [TARGET_ARGS...]"));
    }

    #[test]
    fn usage_describes_new_run_interface() {
        let help = super::usage();
        assert!(help.contains("loadwhat run [OPTIONS] <TARGET> [TARGET_ARGS...]"));
        assert!(help.contains("--no-loader-snaps"));
        assert!(help.contains("Loader-snaps Phase C search is enabled by default"));
    }
}
