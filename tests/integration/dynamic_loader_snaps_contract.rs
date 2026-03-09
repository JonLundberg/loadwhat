use crate::harness;
use std::ffi::OsString;
use std::fs;
use std::time::Duration;

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

fn find_token_line<'a>(lines: &'a [&str], prefix: &str) -> Option<&'a str> {
    lines.iter().copied().find(|line| line.starts_with(prefix))
}

fn quoted_field_value<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!(r#"{key}=""#);
    let start = line.find(&needle)? + needle.len();
    let rest = &line[start..];
    let end = rest.find('"')?;
    Some(&rest[..end])
}

#[test]
fn dynamic_summary_mode_emits_single_missing_line() {
    let Some(paths) = harness::paths::from_env() else {
        return;
    };

    let case = harness::case::TestCase::new(&paths, "dynamic_summary_contract")
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
    let result =
        harness::run_loadwhat::run_public(&paths, case.root(), &args, Duration::from_secs(20))
            .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 10);

    let lines = token_lines(&result.stdout);
    assert_eq!(
        lines.len(),
        1,
        "expected one summary line.\n{}",
        result.stdout
    );
    assert!(
        lines[0].starts_with("DYNAMIC_MISSING ")
            && lines[0].contains(r#"dll="lwtest_a.dll""#)
            && lines[0].contains(r#"reason="NOT_FOUND""#),
        "unexpected summary line: {}",
        lines[0]
    );
    assert!(
        !result.stdout.contains("SEARCH_ORDER")
            && !result.stdout.contains("SEARCH_PATH")
            && !result.stdout.contains("DEBUG_STRING")
            && !result.stdout.contains("RUN_START")
            && !result.stdout.contains("NOTE "),
        "summary mode should suppress trace-only lines.\n{}",
        result.stdout
    );
}

#[test]
fn dynamic_trace_mode_emits_search_evidence() {
    let Some(paths) = harness::paths::from_env() else {
        return;
    };

    let case = harness::case::TestCase::new(&paths, "dynamic_trace_contract")
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
        OsString::from("--trace"),
    ];
    let result =
        harness::run_loadwhat::run_public(&paths, case.root(), &args, Duration::from_secs(20))
            .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 10);

    let lines = token_lines(&result.stdout);
    let dynamic = find_token_line(&lines, "DYNAMIC_MISSING ").expect("missing DYNAMIC_MISSING");
    assert!(dynamic.contains(r#"dll="lwtest_a.dll""#), "{}", dynamic);
    assert!(
        find_token_line(&lines, "SEARCH_ORDER ").is_some(),
        "expected SEARCH_ORDER.\n{}",
        result.stdout
    );
    assert!(
        find_token_line(&lines, "SEARCH_PATH ").is_some(),
        "expected SEARCH_PATH.\n{}",
        result.stdout
    );
    assert!(
        find_token_line(&lines, "RUN_START ").is_none()
            && find_token_line(&lines, "DEBUG_STRING ").is_none(),
        "trace mode without -v should not emit verbose runtime lines.\n{}",
        result.stdout
    );
}

#[test]
fn dynamic_verbose_mode_keeps_diagnosis_stable() {
    let Some(paths) = harness::paths::from_env() else {
        return;
    };

    let case = harness::case::TestCase::new(&paths, "dynamic_verbose_contract")
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
        OsString::from("-v"),
    ];
    let result =
        harness::run_loadwhat::run_public(&paths, case.root(), &args, Duration::from_secs(20))
            .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 10);

    let lines = token_lines(&result.stdout);
    let dynamic = find_token_line(&lines, "DYNAMIC_MISSING ").expect("missing DYNAMIC_MISSING");
    assert!(
        dynamic.contains(r#"dll="lwtest_a.dll""#) && dynamic.contains(r#"reason="NOT_FOUND""#),
        "{}",
        dynamic
    );
    let summary = find_token_line(&lines, "SUMMARY ").expect("missing SUMMARY");
    assert!(
        find_token_line(&lines, "RUN_START ").is_some()
            && find_token_line(&lines, "RUNTIME_LOADED ").is_some()
            && find_token_line(&lines, "RUN_END ").is_some()
            && find_token_line(&lines, "DEBUG_STRING ").is_some()
            && !result.stdout.lines().any(|line| line.trim().starts_with("LOAD ")),
        "expected verbose runtime detail.\n{}",
        result.stdout
    );
    assert!(
        summary.contains("first_break=true")
            && summary.contains("static_missing=0")
            && summary.contains("static_bad_image=0")
            && summary.contains("dynamic_missing=1")
            && !summary.contains("missing_static="),
        "unexpected verbose SUMMARY fields.\n{}",
        summary
    );
}

