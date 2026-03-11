use crate::harness;
use std::ffi::OsString;
use std::time::Duration;

#[test]
fn dynamic_multiple_candidates_discard_optional_failure_after_later_success() {
    let Some(paths) = harness::paths::from_env() else {
        return;
    };

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
        OsString::from("--loader-snaps"),
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
