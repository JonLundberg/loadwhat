use crate::harness;
use std::ffi::OsString;
use std::time::Duration;

#[test]
fn dynamic_nested_loadlibrary_success() {
    let Some(paths) = harness::paths::from_env() else {
        return;
    };

    let case = harness::case::TestCase::new(&paths, "dynamic_nested_loadlibrary_success")
        .expect("failed to initialize test case");
    case.mkdir("app").expect("failed to create app directory");
    let cwd_dir = case.mkdir("cwd").expect("failed to create cwd directory");

    let exe = case
        .copy_fixture(
            harness::fixture::HOST_DYNAMIC_LOADLIBRARY_NESTED_EXE,
            "app\\host_dynamic_loadlibrary_nested.exe",
        )
        .expect("failed to copy host fixture");
    let cwd_lwtest_a = case
        .copy_fixture_as(harness::fixture::DLL_LWTEST_A_NESTED, "cwd", "lwtest_a.dll")
        .expect("failed to copy nested lwtest_a.dll");
    let cwd_lwtest_b = case
        .copy_fixture_as(harness::fixture::DLL_LWTEST_B, "cwd", "lwtest_b.dll")
        .expect("failed to copy nested lwtest_b.dll");

    let args = vec![
        OsString::from("run"),
        harness::case::os(&exe),
        OsString::from("--cwd"),
        harness::case::os(&cwd_dir),
        OsString::from("--loader-snaps"),
    ];
    let result = harness::run_loadwhat::run(&paths, case.root(), &args, Duration::from_secs(20))
        .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 0);
    harness::assert::assert_no_missing_result(&result.stdout);
    harness::assert::assert_target_exit_code(&result.stdout, 0);
    harness::assert::assert_loaded_path(&result.stdout, "lwtest_a.dll", &cwd_lwtest_a);
    harness::assert::assert_loaded_path(&result.stdout, "lwtest_b.dll", &cwd_lwtest_b);
}

#[test]
fn dynamic_nested_loadlibrary_reports_missing_b() {
    let Some(paths) = harness::paths::from_env() else {
        return;
    };

    let case = harness::case::TestCase::new(&paths, "dynamic_nested_loadlibrary_missing_b")
        .expect("failed to initialize test case");
    case.mkdir("app").expect("failed to create app directory");
    let cwd_dir = case.mkdir("cwd").expect("failed to create cwd directory");

    let exe = case
        .copy_fixture(
            harness::fixture::HOST_DYNAMIC_LOADLIBRARY_NESTED_EXE,
            "app\\host_dynamic_loadlibrary_nested.exe",
        )
        .expect("failed to copy host fixture");
    let cwd_lwtest_a = case
        .copy_fixture_as(harness::fixture::DLL_LWTEST_A_NESTED, "cwd", "lwtest_a.dll")
        .expect("failed to copy nested lwtest_a.dll");

    let args = vec![
        OsString::from("run"),
        harness::case::os(&exe),
        OsString::from("--cwd"),
        harness::case::os(&cwd_dir),
        OsString::from("--loader-snaps"),
    ];
    let result = harness::run_loadwhat::run(&paths, case.root(), &args, Duration::from_secs(20))
        .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 2);
    harness::assert::assert_missing_dll(&result.stdout, "lwtest_b.dll");
    harness::assert::assert_target_exit_code(&result.stdout, 10);
    harness::assert::assert_loaded_path(&result.stdout, "lwtest_a.dll", &cwd_lwtest_a);
}