#[test]
fn static_missing_beats_loader_snaps_dynamic_noise() {
    let Some(paths) = harness::paths::from_env() else {
        return;
    };

    let case = harness::case::TestCase::new(&paths, "static_beats_dynamic")
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
        OsString::from("-v"),
    ];
    let result =
        harness::run_loadwhat::run_public(&paths, case.root(), &args, Duration::from_secs(20))
            .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 10);

    let lines = token_lines(&result.stdout);
    assert!(
        find_token_line(&lines, "STATIC_MISSING ").is_some(),
        "expected STATIC_MISSING.\n{}",
        result.stdout
    );
    assert!(
        find_token_line(&lines, "DEBUG_STRING ").is_some(),
        "expected loader-snaps debug chatter.\n{}",
        result.stdout
    );
    assert!(
        find_token_line(&lines, "DYNAMIC_MISSING ").is_none(),
        "static diagnosis should win over loader-snaps noise.\n{}",
        result.stdout
    );
}

#[test]
fn dynamic_success_with_loader_snaps_emits_success_only() {
    let Some(paths) = harness::paths::from_env() else {
        return;
    };

    let case = harness::case::TestCase::new(&paths, "dynamic_success_loader_snaps")
        .expect("failed to initialize test case");
    case.mkdir("app").expect("failed to create app directory");
    let cwd_dir = case.mkdir("cwd").expect("failed to create cwd directory");
    let exe = case
        .copy_fixture(
            harness::fixture::HOST_DYNAMIC_LOADLIBRARY_NAME_EXE,
            "app\\host_dynamic_loadlibrary_name.exe",
        )
        .expect("failed to copy host fixture");
    case.copy_fixture_as(harness::fixture::DLL_LWTEST_A_V1, "cwd", "lwtest_a.dll")
        .expect("failed to copy lwtest_a.dll");
    case.copy_fixture_as(harness::fixture::DLL_LWTEST_B, "cwd", "lwtest_b.dll")
        .expect("failed to copy lwtest_b.dll");

    let args = vec![
        OsString::from("run"),
        harness::case::os(&exe),
        OsString::from("--cwd"),
        harness::case::os(&cwd_dir),
        OsString::from("--loader-snaps"),
    ];
    let result =
        harness::run_loadwhat::run_public(&paths, case.root(), &args, Duration::from_secs(20))
            .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 0);

    let lines = token_lines(&result.stdout);
    assert_eq!(lines, vec!["SUCCESS status=0"], "{}", result.stdout);
}

