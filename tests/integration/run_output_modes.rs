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
                    || line.starts_with("FIRST_BREAK")
                    || line.starts_with("SUMMARY")
                    || line.starts_with("SUCCESS")
                    || line.starts_with("NOTE "))
        })
        .collect()
}

#[test]
fn run_summary_emits_single_static_missing_line() {
    let Some(paths) = harness::paths::from_env() else {
        return;
    };

    let case = harness::case::TestCase::new(&paths, "run_summary_missing")
        .expect("failed to initialize test case");
    let app_dir = case.mkdir("app").expect("failed to create app directory");
    let exe = case
        .copy_fixture(
            harness::fixture::HOST_STATIC_IMPORTS_MISSING_EXE,
            "app\\host_static_imports_missing.exe",
        )
        .expect("failed to copy host fixture");

    let args = vec![
        OsString::from("run"),
        harness::case::os(&exe),
        OsString::from("--cwd"),
        harness::case::os(&app_dir),
    ];
    let result =
        harness::run_loadwhat::run_public(&paths, case.root(), &args, Duration::from_secs(20))
            .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 10);

    let lines = token_lines(&result.stdout);
    assert_eq!(
        lines.len(),
        1,
        "expected one summary line.\n{}",
        result.stdout
    );
    assert!(
        lines[0].starts_with("STATIC_MISSING ") && lines[0].contains(r#"dll="lwtest_a.dll""#),
        "unexpected summary line: {}",
        lines[0]
    );
    assert!(
        !result.stdout.contains("SEARCH_ORDER")
            && !result.stdout.contains("SEARCH_PATH")
            && !result.stdout.contains("NOTE "),
        "summary mode should not emit trace/note lines.\n{}",
        result.stdout
    );
}

#[test]
fn run_summary_emits_success_line() {
    let Some(paths) = harness::paths::from_env() else {
        return;
    };

    let case = harness::case::TestCase::new(&paths, "run_summary_success")
        .expect("failed to initialize test case");
    let app_dir = case.mkdir("app").expect("failed to create app directory");
    let exe = case
        .copy_fixture(
            harness::fixture::HOST_STATIC_IMPORTS_A_EXE,
            "app\\host_static_imports_a.exe",
        )
        .expect("failed to copy host fixture");
    case.copy_fixture_as(harness::fixture::DLL_LWTEST_A, "app", "lwtest_a.dll")
        .expect("failed to copy app lwtest_a.dll");
    case.copy_fixture_as(harness::fixture::DLL_LWTEST_B, "app", "lwtest_b.dll")
        .expect("failed to copy app lwtest_b.dll");

    let args = vec![
        OsString::from("run"),
        harness::case::os(&exe),
        OsString::from("--cwd"),
        harness::case::os(&app_dir),
    ];
    let result =
        harness::run_loadwhat::run_public(&paths, case.root(), &args, Duration::from_secs(20))
            .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 0);

    let lines = token_lines(&result.stdout);
    assert_eq!(
        lines.len(),
        1,
        "expected one summary line.\n{}",
        result.stdout
    );
    assert_eq!(lines[0], "SUCCESS status=0");
}

#[test]
fn run_trace_emits_detailed_lines() {
    let Some(paths) = harness::paths::from_env() else {
        return;
    };

    let case = harness::case::TestCase::new(&paths, "run_trace_missing")
        .expect("failed to initialize test case");
    let app_dir = case.mkdir("app").expect("failed to create app directory");
    let exe = case
        .copy_fixture(
            harness::fixture::HOST_STATIC_IMPORTS_MISSING_EXE,
            "app\\host_static_imports_missing.exe",
        )
        .expect("failed to copy host fixture");

    let args = vec![
        OsString::from("run"),
        harness::case::os(&exe),
        OsString::from("--cwd"),
        harness::case::os(&app_dir),
        OsString::from("--trace"),
    ];
    let result =
        harness::run_loadwhat::run_public(&paths, case.root(), &args, Duration::from_secs(20))
            .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 10);
    assert!(
        result.stdout.contains("SEARCH_ORDER")
            && result.stdout.contains("STATIC_MISSING")
            && result.stdout.contains("SEARCH_PATH"),
        "expected detailed trace lines.\n{}",
        result.stdout
    );
}
