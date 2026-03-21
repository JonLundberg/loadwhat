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

/// A target that loads only system DLLs and exits with code 0 should produce
/// a clean SUCCESS output with no false diagnoses.
#[test]
fn run_target_exits_zero_with_no_app_dlls_reports_success() {
    let paths = harness::paths::require_from_env();
    let case = harness::case::TestCase::new(&paths, "run_success_no_app_dlls")
        .expect("failed to initialize test case");
    case.mkdir("app").expect("failed to create app directory");
    let exe = case
        .copy_fixture(
            harness::fixture::HOST_ECHO_ARGV_CWD_EXE,
            "app\\host_echo_argv_cwd.exe",
        )
        .expect("failed to copy echo fixture");

    let args = vec![OsString::from("run"), harness::case::os(&exe)];
    let result =
        harness::run_loadwhat::run_public(&paths, case.root(), &args, Duration::from_secs(20))
            .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 0);
    assert_eq!(
        token_lines(&result.stdout),
        vec!["SUCCESS status=0"],
        "target with no app DLLs exiting 0 should report clean success.\n{}",
        result.stdout
    );
}

/// A target where every static dependency is present should produce SUCCESS.
#[test]
fn run_success_with_all_deps_present_emits_success() {
    let paths = harness::paths::require_from_env();
    let case = harness::case::TestCase::new(&paths, "run_success_all_deps")
        .expect("failed to initialize test case");
    let app_dir = case.mkdir("app").expect("failed to create app directory");
    let exe = case
        .copy_fixture(
            harness::fixture::HOST_STATIC_IMPORTS_A_EXE,
            "app\\host_static_imports_a.exe",
        )
        .expect("failed to copy host fixture");
    case.copy_fixture_as(harness::fixture::DLL_LWTEST_A, "app", "lwtest_a.dll")
        .expect("failed to copy lwtest_a.dll");
    case.copy_fixture_as(harness::fixture::DLL_LWTEST_B, "app", "lwtest_b.dll")
        .expect("failed to copy lwtest_b.dll");

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
    assert_eq!(
        token_lines(&result.stdout),
        vec!["SUCCESS status=0"],
        "target with all deps present should report clean success.\n{}",
        result.stdout
    );
}
