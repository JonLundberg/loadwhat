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
fn run_summary_emits_single_static_missing_line() {
    let paths = harness::paths::require_from_env();

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
        OsString::from("--cwd"),
        harness::case::os(&app_dir),
        harness::case::os(&exe),
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
        !lines[0].contains("via=") && !lines[0].contains("depth="),
        "direct missing should not include transitive fields.\n{}",
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
fn run_summary_emits_transitive_static_missing_with_via_and_depth() {
    let paths = harness::paths::require_from_env();

    let case = harness::case::TestCase::new(&paths, "run_summary_transitive_missing")
        .expect("failed to initialize test case");
    let app_dir = case.mkdir("app").expect("failed to create app directory");
    let exe = case
        .copy_fixture(
            harness::fixture::HOST_STATIC_A_DEPENDS_ON_B_EXE,
            "app\\host_static_a_depends_on_b.exe",
        )
        .expect("failed to copy host fixture");
    case.copy_fixture_as(harness::fixture::DLL_LWTEST_A, "app", "lwtest_a.dll")
        .expect("failed to copy lwtest_a.dll fixture");

    let args = vec![
        OsString::from("run"),
        OsString::from("--cwd"),
        harness::case::os(&app_dir),
        harness::case::os(&exe),
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
        lines[0].starts_with("STATIC_MISSING ")
            && lines[0].contains(r#"module="host_static_a_depends_on_b.exe""#)
            && lines[0].contains(r#"dll="lwtest_b.dll""#)
            && lines[0].contains(r#"via="lwtest_a.dll""#)
            && lines[0].contains("depth=2"),
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
    let paths = harness::paths::require_from_env();

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
        OsString::from("--cwd"),
        harness::case::os(&app_dir),
        harness::case::os(&exe),
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
    let paths = harness::paths::require_from_env();

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
        OsString::from("--cwd"),
        harness::case::os(&app_dir),
        OsString::from("--trace"),
        harness::case::os(&exe),
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

#[test]
fn imports_emits_static_only_tokens_and_summary() {
    let paths = harness::paths::require_from_env();

    let case = harness::case::TestCase::new(&paths, "imports_output_contract")
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
        OsString::from("imports"),
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
    assert!(
        lines.iter().any(|line| line.starts_with("STATIC_START "))
            && lines.iter().any(|line| line.starts_with("STATIC_IMPORT "))
            && lines.iter().any(|line| line.starts_with("SEARCH_ORDER "))
            && lines.iter().any(|line| line.starts_with("SUMMARY ")),
        "expected static/search imports output.\n{}",
        result.stdout
    );
    assert!(
        !lines.iter().any(|line| {
            line.starts_with("RUN_START ")
                || line.starts_with("RUNTIME_LOADED ")
                || line.starts_with("DEBUG_STRING ")
                || line.starts_with("RUN_END ")
                || line.starts_with("FIRST_BREAK ")
        }),
        "imports should remain offline/static-only.\n{}",
        result.stdout
    );
    let summary = lines
        .iter()
        .copied()
        .find(|line| line.starts_with("SUMMARY "))
        .expect("missing SUMMARY");
    assert!(
        summary.contains("first_break=false")
            && summary.contains("static_missing=0")
            && summary.contains("static_bad_image=0")
            && summary.contains("dynamic_missing=0")
            && summary.contains("runtime_loaded=0"),
        "unexpected imports SUMMARY schema.\n{}",
        summary
    );
}