#[test]
fn dynamic_fullpath_success_uses_requested_path() {
    let Some(paths) = harness::paths::from_env() else {
        return;
    };

    let case = harness::case::TestCase::new(&paths, "dynamic_fullpath_success")
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
    case.copy_fixture_as(harness::fixture::DLL_LWTEST_B, "app", "lwtest_b.dll")
        .expect("failed to copy app-local lwtest_b.dll");

    let args = vec![
        OsString::from("run"),
        harness::case::os(&exe),
        OsString::from("--cwd"),
        harness::case::os(&app_dir),
        OsString::from("-v"),
        OsString::from("--"),
        harness::case::os(&dll_path),
    ];
    let result =
        harness::run_loadwhat::run_public(&paths, case.root(), &args, Duration::from_secs(20))
            .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 0);

    let lines = token_lines(&result.stdout);
    let loaded = lines
        .iter()
        .copied()
        .find(|line| line.starts_with("RUNTIME_LOADED ") && line.contains(r#"dll="lwtest_a.dll""#))
        .expect("expected RUNTIME_LOADED for lwtest_a.dll");
    let actual_path = quoted_field_value(loaded, "path").expect("missing path field");
    let expected_path = dll_path.display().to_string();
    assert!(
        harness::win_path::normalize_for_compare(actual_path)
            == harness::win_path::normalize_for_compare(&expected_path)
            && !result.stdout.contains("DYNAMIC_MISSING"),
        "expected full-path load success evidence.\n{}",
        result.stdout
    );
}

#[test]
fn dynamic_bad_image_is_classified_as_bad_image() {
    let Some(paths) = harness::paths::from_env() else {
        return;
    };

    let case = harness::case::TestCase::new(&paths, "dynamic_bad_image")
        .expect("failed to initialize test case");
    let app_dir = case.mkdir("app").expect("failed to create app directory");
    let bad_dir = case.mkdir("bad").expect("failed to create bad directory");
    let exe = case
        .copy_fixture(
            harness::fixture::HOST_DYNAMIC_LOADLIBRARY_FULLPATH_EXE,
            "app\\host_dynamic_loadlibrary_fullpath.exe",
        )
        .expect("failed to copy host fixture");
    let bad_dll = bad_dir.join("bad.dll");
    fs::write(&bad_dll, b"not a dll").expect("failed to create bad-image payload");

    let args = vec![
        OsString::from("run"),
        harness::case::os(&exe),
        OsString::from("--cwd"),
        harness::case::os(&app_dir),
        OsString::from("--loader-snaps"),
        OsString::from("--"),
        harness::case::os(&bad_dll),
    ];
    let result =
        harness::run_loadwhat::run_public(&paths, case.root(), &args, Duration::from_secs(20))
            .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 10);

    let lines = token_lines(&result.stdout);
    assert_eq!(
        lines.len(),
        1,
        "expected one summary line.\n{}",
        result.stdout
    );
    assert!(
        lines[0].starts_with("DYNAMIC_MISSING ")
            && lines[0].contains(r#"dll="bad.dll""#)
            && lines[0].contains(r#"reason="BAD_IMAGE""#),
        "unexpected bad-image diagnosis: {}",
        lines[0]
    );
}

#[test]
fn dynamic_other_includes_status_for_init_failure() {
    let Some(paths) = harness::paths::from_env() else {
        return;
    };

    let case = harness::case::TestCase::new(&paths, "dynamic_other_status")
        .expect("failed to initialize test case");
    case.mkdir("app").expect("failed to create app directory");
    let cwd_dir = case.mkdir("cwd").expect("failed to create cwd directory");
    let exe = case
        .copy_fixture(
            harness::fixture::HOST_DYNAMIC_LOADLIBRARY_NAME_EXE,
            "app\\host_dynamic_loadlibrary_name.exe",
        )
        .expect("failed to copy host fixture");
    case.copy_fixture_as(
        harness::fixture::DLL_LWTEST_A_INITFAIL,
        "cwd",
        "lwtest_a.dll",
    )
    .expect("failed to copy init-fail lwtest_a.dll");
    case.copy_fixture_as(harness::fixture::DLL_LWTEST_B, "cwd", "lwtest_b.dll")
        .expect("failed to copy lwtest_b.dll");

    let args = vec![
        OsString::from("run"),
        harness::case::os(&exe),
        OsString::from("--cwd"),
        harness::case::os(&cwd_dir),
        OsString::from("--loader-snaps"),
    ];
    let result =
        harness::run_loadwhat::run_public(&paths, case.root(), &args, Duration::from_secs(20))
            .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 10);

    let lines = token_lines(&result.stdout);
    assert_eq!(
        lines.len(),
        1,
        "expected one summary line.\n{}",
        result.stdout
    );
    assert!(
        lines[0].starts_with("DYNAMIC_MISSING ")
            && lines[0].contains(r#"dll="lwtest_a.dll""#)
            && lines[0].contains(r#"reason="OTHER""#)
            && lines[0].contains("status=0x"),
        "unexpected init-failure diagnosis: {}",
        lines[0]
    );
}

#[test]
fn dynamic_multiple_failures_choose_first_unresolved_app_failure() {
    let Some(paths) = harness::paths::from_env() else {
        return;
    };

    let case = harness::case::TestCase::new(&paths, "dynamic_multiple_failures")
        .expect("failed to initialize test case");
    let app_dir = case.mkdir("app").expect("failed to create app directory");
    let bad_dir = case.mkdir("bad").expect("failed to create bad directory");
    case.mkdir("good").expect("failed to create good directory");
    let missing_dir = case
        .mkdir("missing")
        .expect("failed to create missing directory");
    let exe = case
        .copy_fixture(
            harness::fixture::HOST_DYNAMIC_LOADLIBRARY_SEQUENCE_EXE,
            "app\\host_dynamic_loadlibrary_sequence.exe",
        )
        .expect("failed to copy host fixture");
    let bad_same_name = bad_dir.join("lwtest_a.dll");
    fs::write(&bad_same_name, b"not a dll").expect("failed to create bad-image same-name payload");
    let good_a = case
        .copy_fixture_as(harness::fixture::DLL_LWTEST_A_V1, "good", "lwtest_a.dll")
        .expect("failed to copy good lwtest_a.dll");
    case.copy_fixture_as(harness::fixture::DLL_LWTEST_B, "app", "lwtest_b.dll")
        .expect("failed to copy app-local lwtest_b.dll");
    let missing_path = missing_dir.join("lwtest_missing.dll");

    let args = vec![
        OsString::from("run"),
        harness::case::os(&exe),
        OsString::from("--cwd"),
        harness::case::os(&app_dir),
        OsString::from("--loader-snaps"),
        OsString::from("--"),
        harness::case::os(&bad_same_name),
        harness::case::os(&good_a),
        harness::case::os(&missing_path),
    ];
    let result =
        harness::run_loadwhat::run_public(&paths, case.root(), &args, Duration::from_secs(20))
            .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 10);

    let lines = token_lines(&result.stdout);
    assert_eq!(
        lines.len(),
        1,
        "expected one summary line.\n{}",
        result.stdout
    );
    assert!(
        lines[0].starts_with("DYNAMIC_MISSING ")
            && lines[0].contains(r#"dll="lwtest_missing.dll""#)
            && lines[0].contains(r#"reason="NOT_FOUND""#)
            && !lines[0].contains("lwtest_a.dll"),
        "unexpected multi-failure diagnosis: {}",
        lines[0]
    );
}

#[test]
fn dynamic_trace_without_search_context_still_emits_missing() {
    let Some(paths) = harness::paths::from_env() else {
        return;
    };

    let case = harness::case::TestCase::new(&paths, "dynamic_no_search_context")
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
        OsString::from("--trace"),
    ];
    let result = harness::run_loadwhat::run_public_with_env(
        &paths,
        case.root(),
        &args,
        Duration::from_secs(20),
        &[("LOADWHAT_TEST_FORCE_DYNAMIC_SEARCH_CONTEXT_FAIL", "1")],
    )
    .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 10);

    let lines = token_lines(&result.stdout);
    assert_eq!(
        lines.len(),
        1,
        "expected one diagnostic line when search context is unavailable.\n{}",
        result.stdout
    );
    assert!(
        lines[0].starts_with("DYNAMIC_MISSING ")
            && lines[0].contains(r#"dll="lwtest_a.dll""#)
            && lines[0].contains(r#"reason="NOT_FOUND""#),
        "unexpected no-search-context diagnosis: {}",
        lines[0]
    );
    assert!(
        find_token_line(&lines, "SEARCH_ORDER ").is_none()
            && find_token_line(&lines, "SEARCH_PATH ").is_none(),
        "trace mode should omit search evidence when search context cannot be built.\n{}",
        result.stdout
    );
}
