use crate::harness;
use std::ffi::OsString;
use std::time::Duration;

#[test]
fn dynamic_loadlibrary_fullpath_reports_transitive_missing_dll() {
    let paths = harness::paths::require_from_env();

    let case = harness::case::TestCase::new(&paths, "dynamic_loadlibrary_fullpath")
        .expect("failed to initialize test case");
    let app_dir = case.mkdir("app").expect("failed to create app directory");
    case.mkdir("dll").expect("failed to create dll directory");

    let exe = case
        .copy_fixture(
            harness::fixture::HOST_DYNAMIC_LOADLIBRARY_FULLPATH_EXE,
            "app\\host_dynamic_loadlibrary_fullpath.exe",
        )
        .expect("failed to copy host fixture");
    let dll_path = case
        .copy_fixture_as(harness::fixture::DLL_LWTEST_A_V1, "dll", "lwtest_a.dll")
        .expect("failed to copy fullpath lwtest_a.dll");

    let args = vec![
        OsString::from("run"),
        OsString::from("--cwd"),
        harness::case::os(&app_dir),
        harness::case::os(&exe),
        harness::case::os(&dll_path),
    ];
    let result = harness::run_loadwhat::run(&paths, case.root(), &args, Duration::from_secs(20))
        .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 2);
    harness::assert::assert_missing_dll(&result.stdout, "lwtest_b.dll");
    harness::assert::assert_target_exit_code(&result.stdout, 10);
}

#[test]
fn dynamic_loadlibrary_fullpath_loads_requested_root_but_resolves_deps_by_name() {
    let paths = harness::paths::require_from_env();

    let case = harness::case::TestCase::new(&paths, "dynamic_loadlibrary_fullpath_positive")
        .expect("failed to initialize test case");
    let app_dir = case.mkdir("app").expect("failed to create app directory");
    case.mkdir("dll").expect("failed to create dll directory");

    let exe = case
        .copy_fixture(
            harness::fixture::HOST_DYNAMIC_LOADLIBRARY_FULLPATH_EXE,
            "app\\host_dynamic_loadlibrary_fullpath.exe",
        )
        .expect("failed to copy host fixture");
    let dll_path = case
        .copy_fixture_as(harness::fixture::DLL_LWTEST_A_V1, "dll", "lwtest_a.dll")
        .expect("failed to copy requested lwtest_a.dll");
    case.copy_fixture_as(harness::fixture::DLL_LWTEST_A_V2, "app", "lwtest_a.dll")
        .expect("failed to copy decoy lwtest_a.dll");
    let app_lwtest_b = case
        .copy_fixture_as(harness::fixture::DLL_LWTEST_B, "app", "lwtest_b.dll")
        .expect("failed to copy app-local lwtest_b.dll");

    let args = vec![
        OsString::from("run"),
        OsString::from("--cwd"),
        harness::case::os(&app_dir),
        harness::case::os(&exe),
        harness::case::os(&dll_path),
    ];
    let result = harness::run_loadwhat::run(&paths, case.root(), &args, Duration::from_secs(20))
        .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 0);
    harness::assert::assert_no_missing_result(&result.stdout);
    harness::assert::assert_target_exit_code(&result.stdout, 0);
    harness::assert::assert_loaded_path(&result.stdout, "lwtest_a.dll", &dll_path);
    harness::assert::assert_loaded_path(&result.stdout, "lwtest_b.dll", &app_lwtest_b);
}
