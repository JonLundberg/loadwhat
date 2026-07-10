// Parses the public CLI shape and preserves the documented command contract.

use std::env;
use std::ffi::OsString;
use std::path::PathBuf;

#[derive(Debug)]
pub enum Command {
    Run(RunOptions),
    Imports(ImportsOptions),
    Com(ComOptions),
    Help,
}

#[derive(Debug)]
pub struct ComOptions {
    pub sub: ComSubcommand,
    pub trace: bool,
}

#[derive(Debug)]
pub enum ComSubcommand {
    Clsid { query: String, view: ComViewArg },
    Progid { query: String, view: ComViewArg },
    Server { path: PathBuf, view: ComViewArg },
    Audit { target: PathBuf, query: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComViewArg {
    V64,
    V32,
    Both,
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
        "com" => parse_com(&values[1..]),
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

fn parse_com(values: &[OsString]) -> Result<Command, String> {
    if values.is_empty() {
        return Err(format!("error: missing com subcommand\n\n{}", com_usage()));
    }

    let sub = values[0].to_string_lossy().to_ascii_lowercase();
    let rest = &values[1..];

    let mut trace = false;
    let mut view: Option<ComViewArg> = None;
    let mut positionals: Vec<String> = Vec::new();

    let mut i = 0usize;
    while i < rest.len() {
        let token = rest[i].to_string_lossy().to_string();
        if token.starts_with('-') && token.len() > 1 {
            match token.as_str() {
                "--trace" | "--verbose" | "-v" => {
                    trace = true;
                }
                "--summary" => {
                    trace = false;
                }
                "--view" => {
                    i += 1;
                    if i >= rest.len() {
                        return Err(format!("--view requires a value\n\n{}", com_usage()));
                    }
                    let raw = rest[i].to_string_lossy().to_string();
                    view = Some(match raw.as_str() {
                        "64" => ComViewArg::V64,
                        "32" => ComViewArg::V32,
                        "both" => ComViewArg::Both,
                        other => {
                            return Err(format!(
                                "invalid --view value: {other} (expected 64, 32, or both)\n\n{}",
                                com_usage()
                            ));
                        }
                    });
                }
                unknown => {
                    return Err(format!("unknown com option: {unknown}\n\n{}", com_usage()));
                }
            }
        } else {
            positionals.push(token);
        }
        i += 1;
    }

    let sub = match sub.as_str() {
        "clsid" => {
            let query = single_positional(&positionals, "clsid", "<{CLSID}>")?;
            if !is_valid_braced_guid(&query) {
                return Err(format!(
                    "invalid CLSID (expected {{xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx}}): {query}\n\n{}",
                    com_usage()
                ));
            }
            let view = lookup_view(view, "com clsid")?;
            ComSubcommand::Clsid { query, view }
        }
        "progid" => {
            let query = single_positional(&positionals, "progid", "<PROGID>")?;
            if query.starts_with('{') {
                return Err(format!(
                    "invalid ProgID (braced GUIDs are CLSIDs; use com clsid): {query}\n\n{}",
                    com_usage()
                ));
            }
            let view = lookup_view(view, "com progid")?;
            ComSubcommand::Progid { query, view }
        }
        "server" => {
            let path = single_positional(&positionals, "server", "<PATH>")?;
            ComSubcommand::Server {
                path: PathBuf::from(path),
                view: view.unwrap_or(ComViewArg::Both),
            }
        }
        "audit" => {
            if view.is_some() {
                return Err(format!(
                    "com audit does not accept --view; the view derives from the target image\n\n{}",
                    com_usage()
                ));
            }
            if positionals.len() != 2 {
                return Err(format!(
                    "com audit requires <TARGET> <{{CLSID}}|PROGID>\n\n{}",
                    com_usage()
                ));
            }
            let query = positionals[1].clone();
            if query.starts_with('{') && !is_valid_braced_guid(&query) {
                return Err(format!(
                    "invalid CLSID (expected {{xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx}}): {query}\n\n{}",
                    com_usage()
                ));
            }
            ComSubcommand::Audit {
                target: PathBuf::from(positionals[0].clone()),
                query,
            }
        }
        other => {
            return Err(format!(
                "unknown com subcommand: {other}\n\n{}",
                com_usage()
            ));
        }
    };

    Ok(Command::Com(ComOptions { sub, trace }))
}

fn single_positional(
    positionals: &[String],
    sub: &str,
    placeholder: &str,
) -> Result<String, String> {
    match positionals {
        [one] => Ok(one.clone()),
        [] => Err(format!(
            "com {sub} requires {placeholder}\n\n{}",
            com_usage()
        )),
        _ => Err(format!(
            "com {sub} accepts exactly one {placeholder} argument\n\n{}",
            com_usage()
        )),
    }
}

fn lookup_view(view: Option<ComViewArg>, command: &str) -> Result<ComViewArg, String> {
    match view {
        None => Ok(ComViewArg::V64),
        Some(ComViewArg::Both) => Err(format!(
            "{command} does not accept --view both\n\n{}",
            com_usage()
        )),
        Some(value) => Ok(value),
    }
}

/// Validates the canonical braced GUID form: {8-4-4-4-12} hex digits.
pub fn is_valid_braced_guid(value: &str) -> bool {
    let Some(inner) = value
        .strip_prefix('{')
        .and_then(|rest| rest.strip_suffix('}'))
    else {
        return false;
    };
    let groups: Vec<&str> = inner.split('-').collect();
    let expected_lens = [8usize, 4, 4, 4, 12];
    if groups.len() != expected_lens.len() {
        return false;
    }
    groups
        .iter()
        .zip(expected_lens.iter())
        .all(|(group, &len)| group.len() == len && group.chars().all(|c| c.is_ascii_hexdigit()))
}

pub fn usage() -> String {
    let mut out = String::new();
    out.push_str("loadwhat - diagnose Windows DLL loading failures\n\n");
    out.push_str("Usage:\n");
    out.push_str("  loadwhat run [OPTIONS] <TARGET> [TARGET_ARGS...]\n");
    out.push_str("  loadwhat imports <exe_or_dll> [--cwd <dir>]\n");
    out.push_str("  loadwhat com clsid [OPTIONS] <{CLSID}>\n");
    out.push_str("  loadwhat com progid [OPTIONS] <PROGID>\n");
    out.push_str("  loadwhat com server [OPTIONS] <PATH>\n");
    out.push_str("  loadwhat com audit [OPTIONS] <TARGET> <{CLSID}|PROGID>\n");
    out.push_str("  loadwhat help\n");
    out.push_str("\nRun options:\n");
    out.push_str("  --cwd <path>      Working directory for target process\n");
    out.push_str("  --timeout-ms <ms> Maximum runtime before termination; 0 disables deadline\n");
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

fn com_usage() -> String {
    let mut out = String::new();
    out.push_str("Usage:\n");
    out.push_str("  loadwhat com clsid [OPTIONS] <{CLSID}>\n");
    out.push_str("  loadwhat com progid [OPTIONS] <PROGID>\n");
    out.push_str("  loadwhat com server [OPTIONS] <PATH>\n");
    out.push_str("  loadwhat com audit [OPTIONS] <TARGET> <{CLSID}|PROGID>\n");
    out.push_str("\nCom options:\n");
    out.push_str("  --view <64|32>    Registry view for clsid/progid (default 64)\n");
    out.push_str("  --view <64|32|both> Registry views for server (default both)\n");
    out.push_str("  --trace           Print supporting COM trace tokens\n");
    out.push_str("  --summary         Print summary output (default)\n");
    out.push_str("  -v, --verbose     Same as --trace for com commands\n");
    out.push_str("\nBehavior:\n");
    out.push_str("  - com audit derives the registry view from the target image\n");
    out.push_str("  - a braced GUID audit query is a CLSID; anything else is a ProgID\n");
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
        let err = parse_from(["loadwhat", "imports", r"C:\tool\app.exe", "--bogus"]).unwrap_err();
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

    fn parse_com(args: &[&str]) -> super::ComOptions {
        let mut values = vec!["loadwhat", "com"];
        values.extend_from_slice(args);
        match parse_from(values).unwrap() {
            Command::Com(opts) => opts,
            _ => panic!("expected com command"),
        }
    }

    fn parse_com_err(args: &[&str]) -> String {
        let mut values = vec!["loadwhat", "com"];
        values.extend_from_slice(args);
        parse_from(values).unwrap_err()
    }

    const GUID: &str = "{12345678-1234-1234-1234-123456789ABC}";

    #[test]
    fn com_clsid_parses_valid_guid_with_default_view() {
        let opts = parse_com(&["clsid", GUID]);
        assert!(!opts.trace);
        match opts.sub {
            super::ComSubcommand::Clsid { query, view } => {
                assert_eq!(query, GUID);
                assert_eq!(view, super::ComViewArg::V64);
            }
            other => panic!("expected clsid subcommand, got {other:?}"),
        }
    }

    #[test]
    fn com_clsid_rejects_malformed_guid() {
        let err = parse_com_err(&["clsid", "{not-a-guid}"]);
        assert!(err.contains("invalid CLSID"));
    }

    #[test]
    fn com_clsid_rejects_unbraced_guid() {
        let err = parse_com_err(&["clsid", "12345678-1234-1234-1234-123456789ABC"]);
        assert!(err.contains("invalid CLSID"));
    }

    #[test]
    fn com_clsid_accepts_view_32() {
        let opts = parse_com(&["clsid", "--view", "32", GUID]);
        match opts.sub {
            super::ComSubcommand::Clsid { view, .. } => {
                assert_eq!(view, super::ComViewArg::V32);
            }
            other => panic!("expected clsid subcommand, got {other:?}"),
        }
    }

    #[test]
    fn com_clsid_rejects_view_both() {
        let err = parse_com_err(&["clsid", "--view", "both", GUID]);
        assert!(err.contains("does not accept --view both"));
    }

    #[test]
    fn com_progid_rejects_braced_query() {
        let err = parse_com_err(&["progid", GUID]);
        assert!(err.contains("use com clsid"));
    }

    #[test]
    fn com_progid_parses_with_trace() {
        let opts = parse_com(&["progid", "--trace", "Vendor.Widget"]);
        assert!(opts.trace);
        match opts.sub {
            super::ComSubcommand::Progid { query, .. } => assert_eq!(query, "Vendor.Widget"),
            other => panic!("expected progid subcommand, got {other:?}"),
        }
    }

    #[test]
    fn com_trace_then_summary_disables_trace() {
        let opts = parse_com(&["progid", "--trace", "--summary", "Vendor.Widget"]);
        assert!(!opts.trace);
    }

    #[test]
    fn com_verbose_equals_trace() {
        let opts = parse_com(&["progid", "-v", "Vendor.Widget"]);
        assert!(opts.trace);
    }

    #[test]
    fn com_server_defaults_to_both_views() {
        let opts = parse_com(&["server", r"C:\Vendor\foo.dll"]);
        match opts.sub {
            super::ComSubcommand::Server { path, view } => {
                assert_eq!(path, PathBuf::from(r"C:\Vendor\foo.dll"));
                assert_eq!(view, super::ComViewArg::Both);
            }
            other => panic!("expected server subcommand, got {other:?}"),
        }
    }

    #[test]
    fn com_server_accepts_explicit_view() {
        let opts = parse_com(&["server", "--view", "64", r"C:\Vendor\foo.dll"]);
        match opts.sub {
            super::ComSubcommand::Server { view, .. } => {
                assert_eq!(view, super::ComViewArg::V64);
            }
            other => panic!("expected server subcommand, got {other:?}"),
        }
    }

    #[test]
    fn com_audit_classifies_braced_query_as_clsid() {
        let opts = parse_com(&["audit", r"C:\app.exe", GUID]);
        match opts.sub {
            super::ComSubcommand::Audit { target, query } => {
                assert_eq!(target, PathBuf::from(r"C:\app.exe"));
                assert_eq!(query, GUID);
            }
            other => panic!("expected audit subcommand, got {other:?}"),
        }
    }

    #[test]
    fn com_audit_rejects_view_option() {
        let err = parse_com_err(&["audit", "--view", "64", r"C:\app.exe", GUID]);
        assert!(err.contains("com audit does not accept --view"));
    }

    #[test]
    fn com_audit_rejects_malformed_braced_query() {
        let err = parse_com_err(&["audit", r"C:\app.exe", "{oops}"]);
        assert!(err.contains("invalid CLSID"));
    }

    #[test]
    fn com_audit_requires_two_positionals() {
        let err = parse_com_err(&["audit", r"C:\app.exe"]);
        assert!(err.contains("com audit requires"));
    }

    #[test]
    fn com_missing_subcommand_reports_usage() {
        let err = parse_com_err(&[]);
        assert!(err.contains("missing com subcommand"));
    }

    #[test]
    fn com_unknown_subcommand_reports_error() {
        let err = parse_com_err(&["bogus"]);
        assert!(err.contains("unknown com subcommand: bogus"));
    }

    #[test]
    fn com_unknown_option_reports_error() {
        let err = parse_com_err(&["clsid", "--bogus", GUID]);
        assert!(err.contains("unknown com option: --bogus"));
    }

    #[test]
    fn com_view_requires_value() {
        let err = parse_com_err(&["clsid", GUID, "--view"]);
        assert!(err.contains("--view requires a value"));
    }

    #[test]
    fn com_view_rejects_invalid_value() {
        let err = parse_com_err(&["clsid", "--view", "16", GUID]);
        assert!(err.contains("invalid --view value: 16"));
    }

    #[test]
    fn com_clsid_requires_exactly_one_query() {
        assert!(parse_com_err(&["clsid"]).contains("com clsid requires"));
        assert!(parse_com_err(&["clsid", GUID, GUID]).contains("accepts exactly one"));
    }

    #[test]
    fn is_valid_braced_guid_accepts_canonical_form() {
        assert!(super::is_valid_braced_guid(GUID));
        assert!(super::is_valid_braced_guid(
            "{00000000-0000-0000-0000-000000000000}"
        ));
        assert!(super::is_valid_braced_guid(
            "{abcdefAB-cdef-ABCD-efab-cdefabcdefab}"
        ));
    }

    #[test]
    fn is_valid_braced_guid_rejects_bad_forms() {
        assert!(!super::is_valid_braced_guid(""));
        assert!(!super::is_valid_braced_guid("{}"));
        assert!(!super::is_valid_braced_guid("{1234}"));
        assert!(!super::is_valid_braced_guid(
            "12345678-1234-1234-1234-123456789ABC"
        ));
        assert!(!super::is_valid_braced_guid(
            "{12345678-1234-1234-1234-123456789ABG}"
        ));
        assert!(!super::is_valid_braced_guid(
            "{12345678-1234-1234-1234-123456789ABC"
        ));
    }

    #[test]
    fn usage_mentions_com_commands() {
        let help = super::usage();
        assert!(help.contains("loadwhat com clsid [OPTIONS] <{CLSID}>"));
        assert!(help.contains("loadwhat com audit [OPTIONS] <TARGET> <{CLSID}|PROGID>"));
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
