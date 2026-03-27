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

/// When Phase B finds a static issue, the SUMMARY line should report
/// dynamic_missing=0, and no DYNAMIC_MISSING token should appear, even
/// if loader-snaps captured dynamic failures.
#[test]
fn verbose_mode_static_finding_suppresses_dynamic_in_summary() {
    let paths = harness::paths::require_from_env();
    let case = harness::case::TestCase::new(&paths, "verbose_static_suppresses_dynamic")
        .expect("failed to initialize test case");
    let app_dir = case.mkdir("app").expect("failed to create app directory");
    let exe = case
        .copy_fixture(
            harness::fixture::HOST_STATIC_IMPORTS_MISSING_EXE,
            "app\\host_static_imports_missing.exe",
        )
        .expect("failed to copy host fixture");
    // Do NOT copy lwtest_a.dll — Phase B will find it missing

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
    assert!(
        lines
            .iter()
            .any(|line| line.starts_with("STATIC_MISSING ")
                && line.contains(r#"dll="lwtest_a.dll""#)),
        "Phase B should report lwtest_a.dll as missing.\n{}",
        result.stdout
    );
    let summary = lines
        .iter()
        .copied()
        .find(|line| line.starts_with("SUMMARY "))
        .expect("expected SUMMARY line in verbose output");
    assert!(
        summary.contains("dynamic_missing=0"),
        "SUMMARY should report dynamic_missing=0 when static issue found.\n{}",
        summary
    );
    assert!(
        !lines
            .iter()
            .any(|line| line.starts_with("DYNAMIC_MISSING ")),
        "no DYNAMIC_MISSING should appear when static issue takes precedence.\n{}",
        result.stdout
    );
}
