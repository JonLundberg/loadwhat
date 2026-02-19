use crate::harness;
use std::ffi::OsString;
use std::time::Duration;

#[test]
fn static_missing_direct() {
    let Some(paths) = harness::paths::from_env() else {
        return;
    };

    let case = harness::case::TestCase::new(&paths, "static_missing_direct")
        .expect("failed to initialize test case");
    let app_dir = case.mkdir("app").expect("failed to create app directory");
    let exe = case
        .copy_fixture(
            harness::fixture::HOST_STATIC_IMPORTS_MISSING_EXE,
            "app\\host_static_imports_missing.exe",
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
}
