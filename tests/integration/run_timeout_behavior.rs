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

fn make_sleeping_echo_case(
    case_name: &str,
) -> (
    harness::paths::HarnessPaths,
    harness::case::TestCase,
    std::path::PathBuf,
) {
    let paths = harness::paths::require_from_env();
    let case =
        harness::case::TestCase::new(&paths, case_name).expect("failed to initialize test case");
    case.mkdir("app").expect("failed to create app directory");
    let exe = case
        .copy_fixture(
            harness::fixture::HOST_ECHO_ARGV_CWD_EXE,
            "app\\host_echo_argv_cwd.exe",
        )
        .expect("failed to copy echo fixture");
    (paths, case, exe)
}

#[test]
fn summary_mode_timeout_after_runtime_progress_emits_success_line() {
    let (paths, case, exe) = make_sleeping_echo_case("run_timeout_summary");
    let args = vec![
        OsString::from("run"),
        OsString::from("--timeout-ms"),
        OsString::from("100"),
        harness::case::os(&exe),
        OsString::from("--lwtest-sleep-ms"),
        OsString::from("2000"),
    ];
    let result =
        harness::run_loadwhat::run_public(&paths, case.root(), &args, Duration::from_secs(20))
            .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 0);
    assert_eq!(
        token_lines(&result.stdout),
        vec!["SUCCESS status=0"],
        "unexpected summary output for timeout-after-progress.\n{}",
        result.stdout
    );
}

#[test]
fn verbose_mode_timeout_after_runtime_progress_reports_timeout_without_false_diagnosis() {
    let (paths, case, exe) = make_sleeping_echo_case("run_timeout_verbose");
    let args = vec![
        OsString::from("run"),
        OsString::from("--timeout-ms"),
        OsString::from("100"),
        OsString::from("-v"),
        harness::case::os(&exe),
        OsString::from("--lwtest-sleep-ms"),
        OsString::from("2000"),
    ];
    let result =
        harness::run_loadwhat::run_public(&paths, case.root(), &args, Duration::from_secs(20))
            .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 0);
    let lines = token_lines(&result.stdout);
    let run_end = lines
        .iter()
        .copied()
        .find(|line| line.starts_with("RUN_END "))
        .expect("missing RUN_END");
    assert!(
        run_end.contains(r#"exit_kind="TIMEOUT""#),
        "expected timeout RUN_END.\n{}",
        result.stdout
    );
    assert!(
        !lines.iter().any(|line| {
            line.starts_with("STATIC_")
                || line.starts_with("DYNAMIC_")
                || line.starts_with("FIRST_BREAK ")
        }),
        "timeout without a load diagnosis should not invent static/dynamic failures.\n{}",
        result.stdout
    );
}
