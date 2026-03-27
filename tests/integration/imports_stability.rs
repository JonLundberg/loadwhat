use crate::harness;
use std::ffi::OsString;
use std::time::Duration;

#[test]
fn imports_multi_module_bfs_output_is_deterministic_across_runs() {
    let paths = harness::paths::require_from_env();
    let case = harness::case::TestCase::new(&paths, "imports_stability_multi_module")
        .expect("failed to initialize test case");

    let dir = case.mkdir("app").expect("failed to create app directory");

    // root.exe -> a.dll, b.dll, kernel32.dll
    // a.dll -> kernel32.dll
    // b.dll -> kernel32.dll
    harness::pe_builder::write_import_test_pe(
        &dir.join("root.exe"),
        &["kernel32.dll", "a.dll", "b.dll"],
    )
    .expect("failed to write root.exe");
    harness::pe_builder::write_import_test_pe(&dir.join("a.dll"), &["kernel32.dll"])
        .expect("failed to write a.dll");
    harness::pe_builder::write_import_test_pe(&dir.join("b.dll"), &["kernel32.dll"])
        .expect("failed to write b.dll");

    let root = dir.join("root.exe");
    let args = vec![
        OsString::from("imports"),
        harness::case::os(&root),
        OsString::from("--cwd"),
        harness::case::os(&dir),
    ];

    let first =
        harness::run_loadwhat::run_public(&paths, case.root(), &args, Duration::from_secs(20))
            .expect("failed to run first imports command");
    let second =
        harness::run_loadwhat::run_public(&paths, case.root(), &args, Duration::from_secs(20))
            .expect("failed to run second imports command");

    harness::assert::assert_not_timed_out(&first);
    harness::assert::assert_not_timed_out(&second);
    harness::assert::assert_exit_code(&first, 0);
    harness::assert::assert_exit_code(&second, 0);

    assert_eq!(
        first.stdout, second.stdout,
        "multi-module BFS output should be identical across runs"
    );

    // Verify root STATIC_IMPORT lines are in alphabetical order
    let imports: Vec<&str> = first
        .stdout
        .lines()
        .map(|line| line.trim())
        .filter(|line| line.starts_with("STATIC_IMPORT ") && line.contains(r#"module="root.exe""#))
        .collect();
    assert!(
        imports.len() >= 3,
        "expected at least 3 STATIC_IMPORT lines for root.exe.\n{}",
        first.stdout
    );
    assert!(
        imports[0].contains(r#"needs="a.dll""#)
            && imports[1].contains(r#"needs="b.dll""#)
            && imports[2].contains(r#"needs="kernel32.dll""#),
        "STATIC_IMPORT lines should be in alphabetical order: a.dll, b.dll, kernel32.dll.\n{}",
        first.stdout
    );
}
