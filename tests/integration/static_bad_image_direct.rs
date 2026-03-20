use crate::harness;
use std::ffi::OsString;
use std::fs;
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

fn make_direct_bad_image_case(
    case_name: &str,
) -> (
    harness::paths::HarnessPaths,
    harness::case::TestCase,
    std::path::PathBuf,
    std::path::PathBuf,
) {
    let paths = harness::paths::require_from_env();
    let case =
        harness::case::TestCase::new(&paths, case_name).expect("failed to initialize test case");
    let app_dir = case.mkdir("app").expect("failed to create app directory");
    let exe = case
        .copy_fixture(
            harness::fixture::HOST_STATIC_IMPORTS_MISSING_EXE,
            "app\\host_static_imports_missing.exe",
        )
        .expect("failed to copy host fixture");
    let bad_dll = app_dir.join("lwtest_a.dll");
    fs::write(&bad_dll, b"this is not a PE image").expect("failed to create bad image fixture");
    (paths, case, app_dir, exe)
}

#[test]
fn run_summary_mode_reports_direct_static_bad_image() {
    let (paths, case, app_dir, exe) = make_direct_bad_image_case("run_direct_bad_image_summary");
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
        lines[0].starts_with("STATIC_BAD_IMAGE ")
            && lines[0].contains(r#"dll="lwtest_a.dll""#)
            && lines[0].contains(r#"reason="BAD_IMAGE""#),
        "unexpected summary output.\n{}",
        result.stdout
    );
    assert!(
        !result.stdout.contains("SEARCH_ORDER")
            && !result.stdout.contains("SEARCH_PATH")
            && !result.stdout.contains("NOTE ")
            && !result.stdout.contains("RUN_START")
            && !result.stdout.contains("DEBUG_STRING"),
        "summary mode should stay minimal.\n{}",
        result.stdout
    );
}

#[test]
fn run_trace_mode_reports_direct_static_bad_image_with_search_evidence() {
    let (paths, case, app_dir, exe) = make_direct_bad_image_case("run_direct_bad_image_trace");
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
    let lines = token_lines(&result.stdout);
    assert!(
        lines.iter().any(|line| line.starts_with("SEARCH_ORDER "))
            && lines
                .iter()
                .any(|line| line.starts_with("STATIC_BAD_IMAGE "))
            && lines.iter().any(|line| line.starts_with("SEARCH_PATH ")),
        "expected trace search evidence.\n{}",
        result.stdout
    );
    assert!(
        !lines.iter().any(|line| {
            line.starts_with("RUN_START ")
                || line.starts_with("RUNTIME_LOADED ")
                || line.starts_with("DEBUG_STRING ")
                || line.starts_with("RUN_END ")
        }),
        "trace mode without -v should not emit verbose runtime lines.\n{}",
        result.stdout
    );
}

#[test]
fn run_verbose_mode_reports_direct_static_bad_image_with_summary() {
    let (paths, case, app_dir, exe) = make_direct_bad_image_case("run_direct_bad_image_verbose");
    let args = vec![
        OsString::from("run"),
        OsString::from("--cwd"),
        harness::case::os(&app_dir),
        OsString::from("-v"),
        harness::case::os(&exe),
    ];
    let result =
        harness::run_loadwhat::run_public(&paths, case.root(), &args, Duration::from_secs(20))
            .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 10);
    let lines = token_lines(&result.stdout);
    let summary = lines
        .iter()
        .copied()
        .find(|line| line.starts_with("SUMMARY "))
        .expect("missing SUMMARY");
    assert!(
        lines.iter().any(|line| line.starts_with("RUN_START "))
            && lines.iter().any(|line| line.starts_with("RUNTIME_LOADED "))
            && lines.iter().any(|line| line.starts_with("RUN_END "))
            && lines.iter().any(|line| line.starts_with("FIRST_BREAK ")),
        "expected verbose runtime and first-break detail.\n{}",
        result.stdout
    );
    assert!(
        summary.contains("first_break=true")
            && summary.contains("static_bad_image=1")
            && summary.contains("dynamic_missing=0"),
        "unexpected SUMMARY line.\n{}",
        summary
    );
    assert!(
        !result.stdout.contains("DYNAMIC_MISSING"),
        "direct static bad image should prevent dynamic diagnosis.\n{}",
        result.stdout
    );
}
