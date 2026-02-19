use crate::harness;
use std::ffi::OsString;
use std::time::Duration;

#[test]
fn static_missing_transitive() {
    let Some(paths) = harness::paths::from_env() else {
        return;
    };

    let case = harness::case::TestCase::new(&paths, "static_missing_transitive")
        .expect("failed to initialize test case");
    let app_dir = case.mkdir("app").expect("failed to create app directory");
    let exe = case
        .copy_fixture(
            harness::fixture::HOST_STATIC_A_DEPENDS_ON_B_EXE,
            "app\\host_static_a_depends_on_b.exe",
        )
        .expect("failed to copy host fixture");
    case.copy_fixture_as(harness::fixture::DLL_LWTEST_A, "app", "lwtest_a.dll")
        .expect("failed to copy lwtest_a.dll fixture");

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
    harness::assert::assert_missing_dll(&result.stdout, "lwtest_b.dll");
}
