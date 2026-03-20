use crate::harness;
use std::ffi::OsString;
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

#[test]
fn verbose_mode_emits_unreadable_debug_string_and_continues() {
    let paths = harness::paths::require_from_env();
    let case = harness::case::TestCase::new(&paths, "run_unreadable_debug_string")
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
        OsString::from("-v"),
        harness::case::os(&exe),
    ];
    let result = harness::run_loadwhat::run_public_with_env(
        &paths,
        case.root(),
        &args,
        Duration::from_secs(20),
        &[("LOADWHAT_TEST_FORCE_UNREADABLE_DEBUG_STRING", "1")],
    )
    .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 0);
    let lines = token_lines(&result.stdout);
    assert!(
        lines.iter().any(|line| {
            line.starts_with("DEBUG_STRING ")
                && line.contains(r#"source="OUTPUT_DEBUG_STRING_EVENT""#)
                && line.contains(r#"text="UNREADABLE""#)
        }),
        "expected unreadable debug-string fallback.\n{}",
        result.stdout
    );
    assert!(
        lines.iter().any(|line| line.starts_with("RUN_END ")),
        "run should continue and emit RUN_END.\n{}",
        result.stdout
    );
}
