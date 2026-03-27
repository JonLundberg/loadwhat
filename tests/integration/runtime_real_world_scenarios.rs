use crate::harness;
use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

fn path_env_value(extra_dirs: &[&Path]) -> String {
    let mut dirs: Vec<PathBuf> = extra_dirs.iter().map(|dir| dir.to_path_buf()).collect();
    if let Some(existing) = env::var_os("PATH") {
        dirs.extend(env::split_paths(&existing));
    }

    env::join_paths(dirs)
        .expect("failed to join PATH entries")
        .to_string_lossy()
        .into_owned()
}

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

fn system32_path(file_name: &str) -> PathBuf {
    let windir = env::var_os("WINDIR").expect("WINDIR should be set on Windows");
    PathBuf::from(windir).join("System32").join(file_name)
}

fn quoted_field_value<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!(r#"{key}=""#);
    let start = line.find(&needle)? + needle.len();
    let rest = &line[start..];
    let end = rest.find('"')?;
    Some(&rest[..end])
}

fn contains_normalized_path_token(
    stdout: &str,
    prefix: &str,
    dll_name: &str,
    expected_path: &Path,
) -> bool {
    let expected = harness::win_path::normalize_for_compare(&expected_path.display().to_string());
    let dll_field = format!(r#"dll="{dll_name}""#);

    token_lines(stdout).iter().copied().any(|line| {
        line.starts_with(prefix)
            && line.contains(&dll_field)
            && quoted_field_value(line, "path")
                .map(harness::win_path::normalize_for_compare)
                .map(|actual| actual == expected)
                .unwrap_or(false)
    })
}

#[test]
fn optional_probe_then_later_success_finishes_cleanly() {
    let paths = harness::paths::require_from_env();
    let case = harness::case::TestCase::new(&paths, "dynamic_optional_probe_success")
        .expect("failed to initialize test case");
    let app_dir = case.mkdir("app").expect("failed to create app directory");
    case.mkdir("good").expect("failed to create good directory");
    let exe = case
        .copy_fixture(
            harness::fixture::HOST_DYNAMIC_LOADLIBRARY_SEQUENCE_EXE,
            "app\\host_dynamic_loadlibrary_sequence.exe",
        )
        .expect("failed to copy sequence host");
    let fallback = case
        .copy_fixture_as(
            harness::fixture::DLL_LWTEST_A_V1,
            "good",
            "lwtest_optional.dll",
        )
        .expect("failed to copy fallback dll");
    case.copy_fixture_as(harness::fixture::DLL_LWTEST_B, "app", "lwtest_b.dll")
        .expect("failed to copy app-local dependency");

    let args = vec![
        OsString::from("run"),
        OsString::from("--cwd"),
        harness::case::os(&app_dir),
        harness::case::os(&exe),
        OsString::from("optional:lwtest_optional.dll"),
        harness::case::os(&fallback),
    ];
    let result =
        harness::run_loadwhat::run_public(&paths, case.root(), &args, Duration::from_secs(20))
            .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 0);
    assert_eq!(
        token_lines(&result.stdout),
        vec!["SUCCESS status=0"],
        "optional probe should be discarded after later success.\n{}",
        result.stdout
    );
}

#[test]
fn delayed_plugin_failure_is_currently_diagnosed_by_phase_c() {
    let paths = harness::paths::require_from_env();
    let case = harness::case::TestCase::new(&paths, "dynamic_delayed_plugin_failure")
        .expect("failed to initialize test case");
    let app_dir = case.mkdir("app").expect("failed to create app directory");
    case.mkdir("good").expect("failed to create good directory");
    let exe = case
        .copy_fixture(
            harness::fixture::HOST_DYNAMIC_LOADLIBRARY_SEQUENCE_EXE,
            "app\\host_dynamic_loadlibrary_sequence.exe",
        )
        .expect("failed to copy sequence host");
    let startup_ok = case
        .copy_fixture_as(
            harness::fixture::DLL_LWTEST_A_V1,
            "good",
            "lwtest_startup.dll",
        )
        .expect("failed to copy startup dll");
    case.copy_fixture_as(harness::fixture::DLL_LWTEST_B, "app", "lwtest_b.dll")
        .expect("failed to copy app-local dependency");

    let args = vec![
        OsString::from("run"),
        OsString::from("--cwd"),
        harness::case::os(&app_dir),
        harness::case::os(&exe),
        harness::case::os(&startup_ok),
        OsString::from("sleep:250"),
        OsString::from("lwtest_plugin_missing.dll"),
    ];
    let result =
        harness::run_loadwhat::run_public(&paths, case.root(), &args, Duration::from_secs(20))
            .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 10);
    assert!(
        result
            .stdout
            .contains(r#"DYNAMIC_MISSING dll="lwtest_plugin_missing.dll" reason="NOT_FOUND""#),
        "current v1 Phase C should still diagnose delayed missing plugins within the observed run.\n{}",
        result.stdout
    );
}

