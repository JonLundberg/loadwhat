use crate::harness;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
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

struct EchoObservation {
    cwd: String,
    args: Vec<String>,
}

fn parse_echo_observation(stdout: &str) -> EchoObservation {
    let mut cwd = None::<String>;
    let mut argc = None::<usize>;
    let mut args = Vec::<Option<String>>::new();

    for line in stdout.lines().map(|line| line.trim()) {
        if let Some(value) = line.strip_prefix("HOST_CWD: ") {
            cwd = Some(value.to_string());
            continue;
        }
        if let Some(value) = line.strip_prefix("HOST_ARGC: ") {
            argc = Some(
                value
                    .parse::<usize>()
                    .unwrap_or_else(|_| panic!("invalid HOST_ARGC line: {line}")),
            );
            continue;
        }
        if let Some(rest) = line.strip_prefix("HOST_ARG[") {
            let Some((index, value)) = rest.split_once("]: ") else {
                panic!("invalid HOST_ARG line: {line}");
            };
            let index = index
                .parse::<usize>()
                .unwrap_or_else(|_| panic!("invalid HOST_ARG index: {line}"));
            if args.len() <= index {
                args.resize(index + 1, None);
            }
            args[index] = Some(value.to_string());
        }
    }

    let argc = argc.unwrap_or_else(|| panic!("missing HOST_ARGC line.\n{stdout}"));
    let cwd = cwd.unwrap_or_else(|| panic!("missing HOST_CWD line.\n{stdout}"));
    let args: Vec<String> = args
        .into_iter()
        .map(|value| value.unwrap_or_else(|| panic!("missing HOST_ARG line.\n{stdout}")))
        .collect();
    assert_eq!(argc, args.len(), "HOST_ARGC mismatch.\n{stdout}");

    EchoObservation { cwd, args }
}

fn assert_public_output(result: &harness::run_loadwhat::RunResult) {
    assert!(
        !result.stdout.contains("LWTEST:"),
        "public runner leaked internal harness lines.\n{}",
        result.stdout
    );
}

fn assert_normalized_path_eq(actual: &str, expected: &Path) {
    let expected = expected.display().to_string();
    assert_eq!(
        harness::win_path::normalize_for_compare(actual),
        harness::win_path::normalize_for_compare(&expected),
        "path mismatch.\nactual: {actual}\nexpected: {expected}"
    );
}

fn make_echo_case(
    paths: &harness::paths::HarnessPaths,
    case_name: &str,
) -> (harness::case::TestCase, PathBuf, PathBuf) {
    let case =
        harness::case::TestCase::new(paths, case_name).expect("failed to initialize test case");
    case.mkdir("app").expect("failed to create app directory");
    let launch_dir = case
        .mkdir("launch")
        .expect("failed to create launch directory");
    let exe = case
        .copy_fixture(
            harness::fixture::HOST_ECHO_ARGV_CWD_EXE,
            "app\\host_echo_argv_cwd.exe",
        )
        .expect("failed to copy echo fixture");
    (case, exe, launch_dir)
}

fn make_dynamic_missing_case(
    paths: &harness::paths::HarnessPaths,
    case_name: &str,
) -> (harness::case::TestCase, PathBuf, PathBuf) {
    let case =
        harness::case::TestCase::new(paths, case_name).expect("failed to initialize test case");
    let app_dir = case.mkdir("app").expect("failed to create app directory");
    let exe = case
        .copy_fixture(
            harness::fixture::HOST_DYNAMIC_LOADLIBRARY_NAME_EXE,
            "app\\host_dynamic_loadlibrary_name.exe",
        )
        .expect("failed to copy dynamic host fixture");
    (case, exe, app_dir)
}

#[test]
fn group_a_minimal_run_uses_summary_output() {
    let paths = harness::paths::require_from_env();
    let (_case, exe, launch_dir) = make_echo_case(&paths, "run_cli_contract_a1");
    let args = vec![OsString::from("run"), harness::case::os(&exe)];
    let result = harness::run_loadwhat::run_public(&paths, &launch_dir, &args, Duration::from_secs(20))
        .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 0);
    assert_public_output(&result);

    let observation = parse_echo_observation(&result.stdout);
    assert_normalized_path_eq(&observation.cwd, &launch_dir);
    assert!(observation.args.is_empty(), "{}", result.stdout);
    assert_eq!(token_lines(&result.stdout), vec!["SUCCESS status=0"]);
}

#[test]
fn group_a_cwd_is_consumed_before_target() {
    let paths = harness::paths::require_from_env();
    let (_case, exe, launch_dir) = make_echo_case(&paths, "run_cli_contract_a2");
    let cwd_dir = launch_dir.join("child-cwd");
    std::fs::create_dir_all(&cwd_dir).expect("failed to create cwd directory");

    let args = vec![
        OsString::from("run"),
        OsString::from("--cwd"),
        harness::case::os(&cwd_dir),
        harness::case::os(&exe),
    ];
    let result = harness::run_loadwhat::run_public(&paths, &launch_dir, &args, Duration::from_secs(20))
        .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 0);
    let observation = parse_echo_observation(&result.stdout);
    assert_normalized_path_eq(&observation.cwd, &cwd_dir);
    assert!(observation.args.is_empty(), "{}", result.stdout);
}

