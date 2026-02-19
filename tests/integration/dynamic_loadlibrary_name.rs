use crate::harness;
use std::ffi::OsString;
use std::time::Duration;

#[test]
fn dynamic_loadlibrary_name_uses_target_cwd_search() {
    let Some(paths) = harness::paths::from_env() else {
        return;
    };

    let case = harness::case::TestCase::new(&paths, "dynamic_loadlibrary_name")
        .expect("failed to initialize test case");
    case.mkdir("app").expect("failed to create app directory");
    let cwd_dir = case.mkdir("cwd").expect("failed to create cwd directory");

    let exe = case
        .copy_fixture(
            harness::fixture::HOST_DYNAMIC_LOADLIBRARY_NAME_EXE,
            "app\\host_dynamic_loadlibrary_name.exe",
        )
        .expect("failed to copy host fixture");

    let cwd_lwtest_a = case
        .copy_fixture_as(harness::fixture::DLL_LWTEST_A_V1, "cwd", "lwtest_a.dll")
        .expect("failed to copy cwd lwtest_a.dll");
    case.copy_fixture_as(harness::fixture::DLL_LWTEST_B, "cwd", "lwtest_b.dll")
        .expect("failed to copy cwd lwtest_b.dll");

    let args = vec![
        OsString::from("run"),
        harness::case::os(&exe),
        OsString::from("--cwd"),
        harness::case::os(&cwd_dir),
    ];
    let result = harness::run_loadwhat::run(&paths, case.root(), &args, Duration::from_secs(20))
        .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 0);
    harness::assert::assert_no_missing_result(&result.stdout);
    harness::assert::assert_target_exit_code(&result.stdout, 0);
    harness::assert::assert_loaded_path(&result.stdout, "lwtest_a.dll", &cwd_lwtest_a);
}
