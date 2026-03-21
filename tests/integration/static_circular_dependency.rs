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
fn imports_circular_dependency_terminates() {
    let paths = harness::paths::require_from_env();
    let case = harness::case::TestCase::new(&paths, "imports_circular_dep")
        .expect("failed to initialize test case");

    let dir = case.mkdir("app").expect("failed to create app directory");

    let root = dir.join("root.exe");
    harness::pe_builder::write_import_test_pe(&root, &["a.dll"])
        .expect("failed to write root.exe");

    let a_dll = dir.join("a.dll");
    harness::pe_builder::write_import_test_pe(&a_dll, &["b.dll"])
        .expect("failed to write a.dll");

    let b_dll = dir.join("b.dll");
    harness::pe_builder::write_import_test_pe(&b_dll, &["a.dll"])
        .expect("failed to write b.dll");

    let args = vec![
        OsString::from("imports"),
        harness::case::os(&root),
        OsString::from("--cwd"),
        harness::case::os(&dir),
    ];

    // Run twice to verify deterministic output
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

    let first_lines = token_lines(&first.stdout);
    let second_lines = token_lines(&second.stdout);
    assert_eq!(
        first_lines, second_lines,
        "circular dependency output should be deterministic across runs"
    );

    let summary = first_lines
        .iter()
        .copied()
        .find(|line| line.starts_with("SUMMARY "))
        .expect("expected SUMMARY line");
    assert!(
        summary.contains("static_missing=0") && summary.contains("static_bad_image=0"),
        "circular deps where all DLLs exist should report no issues.\n{}",
        summary
    );
}
