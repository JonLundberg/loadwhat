use crate::harness;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

fn path_env_value(extra_dirs: &[&Path]) -> String {
    let mut dirs: Vec<PathBuf> = extra_dirs.iter().map(|dir| dir.to_path_buf()).collect();
    if let Some(existing) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&existing));
    }

    std::env::join_paths(dirs)
        .expect("failed to join PATH entries")
        .to_string_lossy()
        .into_owned()
}

#[test]
fn dynamic_name_uses_path_when_app_and_cwd_do_not_contain_dll() {
    let Some(paths) = harness::paths::from_env() else {
        return;
    };

    let case = harness::case::TestCase::new(&paths, "path_search_fallback")
        .expect("failed to initialize test case");
    case.mkdir("app").expect("failed to create app directory");
    let cwd_dir = case.mkdir("cwd").expect("failed to create cwd directory");
    let path1_dir = case
        .mkdir("path1")
        .expect("failed to create path1 directory");

    let exe = case
        .copy_fixture(
            harness::fixture::HOST_DYNAMIC_LOADLIBRARY_NAME_EXE,
            "app\\host_dynamic_loadlibrary_name.exe",
        )
        .expect("failed to copy host fixture");
    let path1_lwtest_a = case
        .copy_fixture_as(harness::fixture::DLL_LWTEST_A_V1, "path1", "lwtest_a.dll")
        .expect("failed to copy PATH lwtest_a.dll");
    case.copy_fixture_as(harness::fixture::DLL_LWTEST_B, "app", "lwtest_b.dll")
        .expect("failed to copy app-local lwtest_b.dll");

    let path_env = path_env_value(&[path1_dir.as_path()]);
    let args = vec![
        OsString::from("run"),
        OsString::from("--cwd"),
        harness::case::os(&cwd_dir),
        harness::case::os(&exe),
    ];
    let result = harness::run_loadwhat::run_with_env(
        &paths,
        case.root(),
        &args,
        Duration::from_secs(20),
        &[("PATH", path_env.as_str())],
    )
    .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 0);
    harness::assert::assert_no_missing_result(&result.stdout);
    harness::assert::assert_target_exit_code(&result.stdout, 0);
    harness::assert::assert_loaded_path(&result.stdout, "lwtest_a.dll", &path1_lwtest_a);
}

#[test]
fn dynamic_name_prefers_app_directory_over_path() {
    let Some(paths) = harness::paths::from_env() else {
        return;
    };

    let case = harness::case::TestCase::new(&paths, "path_search_app_over_path")
        .expect("failed to initialize test case");
    case.mkdir("app").expect("failed to create app directory");
    let cwd_dir = case.mkdir("cwd").expect("failed to create cwd directory");
    let path1_dir = case
        .mkdir("path1")
        .expect("failed to create path1 directory");

    let exe = case
        .copy_fixture(
            harness::fixture::HOST_DYNAMIC_LOADLIBRARY_NAME_EXE,
            "app\\host_dynamic_loadlibrary_name.exe",
        )
        .expect("failed to copy host fixture");
    let app_lwtest_a = case
        .copy_fixture_as(harness::fixture::DLL_LWTEST_A_V2, "app", "lwtest_a.dll")
        .expect("failed to copy app lwtest_a.dll");
    case.copy_fixture_as(harness::fixture::DLL_LWTEST_B, "app", "lwtest_b.dll")
        .expect("failed to copy app-local lwtest_b.dll");
    case.copy_fixture_as(harness::fixture::DLL_LWTEST_A_V1, "path1", "lwtest_a.dll")
        .expect("failed to copy PATH lwtest_a.dll");

    let path_env = path_env_value(&[path1_dir.as_path()]);
    let args = vec![
        OsString::from("run"),
        OsString::from("--cwd"),
        harness::case::os(&cwd_dir),
        harness::case::os(&exe),
    ];
    let result = harness::run_loadwhat::run_with_env(
        &paths,
        case.root(),
        &args,
        Duration::from_secs(20),
        &[("PATH", path_env.as_str())],
    )
    .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 0);
    harness::assert::assert_no_missing_result(&result.stdout);
    harness::assert::assert_target_exit_code(&result.stdout, 0);
    harness::assert::assert_loaded_path(&result.stdout, "lwtest_a.dll", &app_lwtest_a);
}

#[test]
fn dynamic_name_uses_path_entry_order() {
    let Some(paths) = harness::paths::from_env() else {
        return;
    };

    let case = harness::case::TestCase::new(&paths, "path_search_path_order")
        .expect("failed to initialize test case");
    case.mkdir("app").expect("failed to create app directory");
    let cwd_dir = case.mkdir("cwd").expect("failed to create cwd directory");
    let path1_dir = case
        .mkdir("path1")
        .expect("failed to create path1 directory");
    let path2_dir = case
        .mkdir("path2")
        .expect("failed to create path2 directory");

    let exe = case
        .copy_fixture(
            harness::fixture::HOST_DYNAMIC_LOADLIBRARY_NAME_EXE,
            "app\\host_dynamic_loadlibrary_name.exe",
        )
        .expect("failed to copy host fixture");
    let path1_lwtest_a = case
        .copy_fixture_as(harness::fixture::DLL_LWTEST_A_V1, "path1", "lwtest_a.dll")
        .expect("failed to copy PATH1 lwtest_a.dll");
    case.copy_fixture_as(harness::fixture::DLL_LWTEST_A_V2, "path2", "lwtest_a.dll")
        .expect("failed to copy PATH2 lwtest_a.dll");
    case.copy_fixture_as(harness::fixture::DLL_LWTEST_B, "app", "lwtest_b.dll")
        .expect("failed to copy app-local lwtest_b.dll");

    let path_env = path_env_value(&[path1_dir.as_path(), path2_dir.as_path()]);
    let args = vec![
        OsString::from("run"),
        OsString::from("--cwd"),
        harness::case::os(&cwd_dir),
        harness::case::os(&exe),
    ];
    let result = harness::run_loadwhat::run_with_env(
        &paths,
        case.root(),
        &args,
        Duration::from_secs(20),
        &[("PATH", path_env.as_str())],
    )
    .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 0);
    harness::assert::assert_no_missing_result(&result.stdout);
    harness::assert::assert_target_exit_code(&result.stdout, 0);
    harness::assert::assert_loaded_path(&result.stdout, "lwtest_a.dll", &path1_lwtest_a);
}
