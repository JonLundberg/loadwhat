use crate::harness;
use std::ffi::OsString;
use std::fs;
use std::path::Path;
use std::time::Duration;

fn run_audit(target: &Path, query: &str) -> harness::run_loadwhat::RunResult {
    let paths = harness::paths::require_from_env();
    let current_dir = target.parent().unwrap_or_else(|| Path::new("."));
    let args = vec![
        OsString::from("com"),
        OsString::from("audit"),
        harness::case::os(target),
        OsString::from(query),
    ];
    harness::run_loadwhat::run_public(&paths, current_dir, &args, Duration::from_secs(20))
        .expect("failed to run loadwhat com audit")
}

fn assert_one_summary(result: &harness::run_loadwhat::RunResult, prefix: &str) -> String {
    harness::assert::assert_not_timed_out(result);
    let lines: Vec<_> = result
        .stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    assert_eq!(
        lines.len(),
        1,
        "expected one summary line\n{}",
        result.stdout
    );
    assert!(
        lines[0].starts_with(prefix),
        "expected {prefix} summary\n{}",
        result.stdout
    );
    lines[0].to_string()
}

#[test]
fn com_audit_missing_target_emits_indeterminate_summary() {
    let paths = harness::paths::require_from_env();
    let missing = paths.test_root.join("does-not-exist-com-target.exe");
    let result = run_audit(&missing, "{00000000-0000-0000-0000-000000000001}");

    harness::assert::assert_exit_code(&result, 21);
    let line = assert_one_summary(&result, "COM_AUDIT ");
    assert!(line.contains(r#"target_machine="unknown""#));
    assert!(line.contains(r#"source="none""#));
    assert!(line.contains(r#"status="INDETERMINATE""#));
}

#[test]
fn com_audit_non_pe_target_emits_indeterminate_summary() {
    let paths = harness::paths::require_from_env();
    let case = harness::case::TestCase::new(&paths, "com_audit_non_pe")
        .expect("failed to initialize test case");
    let target = case.root().join("not-a-pe.exe");
    fs::write(&target, b"not a PE image").expect("failed to write non-PE target");

    let result = run_audit(&target, "{00000000-0000-0000-0000-000000000001}");
    harness::assert::assert_exit_code(&result, 21);
    let line = assert_one_summary(&result, "COM_AUDIT ");
    assert!(line.contains(r#"status="INDETERMINATE""#));
}

#[test]
fn com_audit_unsupported_machine_emits_summary() {
    let paths = harness::paths::require_from_env();
    let case = harness::case::TestCase::new(&paths, "com_audit_unsupported_machine")
        .expect("failed to initialize test case");
    let target = case.root().join("unsupported.exe");
    let mut bytes = harness::pe_builder::build_import_test_pe(&[]);
    bytes[0x84..0x86].copy_from_slice(&0x01C4u16.to_le_bytes());
    fs::write(&target, bytes).expect("failed to write unsupported target");

    let result = run_audit(&target, "{00000000-0000-0000-0000-000000000001}");
    harness::assert::assert_exit_code(&result, 22);
    let line = assert_one_summary(&result, "COM_AUDIT ");
    assert!(line.contains(r#"status="UNSUPPORTED_ARCHITECTURE""#));
}

#[test]
fn com_server_dependency_walk_error_emits_indeterminate_summary() {
    let paths = harness::paths::require_from_env();
    let case = harness::case::TestCase::new(&paths, "com_server_indeterminate")
        .expect("failed to initialize test case");
    let server = case.root().join("bad-import-table.dll");
    let mut bytes = harness::pe_builder::build_import_test_pe(&["kernel32.dll"]);
    bytes[0x110..0x114].copy_from_slice(&0xFFFF_0000u32.to_le_bytes());
    fs::write(&server, bytes).expect("failed to write malformed server");

    let args = vec![
        OsString::from("com"),
        OsString::from("server"),
        harness::case::os(&server),
    ];
    let result =
        harness::run_loadwhat::run_public(&paths, case.root(), &args, Duration::from_secs(20))
            .expect("failed to run loadwhat com server");

    harness::assert::assert_exit_code(&result, 21);
    let line = assert_one_summary(&result, "COM_SERVER ");
    assert!(line.contains(r#"status="INDETERMINATE""#));
}
