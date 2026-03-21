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

/// A target that exits with a non-loader error code should not produce false DLL
/// diagnoses. This complements the existing `unrelated_non_loader_failure_does_not_invent_dll_diagnoses`
/// test by using a larger exit code that could be confused with a Windows status code.
#[test]
fn run_non_loader_high_exit_code_does_not_diagnose_dll() {
    let paths = harness::paths::require_from_env();
    let case = harness::case::TestCase::new(&paths, "post_init_high_exit_code")
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
        harness::case::os(&exe),
        OsString::from("--lwtest-exit-code"),
        OsString::from("42"),
    ];
    let result =
        harness::run_loadwhat::run_public(&paths, case.root(), &args, Duration::from_secs(20))
            .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 21);
    assert!(
        !token_lines(&result.stdout)
            .iter()
            .any(|line| { line.starts_with("STATIC_") || line.starts_with("DYNAMIC_") }),
        "non-loader exit code should not produce false DLL diagnoses.\n{}",
        result.stdout
    );
}