#[test]
fn group_a_timeout_ms_is_accepted() {
    let paths = harness::paths::require_from_env();
    let (_case, exe, launch_dir) = make_echo_case(&paths, "run_cli_contract_a3");
    let args = vec![
        OsString::from("run"),
        OsString::from("--timeout-ms"),
        OsString::from("1234"),
        harness::case::os(&exe),
    ];
    let result = harness::run_loadwhat::run_public(&paths, &launch_dir, &args, Duration::from_secs(20))
        .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 0);
    assert_eq!(token_lines(&result.stdout), vec!["SUCCESS status=0"]);
}

#[test]
fn group_a_trace_emits_public_trace_tokens() {
    let paths = harness::paths::require_from_env();
    let (_case, exe, app_dir) = make_dynamic_missing_case(&paths, "run_cli_contract_a4");
    let args = vec![
        OsString::from("run"),
        OsString::from("--cwd"),
        harness::case::os(&app_dir),
        OsString::from("--trace"),
        harness::case::os(&exe),
    ];
    let result = harness::run_loadwhat::run_public(&paths, app_dir.parent().unwrap(), &args, Duration::from_secs(20))
        .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 10);
    assert_public_output(&result);
    let lines = token_lines(&result.stdout);
    assert!(
        lines.iter().any(|line| line.starts_with("DYNAMIC_MISSING "))
            && lines.iter().any(|line| line.starts_with("SEARCH_ORDER "))
            && lines.iter().any(|line| line.starts_with("SEARCH_PATH ")),
        "expected public trace output.\n{}",
        result.stdout
    );
}

#[test]
fn group_a_verbose_implies_trace() {
    let paths = harness::paths::require_from_env();
    let (_case, exe, app_dir) = make_dynamic_missing_case(&paths, "run_cli_contract_a5");
    let args = vec![
        OsString::from("run"),
        OsString::from("--cwd"),
        harness::case::os(&app_dir),
        OsString::from("-v"),
        harness::case::os(&exe),
    ];
    let result = harness::run_loadwhat::run_public(&paths, app_dir.parent().unwrap(), &args, Duration::from_secs(20))
        .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 10);
    let lines = token_lines(&result.stdout);
    assert!(
        lines.iter().any(|line| line.starts_with("RUN_START "))
            && lines.iter().any(|line| line.starts_with("DEBUG_STRING "))
            && lines.iter().any(|line| line.starts_with("RUN_END "))
            && lines.iter().any(|line| line.starts_with("DYNAMIC_MISSING ")),
        "expected verbose trace output.\n{}",
        result.stdout
    );
}

#[test]
fn group_a_no_loader_snaps_disables_dynamic_inference() {
    let paths = harness::paths::require_from_env();
    let (_case, exe, app_dir) = make_dynamic_missing_case(&paths, "run_cli_contract_a6");
    let args = vec![
        OsString::from("run"),
        OsString::from("--cwd"),
        harness::case::os(&app_dir),
        OsString::from("--no-loader-snaps"),
        harness::case::os(&exe),
    ];
    let result = harness::run_loadwhat::run_public(&paths, app_dir.parent().unwrap(), &args, Duration::from_secs(20))
        .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 21);
    assert!(
        !result.stdout.contains("DYNAMIC_MISSING"),
        "dynamic inference should be disabled.\n{}",
        result.stdout
    );
}

#[test]
fn group_a_mixed_options_preserve_target_args() {
    let paths = harness::paths::require_from_env();
    let (_case, exe, launch_dir) = make_echo_case(&paths, "run_cli_contract_a7");
    let cwd_dir = launch_dir.join("trace-cwd");
    std::fs::create_dir_all(&cwd_dir).expect("failed to create cwd directory");

    let args = vec![
        OsString::from("run"),
        OsString::from("--cwd"),
        harness::case::os(&cwd_dir),
        OsString::from("--trace"),
        harness::case::os(&exe),
        OsString::from("--flag"),
        OsString::from("value"),
    ];
    let result = harness::run_loadwhat::run_public(&paths, &launch_dir, &args, Duration::from_secs(20))
        .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 0);
    let observation = parse_echo_observation(&result.stdout);
    assert_normalized_path_eq(&observation.cwd, &cwd_dir);
    assert_eq!(observation.args, vec!["--flag".to_string(), "value".to_string()]);
    assert!(
        !token_lines(&result.stdout).iter().any(|line| *line == "SUCCESS status=0"),
        "--trace should suppress summary-only success output.\n{}",
        result.stdout
    );
}

