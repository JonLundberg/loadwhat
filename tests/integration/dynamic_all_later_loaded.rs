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

/// When a DLL fails to load initially but is later loaded successfully from a
/// different path, Phase C should discard the failure candidate and report no
/// DYNAMIC_MISSING.
#[test]
fn dynamic_all_candidates_later_loaded_emits_no_dynamic_missing() {
    let paths = harness::paths::require_from_env();
    let case = harness::case::TestCase::new(&paths, "dynamic_all_later_loaded")
        .expect("failed to initialize test case");

    let app_dir = case.mkdir("app").expect("failed to create app directory");
    case.mkdir("good").expect("failed to create good directory");

    let exe = case
        .copy_fixture(
            harness::fixture::HOST_DYNAMIC_LOADLIBRARY_SEQUENCE_EXE,
            "app\\host_dynamic_loadlibrary_sequence.exe",
        )
        .expect("failed to copy sequence host");
    let fullpath_dll = case
        .copy_fixture_as(
            harness::fixture::DLL_LWTEST_A_V1,
            "good",
            "lwtest_probe.dll",
        )
        .expect("failed to copy probe dll");
    // Provide lwtest_b.dll in app dir for transitive deps
    case.copy_fixture_as(harness::fixture::DLL_LWTEST_B, "app", "lwtest_b.dll")
        .expect("failed to copy app-local dependency");

    // First arg: try to load by name (will fail, not in app dir)
    // Second arg: load by full path (will succeed)
    let args = vec![
        OsString::from("run"),
        OsString::from("--cwd"),
        harness::case::os(&app_dir),
        harness::case::os(&exe),
        OsString::from("optional:lwtest_probe.dll"),
        harness::case::os(&fullpath_dll),
    ];
    let result =
        harness::run_loadwhat::run_public(&paths, case.root(), &args, Duration::from_secs(20))
            .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 0);
    assert_eq!(
        token_lines(&result.stdout),
        vec!["SUCCESS status=0"],
        "all-later-loaded candidates should produce no DYNAMIC_MISSING.\n{}",
        result.stdout
    );
}
