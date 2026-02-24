use crate::harness;
use std::ffi::OsString;
use std::time::Duration;

#[test]
fn dynamic_missing_direct_reports_lwtest_a() {
    let Some(paths) = harness::paths::from_env() else {
        return;
    };

    let case = harness::case::TestCase::new(&paths, "dynamic_missing_direct")
        .expect("failed to initialize test case");
    let app_dir = case.mkdir("app").expect("failed to create app directory");
    let exe = case
        .copy_fixture(
            harness::fixture::HOST_DYNAMIC_LOADLIBRARY_NAME_EXE,
            "app\\host_dynamic_loadlibrary_name.exe",
        )
        .expect("failed to copy host fixture");

    let args = vec![
        OsString::from("run"),
        harness::case::os(&exe),
        OsString::from("--cwd"),
        harness::case::os(&app_dir),
        OsString::from("--loader-snaps"),
    ];
    let result = harness::run_loadwhat::run(&paths, case.root(), &args, Duration::from_secs(20))
        .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 2);
    harness::assert::assert_missing_dll(&result.stdout, "lwtest_a.dll");
    harness::assert::assert_target_exit_code(&result.stdout, 10);
}

