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
fn imports_on_dll_with_missing_transitive_dep() {
    let paths = harness::paths::require_from_env();
    let case = harness::case::TestCase::new(&paths, "imports_dll_transitive_missing")
        .expect("failed to initialize test case");

    let dir = case.mkdir("app").expect("failed to create app directory");

    let root = dir.join("root.dll");
    harness::pe_builder::write_import_test_pe(&root, &["child.dll"])
        .expect("failed to write root.dll");

    let child = dir.join("child.dll");
    harness::pe_builder::write_import_test_pe(&child, &["missing.dll"])
        .expect("failed to write child.dll");

    // Do NOT create missing.dll

    let args = vec![
        OsString::from("imports"),
        harness::case::os(&root),
        OsString::from("--cwd"),
        harness::case::os(&dir),
    ];
    let result =
        harness::run_loadwhat::run_public(&paths, case.root(), &args, Duration::from_secs(20))
            .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 10);
    let lines = token_lines(&result.stdout);
    let missing_line = lines
        .iter()
        .find(|line| line.starts_with("STATIC_MISSING ") && line.contains(r#"dll="missing.dll""#))
        .expect("expected STATIC_MISSING for missing.dll");
    assert!(
        missing_line.contains(r#"via="child.dll""#),
        "STATIC_MISSING should include via=\"child.dll\".\n{}",
        missing_line
    );
    assert!(
        missing_line.contains("depth=2"),
        "STATIC_MISSING should report depth=2.\n{}",
        missing_line
    );
    assert!(
        !lines.iter().any(|line| {
            line.starts_with("RUN_START ")
                || line.starts_with("RUNTIME_LOADED ")
                || line.starts_with("DEBUG_STRING ")
                || line.starts_with("RUN_END ")
        }),
        "imports command should not produce runtime tokens.\n{}",
        result.stdout
    );
}

#[test]
fn imports_on_dll_with_no_issues() {
    let paths = harness::paths::require_from_env();
    let case = harness::case::TestCase::new(&paths, "imports_dll_no_issues")
        .expect("failed to initialize test case");

    let root = case.root().join("root.dll");
    harness::pe_builder::write_import_test_pe(&root, &["kernel32.dll"])
        .expect("failed to write root.dll");

    let args = vec![OsString::from("imports"), harness::case::os(&root)];
    let result =
        harness::run_loadwhat::run_public(&paths, case.root(), &args, Duration::from_secs(20))
            .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 0);
    let lines = token_lines(&result.stdout);
    let summary = lines
        .iter()
        .copied()
        .find(|line| line.starts_with("SUMMARY "))
        .expect("expected SUMMARY line");
    assert!(
        summary.contains("static_missing=0") && summary.contains("static_bad_image=0"),
        "DLL with satisfied imports should have zero issues.\n{}",
        summary
    );
    assert!(
        !lines.iter().any(
            |line| line.starts_with("STATIC_MISSING ") || line.starts_with("STATIC_BAD_IMAGE ")
        ),
        "should have no missing or bad-image tokens.\n{}",
        result.stdout
    );
}
