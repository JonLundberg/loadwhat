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
fn dynamic_multiple_candidates_discard_optional_failure_after_later_success() {
    let paths = harness::paths::require_from_env();

    let case = harness::case::TestCase::new(&paths, "dynamic_multiple_candidates")
        .expect("failed to initialize test case");
    let app_dir = case.mkdir("app").expect("failed to create app directory");
    case.mkdir("good").expect("failed to create good directory");

    let exe = case
        .copy_fixture(
            harness::fixture::HOST_DYNAMIC_LOADLIBRARY_SEQUENCE_EXE,
            "app\\host_dynamic_loadlibrary_sequence.exe",
        )
        .expect("failed to copy host fixture");
    let optional_good = case
        .copy_fixture_as(
            harness::fixture::DLL_LWTEST_A_V1,
            "good",
            "lwtest_optional.dll",
        )
        .expect("failed to copy optional dll");
    case.copy_fixture_as(harness::fixture::DLL_LWTEST_B, "app", "lwtest_b.dll")
        .expect("failed to copy app-local lwtest_b.dll");

    let args = vec![
        OsString::from("run"),
        OsString::from("--cwd"),
        harness::case::os(&app_dir),
        harness::case::os(&exe),
        OsString::from("lwtest_optional.dll"),
        harness::case::os(&optional_good),
        OsString::from("lwtest_required.dll"),
    ];
    let result = harness::run_loadwhat::run(&paths, case.root(), &args, Duration::from_secs(20))
        .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 2);
    harness::assert::assert_missing_dll(&result.stdout, "lwtest_required.dll");
    harness::assert::assert_target_exit_code(&result.stdout, 10);
    harness::assert::assert_loaded_path(&result.stdout, "lwtest_optional.dll", &optional_good);
}

#[test]
fn earlier_successful_helper_probe_does_not_beat_real_missing_candidate() {
    let paths = harness::paths::require_from_env();
    let case =
        harness::case::TestCase::new(&paths, "dynamic_multiple_candidates_helper_reconciled")
            .expect("failed to initialize test case");
    let app_dir = case.mkdir("app").expect("failed to create app directory");
    case.mkdir("good").expect("failed to create good directory");
    let bad_dir = case.mkdir("bad").expect("failed to create bad directory");

    let exe = case
        .copy_fixture(
            harness::fixture::HOST_DYNAMIC_LOADLIBRARY_SEQUENCE_EXE,
            "app\\host_dynamic_loadlibrary_sequence.exe",
        )
        .expect("failed to copy host fixture");
    let helper_good = case
        .copy_fixture_as(
            harness::fixture::DLL_LWTEST_A_V1,
            "good",
            "lwtest_resolved.dll",
        )
        .expect("failed to copy resolved helper dll");
    case.copy_fixture_as(harness::fixture::DLL_LWTEST_B, "app", "lwtest_b.dll")
        .expect("failed to copy app-local lwtest_b.dll");

    let bad_helper_probe = bad_dir.join("lwtest_resolved.dll");
    let args = vec![
        OsString::from("run"),
        OsString::from("--cwd"),
        harness::case::os(&app_dir),
        harness::case::os(&exe),
        harness::case::os(&helper_good),
        OsString::from(format!("optional:{}", bad_helper_probe.display())),
        OsString::from("lwtest_required.dll"),
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
        "expected single summary line.\n{}",
        result.stdout
    );
    assert!(
        lines[0].starts_with("DYNAMIC_MISSING ")
            && lines[0].contains(r#"dll="lwtest_required.dll""#)
            && !lines[0].contains("lwtest_resolved.dll"),
        "transient full-path helper probe should not win final diagnosis.\n{}",
        result.stdout
    );
}