#[test]
fn group_a_target_args_that_look_like_loadwhat_options_are_passed_through() {
    let paths = harness::paths::require_from_env();
    let (_case, exe, launch_dir) = make_echo_case(&paths, "run_cli_contract_a8");
    let args = vec![
        OsString::from("run"),
        OsString::from("-v"),
        harness::case::os(&exe),
        OsString::from("--trace"),
        OsString::from("--cwd"),
        OsString::from("X"),
        OsString::from("--timeout"),
        OsString::from("5"),
    ];
    let result = harness::run_loadwhat::run_public(&paths, &launch_dir, &args, Duration::from_secs(20))
        .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 0);
    let observation = parse_echo_observation(&result.stdout);
    assert_eq!(
        observation.args,
        vec![
            "--trace".to_string(),
            "--cwd".to_string(),
            "X".to_string(),
            "--timeout".to_string(),
            "5".to_string(),
        ]
    );
    let lines = token_lines(&result.stdout);
    assert!(
        lines.iter().any(|line| line.starts_with("RUN_START "))
            && lines.iter().any(|line| line.starts_with("RUN_END ")),
        "expected verbose runtime lines.\n{}",
        result.stdout
    );
}

#[test]
fn group_b_post_target_cwd_is_passed_to_target_not_loadwhat() {
    let paths = harness::paths::require_from_env();
    let (_case, exe, launch_dir) = make_echo_case(&paths, "run_cli_contract_b1");
    let post_target_cwd = launch_dir.join("ignored-cwd");
    std::fs::create_dir_all(&post_target_cwd).expect("failed to create ignored cwd directory");
    let args = vec![
        OsString::from("run"),
        harness::case::os(&exe),
        OsString::from("--cwd"),
        harness::case::os(&post_target_cwd),
    ];
    let result = harness::run_loadwhat::run_public(&paths, &launch_dir, &args, Duration::from_secs(20))
        .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 0);
    let observation = parse_echo_observation(&result.stdout);
    assert_normalized_path_eq(&observation.cwd, &launch_dir);
    assert_eq!(
        observation.args,
        vec![
            "--cwd".to_string(),
            post_target_cwd.display().to_string(),
        ]
    );
    assert_eq!(token_lines(&result.stdout), vec!["SUCCESS status=0"]);
}

#[test]
fn group_b_post_target_trace_is_pass_through() {
    let paths = harness::paths::require_from_env();
    let (_case, exe, launch_dir) = make_echo_case(&paths, "run_cli_contract_b2");
    let args = vec![
        OsString::from("run"),
        harness::case::os(&exe),
        OsString::from("--trace"),
    ];
    let result = harness::run_loadwhat::run_public(&paths, &launch_dir, &args, Duration::from_secs(20))
        .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 0);
    let observation = parse_echo_observation(&result.stdout);
    assert_eq!(observation.args, vec!["--trace".to_string()]);
    assert_eq!(token_lines(&result.stdout), vec!["SUCCESS status=0"]);
}

#[test]
fn group_b_post_target_verbose_is_pass_through() {
    let paths = harness::paths::require_from_env();
    let (_case, exe, launch_dir) = make_echo_case(&paths, "run_cli_contract_b3");
    let args = vec![
        OsString::from("run"),
        harness::case::os(&exe),
        OsString::from("-v"),
    ];
    let result = harness::run_loadwhat::run_public(&paths, &launch_dir, &args, Duration::from_secs(20))
        .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 0);
    let observation = parse_echo_observation(&result.stdout);
    assert_eq!(observation.args, vec!["-v".to_string()]);
    assert_eq!(token_lines(&result.stdout), vec!["SUCCESS status=0"]);
}

#[test]
fn group_b_post_target_no_loader_snaps_is_pass_through() {
    let paths = harness::paths::require_from_env();
    let (_case, exe, launch_dir) = make_echo_case(&paths, "run_cli_contract_b4");
    let args = vec![
        OsString::from("run"),
        harness::case::os(&exe),
        OsString::from("--no-loader-snaps"),
    ];
    let result = harness::run_loadwhat::run_public(&paths, &launch_dir, &args, Duration::from_secs(20))
        .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 0);
    let observation = parse_echo_observation(&result.stdout);
    assert_eq!(observation.args, vec!["--no-loader-snaps".to_string()]);
    assert_eq!(token_lines(&result.stdout), vec!["SUCCESS status=0"]);
}

#[test]
fn group_b_post_target_timeout_is_pass_through() {
    let paths = harness::paths::require_from_env();
    let (_case, exe, launch_dir) = make_echo_case(&paths, "run_cli_contract_b5");
    let args = vec![
        OsString::from("run"),
        harness::case::os(&exe),
        OsString::from("--timeout-ms"),
        OsString::from("9999"),
    ];
    let result = harness::run_loadwhat::run_public(&paths, &launch_dir, &args, Duration::from_secs(20))
        .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 0);
    let observation = parse_echo_observation(&result.stdout);
    assert_eq!(
        observation.args,
        vec!["--timeout-ms".to_string(), "9999".to_string()]
    );
    assert_eq!(token_lines(&result.stdout), vec!["SUCCESS status=0"]);
}