#[test]
fn app_local_failure_beats_later_windows_noise_in_public_output() {
    let paths = harness::paths::require_from_env();
    let case = harness::case::TestCase::new(&paths, "dynamic_app_local_beats_windows_noise")
        .expect("failed to initialize test case");
    let app_dir = case.mkdir("app").expect("failed to create app directory");
    let exe = case
        .copy_fixture(
            harness::fixture::HOST_DYNAMIC_LOADLIBRARY_SEQUENCE_EXE,
            "app\\host_dynamic_loadlibrary_sequence.exe",
        )
        .expect("failed to copy sequence host");
    let windows_noise = system32_path("ui_noise_missing.dll");

    let args = vec![
        OsString::from("run"),
        OsString::from("--cwd"),
        harness::case::os(&app_dir),
        harness::case::os(&exe),
        OsString::from("lwtest_required.dll"),
        harness::case::os(&windows_noise),
    ];
    let result =
        harness::run_loadwhat::run_public(&paths, case.root(), &args, Duration::from_secs(20))
            .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 10);
    assert!(
        result
            .stdout
            .contains(r#"DYNAMIC_MISSING dll="lwtest_required.dll" reason="NOT_FOUND""#),
        "later Windows-path noise should not replace the earlier app-local failure.\n{}",
        result.stdout
    );
}

#[test]
fn unicode_and_spaced_paths_are_preserved_in_runtime_and_search_output() {
    let paths = harness::paths::require_from_env();
    let case = harness::case::TestCase::new(&paths, "runtime_unicode_paths")
        .expect("failed to initialize test case");
    let app_dir = case
        .mkdir("app space é")
        .expect("failed to create unicode app directory");
    let _dll_dir = case
        .mkdir("dll zone é")
        .expect("failed to create unicode dll directory");
    let search_dir = case
        .mkdir("search dir é")
        .expect("failed to create unicode search directory");

    let fullpath_exe = case
        .copy_fixture(
            harness::fixture::HOST_DYNAMIC_LOADLIBRARY_FULLPATH_EXE,
            "app space é\\host_dynamic_loadlibrary_fullpath.exe",
        )
        .expect("failed to copy fullpath host");
    let fullpath_dll = case
        .copy_fixture_as(
            harness::fixture::DLL_LWTEST_A_V1,
            "dll zone é",
            "lwtest_a.dll",
        )
        .expect("failed to copy fullpath dll");
    case.copy_fixture_as(
        harness::fixture::DLL_LWTEST_B,
        "app space é",
        "lwtest_b.dll",
    )
    .expect("failed to copy dependency");

    let run_args = vec![
        OsString::from("run"),
        OsString::from("--cwd"),
        harness::case::os(&app_dir),
        OsString::from("-v"),
        harness::case::os(&fullpath_exe),
        harness::case::os(&fullpath_dll),
    ];
    let run_result =
        harness::run_loadwhat::run_public(&paths, case.root(), &run_args, Duration::from_secs(20))
            .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&run_result);
    harness::assert::assert_exit_code(&run_result, 0);
    assert!(
        contains_normalized_path_token(
            &run_result.stdout,
            "RUNTIME_LOADED ",
            "lwtest_a.dll",
            &fullpath_dll,
        ),
        "runtime output should preserve unicode and spaced paths.\n{}",
        run_result.stdout
    );

    let static_exe = case
        .copy_fixture(
            harness::fixture::HOST_STATIC_IMPORTS_MISSING_EXE,
            "app space é\\host_static_imports_missing.exe",
        )
        .expect("failed to copy static host");
    case.copy_fixture_as(
        harness::fixture::DLL_LWTEST_A_V1,
        "search dir é",
        "lwtest_a.dll",
    )
    .expect("failed to copy search dll");
    case.copy_fixture_as(
        harness::fixture::DLL_LWTEST_B,
        "search dir é",
        "lwtest_b.dll",
    )
    .expect("failed to copy search dependency");
    let path_env = path_env_value(&[search_dir.as_path()]);
    let imports_args = vec![
        OsString::from("imports"),
        harness::case::os(&static_exe),
        OsString::from("--cwd"),
        harness::case::os(&app_dir),
    ];
    let imports_result = harness::run_loadwhat::run_public_with_env(
        &paths,
        case.root(),
        &imports_args,
        Duration::from_secs(20),
        &[("PATH", path_env.as_str())],
    )
    .expect("failed to run imports");

    harness::assert::assert_not_timed_out(&imports_result);
    harness::assert::assert_exit_code(&imports_result, 0);
    assert!(
        contains_normalized_path_token(
            &imports_result.stdout,
            "SEARCH_PATH ",
            "lwtest_a.dll",
            &search_dir.join("lwtest_a.dll"),
        ),
        "search output should preserve unicode and spaced paths.\n{}",
        imports_result.stdout
    );
}

#[test]
fn unrelated_non_loader_failure_does_not_invent_dll_diagnoses() {
    let paths = harness::paths::require_from_env();
    let case = harness::case::TestCase::new(&paths, "non_loader_early_failure")
        .expect("failed to initialize test case");
    case.mkdir("app").expect("failed to create app directory");
    let exe = case
        .copy_fixture(
            harness::fixture::HOST_ECHO_ARGV_CWD_EXE,
            "app\\host_echo_argv_cwd.exe",
        )
        .expect("failed to copy echo fixture");

    let args = vec![
        OsString::from("run"),
        harness::case::os(&exe),
        OsString::from("--lwtest-exit-code"),
        OsString::from("7"),
    ];
    let result =
        harness::run_loadwhat::run_public(&paths, case.root(), &args, Duration::from_secs(20))
            .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 21);
    assert!(
        !token_lines(&result.stdout)
            .iter()
            .any(|line| { line.starts_with("STATIC_") || line.starts_with("DYNAMIC_") }),
        "non-loader failures should not produce false DLL diagnoses.\n{}",
        result.stdout
    );
}
