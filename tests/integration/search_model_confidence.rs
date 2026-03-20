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

fn quoted_path(path: &Path) -> String {
    format!(
        r#"path="{}""#,
        path.display().to_string().replace('\\', r"\\")
    )
}

#[test]
fn imports_uses_cwd_immediately_after_app_dir_when_safe_dll_search_is_disabled() {
    let paths = harness::paths::require_from_env();
    let case = harness::case::TestCase::new(&paths, "search_safe_dll_disabled")
        .expect("failed to initialize test case");
    case.mkdir("app").expect("failed to create app directory");
    let cwd_dir = case.mkdir("cwd").expect("failed to create cwd directory");
    let path_dir = case.mkdir("path").expect("failed to create path directory");

    let exe = case
        .copy_fixture(
            harness::fixture::HOST_STATIC_IMPORTS_MISSING_EXE,
            "app\\host_static_imports_missing.exe",
        )
        .expect("failed to copy host fixture");
    case.copy_fixture_as(harness::fixture::DLL_LWTEST_A, "cwd", "lwtest_a.dll")
        .expect("failed to copy cwd lwtest_a.dll");
    case.copy_fixture_as(harness::fixture::DLL_LWTEST_B, "cwd", "lwtest_b.dll")
        .expect("failed to copy cwd lwtest_b.dll");
    case.copy_fixture_as(harness::fixture::DLL_LWTEST_A_V2, "path", "lwtest_a.dll")
        .expect("failed to copy path lwtest_a.dll");
    let path_env = path_env_value(&[path_dir.as_path()]);

    let args = vec![
        OsString::from("imports"),
        harness::case::os(&exe),
        OsString::from("--cwd"),
        harness::case::os(&cwd_dir),
    ];
    let result = harness::run_loadwhat::run_public_with_env(
        &paths,
        case.root(),
        &args,
        Duration::from_secs(20),
        &[
            ("LOADWHAT_TEST_SAFE_DLL_SEARCH_MODE", "0"),
            ("PATH", path_env.as_str()),
        ],
    )
    .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 0);
    assert!(
        result.stdout.contains("SEARCH_ORDER safedll=0"),
        "expected safedll=0 search order.\n{}",
        result.stdout
    );
    assert!(
        result.stdout.contains(&format!(
            r#"SEARCH_PATH dll="lwtest_a.dll" order=1 {} result="MISS""#,
            quoted_path(&case.root().join("app").join("lwtest_a.dll"))
        )),
        "expected app-dir miss to remain first.\n{}",
        result.stdout
    );
    assert!(
        result.stdout.contains(&format!(
            r#"SEARCH_PATH dll="lwtest_a.dll" order=2 {} result="HIT""#,
            quoted_path(&cwd_dir.join("lwtest_a.dll"))
        )),
        "expected cwd hit immediately after app dir when SafeDllSearchMode is disabled.\n{}",
        result.stdout
    );
}

#[test]
fn fullpath_root_dependencies_still_use_basename_search() {
    let paths = harness::paths::require_from_env();
    let case = harness::case::TestCase::new(&paths, "search_fullpath_dependency_basename")
        .expect("failed to initialize test case");
    let app_dir = case.mkdir("app").expect("failed to create app directory");
    let dll_dir = case.mkdir("dll").expect("failed to create dll directory");
    let exe = case
        .copy_fixture(
            harness::fixture::HOST_DYNAMIC_LOADLIBRARY_FULLPATH_EXE,
            "app\\host_dynamic_loadlibrary_fullpath.exe",
        )
        .expect("failed to copy host fixture");
    let dll_path = case
        .copy_fixture_as(harness::fixture::DLL_LWTEST_A_V1, "dll", "lwtest_a.dll")
        .expect("failed to copy fullpath root dll");

    let args = vec![
        OsString::from("run"),
        OsString::from("--cwd"),
        harness::case::os(&app_dir),
        OsString::from("--trace"),
        harness::case::os(&exe),
        harness::case::os(&dll_path),
    ];
    let result =
        harness::run_loadwhat::run_public(&paths, case.root(), &args, Duration::from_secs(20))
            .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 10);
    assert!(
        result
            .stdout
            .contains(r#"DYNAMIC_MISSING dll="lwtest_b.dll""#),
        "expected transitive missing dependency from fullpath root.\n{}",
        result.stdout
    );
    assert!(
        result.stdout.contains(&format!(
            r#"SEARCH_PATH dll="lwtest_b.dll" order=1 {} result="MISS""#,
            quoted_path(&app_dir.join("lwtest_b.dll"))
        )),
        "dependent DLLs of an absolute-path root should use basename app-dir search.\n{}",
        result.stdout
    );
    assert!(
        !result
            .stdout
            .contains(&quoted_path(&dll_dir.join("lwtest_b.dll"))),
        "basename dependency search should not evaluate the root DLL directory as an altered search path.\n{}",
        result.stdout
    );
}
