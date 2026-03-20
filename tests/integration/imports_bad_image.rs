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
                    || line.starts_with("SEARCH_")
                    || line.starts_with("SUMMARY")
                    || line.starts_with("NOTE ")
                    || line.starts_with("RUN_")
                    || line.starts_with("RUNTIME_")
                    || line.starts_with("DEBUG_STRING")
                    || line.starts_with("DYNAMIC_")
                    || line.starts_with("FIRST_BREAK"))
        })
        .collect()
}

#[test]
fn imports_reports_direct_bad_image_without_runtime_tokens() {
    let paths = harness::paths::require_from_env();
    let case = harness::case::TestCase::new(&paths, "imports_direct_bad_image")
        .expect("failed to initialize test case");
    let app_dir = case.mkdir("app").expect("failed to create app directory");
    let exe = case
        .copy_fixture(
            harness::fixture::HOST_STATIC_IMPORTS_MISSING_EXE,
            "app\\host_static_imports_missing.exe",
        )
        .expect("failed to copy host fixture");
    fs::write(app_dir.join("lwtest_a.dll"), b"not a pe image").expect("failed to create bad image");

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
    harness::assert::assert_exit_code(&result, 10);
    let lines = token_lines(&result.stdout);
    let summary = lines
        .iter()
        .copied()
        .find(|line| line.starts_with("SUMMARY "))
        .expect("missing SUMMARY");
    assert!(
        lines.iter().any(|line| line.starts_with("SEARCH_ORDER "))
            && lines.iter().any(|line| {
                line.starts_with("STATIC_BAD_IMAGE ")
                    && line.contains(r#"dll="lwtest_a.dll""#)
                    && line.contains(r#"reason="BAD_IMAGE""#)
            }),
        "expected direct bad-image imports output.\n{}",
        result.stdout
    );
    assert!(
        summary.contains("first_break=false") && summary.contains("static_bad_image=1"),
        "unexpected SUMMARY line.\n{}",
        summary
    );
    assert!(
        !lines.iter().any(|line| {
            line.starts_with("RUN_START ")
                || line.starts_with("RUNTIME_LOADED ")
                || line.starts_with("DEBUG_STRING ")
                || line.starts_with("RUN_END ")
                || line.starts_with("FIRST_BREAK ")
                || line.starts_with("DYNAMIC_")
        }),
        "imports must remain static-only.\n{}",
        result.stdout
    );
}

#[test]
fn imports_reports_transitive_bad_image_deterministically() {
    let paths = harness::paths::require_from_env();
    let case = harness::case::TestCase::new(&paths, "imports_transitive_bad_image")
        .expect("failed to initialize test case");
    let app_dir = case.mkdir("app").expect("failed to create app directory");
    let exe = case
        .copy_fixture(
            harness::fixture::HOST_STATIC_A_DEPENDS_ON_B_EXE,
            "app\\host_static_a_depends_on_b.exe",
        )
        .expect("failed to copy host fixture");
    case.copy_fixture_as(harness::fixture::DLL_LWTEST_A, "app", "lwtest_a.dll")
        .expect("failed to copy lwtest_a.dll");
    fs::write(app_dir.join("lwtest_b.dll"), b"still not a pe image")
        .expect("failed to create bad transitive image");

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
    harness::assert::assert_exit_code(&result, 10);
    assert!(
        result.stdout.contains(
            r#"STATIC_BAD_IMAGE module="lwtest_a.dll" dll="lwtest_b.dll" reason="BAD_IMAGE""#
        ),
        "expected transitive bad-image diagnosis.\n{}",
        result.stdout
    );
}
