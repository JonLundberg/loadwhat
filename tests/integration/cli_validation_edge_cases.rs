use crate::harness;
use std::ffi::OsString;
use std::time::Duration;

fn token_lines(stdout: &str) -> Vec<&str> {
    stdout
        .lines()
        .map(|line| line.trim())
        .filter(|line| {
            !line.is_empty()
                && (line.starts_with("STATIC_")
                    || line.starts_with("DYNAMIC_")
                    || line.starts_with("SEARCH_")
                    || line.starts_with("RUN_")
                    || line.starts_with("RUNTIME_")
                    || line.starts_with("FIRST_BREAK")
                    || line.starts_with("SUMMARY")
                    || line.starts_with("SUCCESS")
                    || line.starts_with("NOTE ")
                    || line.starts_with("DEBUG_STRING"))
        })
        .collect()
}

#[test]
fn cli_missing_target_returns_exit_20() {
    let paths = harness::paths::require_from_env();
    let case = harness::case::TestCase::new(&paths, "cli_missing_target")
        .expect("failed to initialize test case");

    let args = vec![OsString::from("run")];
    let result =
        harness::run_loadwhat::run_public(&paths, case.root(), &args, Duration::from_secs(20))
            .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 20);
    assert!(
        result.stderr.to_ascii_lowercase().contains("missing target executable")
            || result.stdout.to_ascii_lowercase().contains("missing target executable"),
        "expected 'missing target executable' in output.\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(
        !token_lines(&result.stdout).iter().any(|line| {
            line.starts_with("STATIC_")
                || line.starts_with("DYNAMIC_")
                || line.starts_with("SUCCESS")
        }),
        "CLI parse error should not produce diagnosis or success tokens.\n{}",
        result.stdout
    );
}

#[test]
fn cli_nonexistent_target_returns_exit_20() {
    let paths = harness::paths::require_from_env();
    let case = harness::case::TestCase::new(&paths, "cli_nonexistent_target")
        .expect("failed to initialize test case");

    let args = vec![
        OsString::from("run"),
        OsString::from(r"C:\nonexistent_path_12345\fake.exe"),
    ];
    let result =
        harness::run_loadwhat::run_public(&paths, case.root(), &args, Duration::from_secs(20))
            .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    // normalize_existing_run_target returns exit 20 when the path doesn't exist
    harness::assert::assert_exit_code(&result, 20);
    assert!(
        !token_lines(&result.stdout).iter().any(|line| {
            line.starts_with("STATIC_")
                || line.starts_with("DYNAMIC_")
                || line.starts_with("SUCCESS")
        }),
        "nonexistent target should not produce diagnosis or success tokens.\n{}",
        result.stdout
    );
}

#[test]
fn cli_very_large_timeout_is_accepted() {
    let paths = harness::paths::require_from_env();
    let case = harness::case::TestCase::new(&paths, "cli_large_timeout")
        .expect("failed to initialize test case");
    case.mkdir("app").expect("failed to create app directory");
    let exe = case
        .copy_fixture(
            harness::fixture::HOST_ECHO_ARGV_CWD_EXE,
            "app\\host_echo_argv_cwd.exe",
        )
        .expect("failed to copy echo fixture");

    let args = vec![
        OsString::from("run"),
        OsString::from("--timeout-ms"),
        OsString::from("4294967295"),
        harness::case::os(&exe),
    ];
    let result =
        harness::run_loadwhat::run_public(&paths, case.root(), &args, Duration::from_secs(20))
            .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 0);
    assert_eq!(
        token_lines(&result.stdout),
        vec!["SUCCESS status=0"],
        "large timeout should parse and target should exit normally.\n{}",
        result.stdout
    );
}

#[test]
fn cli_timeout_overflow_returns_parse_error() {
    let paths = harness::paths::require_from_env();
    let case = harness::case::TestCase::new(&paths, "cli_timeout_overflow")
        .expect("failed to initialize test case");
    case.mkdir("app").expect("failed to create app directory");
    let exe = case
        .copy_fixture(
            harness::fixture::HOST_ECHO_ARGV_CWD_EXE,
            "app\\host_echo_argv_cwd.exe",
        )
        .expect("failed to copy echo fixture");

    let args = vec![
        OsString::from("run"),
        OsString::from("--timeout-ms"),
        OsString::from("4294967296"),
        harness::case::os(&exe),
    ];
    let result =
        harness::run_loadwhat::run_public(&paths, case.root(), &args, Duration::from_secs(20))
            .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 20);
    let combined = format!("{}\n{}", result.stdout, result.stderr).to_ascii_lowercase();
    assert!(
        combined.contains("invalid --timeout-ms value"),
        "expected 'invalid --timeout-ms value' error message.\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
}
