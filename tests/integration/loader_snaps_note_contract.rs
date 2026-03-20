use crate::harness;
use std::ffi::OsString;
use std::time::Duration;

fn make_echo_case(
    case_name: &str,
) -> (
    harness::paths::HarnessPaths,
    harness::case::TestCase,
    std::path::PathBuf,
) {
    let paths = harness::paths::require_from_env();
    let case =
        harness::case::TestCase::new(&paths, case_name).expect("failed to initialize test case");
    case.mkdir("app").expect("failed to create app directory");
    let exe = case
        .copy_fixture(
            harness::fixture::HOST_ECHO_ARGV_CWD_EXE,
            "app\\host_echo_argv_cwd.exe",
        )
        .expect("failed to copy echo fixture");
    (paths, case, exe)
}

#[test]
fn summary_mode_omits_terminal_loader_snaps_notes() {
    let (paths, case, exe) = make_echo_case("loader_snaps_summary_terminal_note");
    let args = vec![OsString::from("run"), harness::case::os(&exe)];
    let result = harness::run_loadwhat::run_public_with_env(
        &paths,
        case.root(),
        &args,
        Duration::from_secs(20),
        &[
            ("LOADWHAT_TEST_PEB_ENABLE", "fail:0x00000057"),
            ("LOADWHAT_TEST_IFEO_ENABLE", "fail:0x00000005"),
        ],
    )
    .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 21);
    assert!(
        !result.stdout.contains(r#"topic="loader-snaps""#)
            && !result.stdout.contains(r#"detail="enable-failed""#),
        "summary mode should suppress loader-snaps terminal notes.\n{}",
        result.stdout
    );
}

#[test]
fn trace_mode_emits_enable_failed_note() {
    let (paths, case, exe) = make_echo_case("loader_snaps_trace_enable_failed");
    let args = vec![
        OsString::from("run"),
        OsString::from("--trace"),
        harness::case::os(&exe),
    ];
    let result = harness::run_loadwhat::run_public_with_env(
        &paths,
        case.root(),
        &args,
        Duration::from_secs(20),
        &[
            ("LOADWHAT_TEST_PEB_ENABLE", "fail:0x00000057"),
            ("LOADWHAT_TEST_IFEO_ENABLE", "fail:0x00000005"),
        ],
    )
    .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 21);
    assert!(
        result
            .stdout
            .contains(r#"NOTE topic="loader-snaps" detail="enable-failed" code=0x00000005"#),
        "expected trace-visible enable-failed note.\n{}",
        result.stdout
    );
}

#[test]
fn verbose_mode_emits_peb_enable_failed_note_when_fallback_succeeds() {
    let (paths, case, exe) = make_echo_case("loader_snaps_verbose_peb_enable_failed");
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
        &[
            ("LOADWHAT_TEST_PEB_ENABLE", "fail:0x00000057"),
            ("LOADWHAT_TEST_IFEO_ENABLE", "ok-noop"),
        ],
    )
    .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 0);
    assert!(
        result
            .stdout
            .contains(r#"NOTE topic="loader-snaps" detail="peb-enable-failed" code=0x00000057"#),
        "expected verbose-only peb-enable-failed note.\n{}",
        result.stdout
    );
    assert!(
        !result.stdout.contains(r#"detail="enable-failed""#),
        "fallback success should not emit terminal enable-failed.\n{}",
        result.stdout
    );
}

#[test]
fn trace_mode_emits_restore_failed_note() {
    let (paths, case, exe) = make_echo_case("loader_snaps_trace_restore_failed");
    let args = vec![
        OsString::from("run"),
        OsString::from("--trace"),
        harness::case::os(&exe),
    ];
    let result = harness::run_loadwhat::run_public_with_env(
        &paths,
        case.root(),
        &args,
        Duration::from_secs(20),
        &[
            ("LOADWHAT_TEST_PEB_ENABLE", "fail:0x00000057"),
            ("LOADWHAT_TEST_IFEO_ENABLE", "ok-noop"),
            ("LOADWHAT_TEST_IFEO_RESTORE", "fail:0x00000006"),
        ],
    )
    .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 0);
    assert!(
        result
            .stdout
            .contains(r#"NOTE topic="loader-snaps" detail="restore-failed" code=0x00000006"#),
        "expected restore-failed note.\n{}",
        result.stdout
    );
    assert!(
        !result.stdout.contains(r#"detail="peb-enable-failed""#),
        "trace without -v should not emit verbose-only fallback detail.\n{}",
        result.stdout
    );
}

#[test]
fn summary_mode_omits_wow64_unsupported_note() {
    let (paths, case, exe) = make_echo_case("loader_snaps_summary_wow64");
    let args = vec![OsString::from("run"), harness::case::os(&exe)];
    let result = harness::run_loadwhat::run_public_with_env(
        &paths,
        case.root(),
        &args,
        Duration::from_secs(20),
        &[("LOADWHAT_TEST_PEB_ENABLE", "wow64")],
    )
    .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 22);
    assert!(
        !result
            .stdout
            .contains(r#"detail="wow64-target-unsupported""#),
        "summary mode should suppress wow64 loader-snaps note.\n{}",
        result.stdout
    );
}

#[test]
fn trace_mode_emits_wow64_unsupported_note() {
    let (paths, case, exe) = make_echo_case("loader_snaps_trace_wow64");
    let args = vec![
        OsString::from("run"),
        OsString::from("--trace"),
        harness::case::os(&exe),
    ];
    let result = harness::run_loadwhat::run_public_with_env(
        &paths,
        case.root(),
        &args,
        Duration::from_secs(20),
        &[("LOADWHAT_TEST_PEB_ENABLE", "wow64")],
    )
    .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 22);
    assert!(
        result
            .stdout
            .contains(r#"NOTE topic="loader-snaps" detail="wow64-target-unsupported""#),
        "expected wow64-target-unsupported trace note.\n{}",
        result.stdout
    );
}
