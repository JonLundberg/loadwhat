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

#[test]
fn run_reports_transitive_static_bad_image_deterministically() {
    let paths = harness::paths::require_from_env();
    let case = harness::case::TestCase::new(&paths, "run_transitive_bad_image")
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
    fs::write(app_dir.join("lwtest_b.dll"), b"not a valid PE")
        .expect("failed to create bad lwtest_b.dll");

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
            && lines[0].contains(r#"module="host_static_a_depends_on_b.exe""#)
            && lines[0].contains(r#"dll="lwtest_b.dll""#)
            && lines[0].contains(r#"reason="BAD_IMAGE""#),
        "unexpected transitive bad-image summary.\n{}",
        result.stdout
    );
}
