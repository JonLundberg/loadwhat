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
        return Err(format!("missing <exe_path>\n\n{}", usage()));
    }

    let exe_path = PathBuf::from(values[0].clone());
    let mut cwd = None;
    let mut timeout_ms = 30_000;
    let mut loader_snaps = false;
    let mut exe_args = Vec::new();
    let mut verbose = false;

    let mut i = 1usize;
    while i < values.len() {
        let token = values[i].to_string_lossy().to_string();
        if token == "--" {
            exe_args.extend(values[i + 1..].iter().cloned());
            break;
        }

        match token.as_str() {
            "--cwd" => {
                i += 1;
                if i >= values.len() {
                    return Err(format!("--cwd requires a value\n\n{}", usage()));
                }
                cwd = Some(PathBuf::from(values[i].clone()));
            }
            "--timeout-ms" => {
                i += 1;
                if i >= values.len() {
                    return Err(format!("--timeout-ms requires a value\n\n{}", usage()));
                }
                let raw = values[i].to_string_lossy();
                timeout_ms = raw
                    .parse::<u32>()
                    .map_err(|_| format!("invalid --timeout-ms value: {raw}\n\n{}", usage()))?;
            }
            "--verbose" | "-v" => {
                verbose = true;
            }
            "--loader-snaps" => {
                loader_snaps = true;
            }
            "--quiet" => {
                verbose = false;
            }
            "--strict" => {}
            unknown => {
                return Err(format!("unknown run option: {unknown}\n\n{}", usage()));
            }
        }

        i += 1;
    }

    Ok(Command::Run(RunOptions {
        exe_path,
        exe_args,
        cwd,
        timeout_ms,
        loader_snaps,
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
    out.push_str(
        "  loadwhat run <exe_path> [--cwd <dir>] [--timeout-ms <n>] [--loader-snaps] [-v|--verbose] [-- <args...>]\n",
    );
    out.push_str("  loadwhat imports <exe_or_dll> [--cwd <dir>]\n");
    out.push_str("  loadwhat help\n");
    out
}

#[cfg(test)]
mod tests {
    use super::{parse_from, Command};

    #[test]
    fn parses_run_with_separator() {
        let cmd = parse_from([
            "loadwhat",
            "run",
            r"C:\tool\app.exe",
            "--timeout-ms",
            "1234",
            "--",
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
        let cmd = parse_from(["loadwhat", "run", "notepad.exe", "-v"]).unwrap();
        match cmd {
            Command::Run(opts) => {
                assert!(opts.verbose);
            }
            _ => panic!("expected run command"),
        }
    }

    #[test]
    fn parses_run_loader_snaps_flag() {
        let cmd = parse_from(["loadwhat", "run", "notepad.exe", "--loader-snaps"]).unwrap();
        match cmd {
            Command::Run(opts) => {
                assert!(opts.loader_snaps);
            }
            _ => panic!("expected run command"),
        }
    }
}
