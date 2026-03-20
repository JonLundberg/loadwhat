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
                    || line.starts_with("SEARCH_")
                    || line.starts_with("SUMMARY")
                    || line.starts_with("NOTE ")
                    || line.starts_with("RUN_")
                    || line.starts_with("RUNTIME_")
                    || line.starts_with("DEBUG_STRING")
                    || line.starts_with("DYNAMIC_")
                    || line.starts_with("FIRST_BREAK"))
        })
        .collect()
}

#[test]
fn imports_malformed_root_fails_cleanly_without_diagnosis_tokens() {
    let paths = harness::paths::require_from_env();
    let case = harness::case::TestCase::new(&paths, "imports_malformed_root")
        .expect("failed to initialize test case");
    let broken = case.root().join("broken.exe");
    fs::write(&broken, b"not a pe file").expect("failed to write malformed root");

    let args = vec![OsString::from("imports"), harness::case::os(&broken)];
    let result =
        harness::run_loadwhat::run_public(&paths, case.root(), &args, Duration::from_secs(20))
            .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 21);
    assert!(
        !token_lines(&result.stdout).iter().any(|line| {
            line.starts_with("STATIC_MISSING ")
                || line.starts_with("STATIC_BAD_IMAGE ")
                || line.starts_with("SUMMARY ")
                || line.starts_with("DYNAMIC_")
        }),
        "malformed-root failure should not invent diagnosis or summary tokens.\n{}",
        result.stdout
    );
}

#[test]
fn imports_zero_import_binary_reports_zero_summary_and_no_runtime_tokens() {
    let paths = harness::paths::require_from_env();
    let case = harness::case::TestCase::new(&paths, "imports_zero_imports")
        .expect("failed to initialize test case");
    let zero = case.root().join("zero.exe");
    harness::pe_builder::write_import_test_pe(&zero, &[])
        .expect("failed to write zero-import test image");

    let args = vec![OsString::from("imports"), harness::case::os(&zero)];
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
        .expect("missing SUMMARY");
    assert!(
        summary.contains("first_break=false")
            && summary.contains("static_missing=0")
            && summary.contains("static_bad_image=0")
            && summary.contains("dynamic_missing=0")
            && summary.contains("runtime_loaded=0"),
        "unexpected zero-import SUMMARY.\n{}",
        summary
    );
    assert!(
        !lines.iter().any(|line| {
            line.starts_with("RUN_START ")
                || line.starts_with("RUNTIME_LOADED ")
                || line.starts_with("DEBUG_STRING ")
                || line.starts_with("RUN_END ")
                || line.starts_with("FIRST_BREAK ")
                || line.starts_with("DYNAMIC_")
                || line.starts_with("NOTE ")
        }),
        "zero-import imports output should remain static-only and omit the unmodeled note by default.\n{}",
        result.stdout
    );
}

#[test]
fn imports_static_import_order_is_lexicographic_and_stable() {
    let paths = harness::paths::require_from_env();
    let case = harness::case::TestCase::new(&paths, "imports_sorted_order")
        .expect("failed to initialize test case");
    let image = case.root().join("ordered.exe");
    harness::pe_builder::write_import_test_pe(&image, &["z.dll", "Kernel32.dll", "a.dll"])
        .expect("failed to write ordered import image");

    let args = vec![OsString::from("imports"), harness::case::os(&image)];
    let first =
        harness::run_loadwhat::run_public(&paths, case.root(), &args, Duration::from_secs(20))
            .expect("failed to run first imports command");
    let second =
        harness::run_loadwhat::run_public(&paths, case.root(), &args, Duration::from_secs(20))
            .expect("failed to run second imports command");

    harness::assert::assert_not_timed_out(&first);
    harness::assert::assert_not_timed_out(&second);
    let first_imports: Vec<&str> = first
        .stdout
        .lines()
        .map(|line| line.trim())
        .filter(|line| line.starts_with("STATIC_IMPORT "))
        .collect();
    let second_imports: Vec<&str> = second
        .stdout
        .lines()
        .map(|line| line.trim())
        .filter(|line| line.starts_with("STATIC_IMPORT "))
        .collect();
    assert_eq!(
        first_imports, second_imports,
        "imports output changed between runs"
    );
    assert!(
        first_imports.starts_with(&[
            r#"STATIC_IMPORT module="ordered.exe" needs="a.dll""#,
            r#"STATIC_IMPORT module="ordered.exe" needs="kernel32.dll""#,
            r#"STATIC_IMPORT module="ordered.exe" needs="z.dll""#,
        ]),
        "unexpected root STATIC_IMPORT ordering.\n{}",
        first.stdout
    );
}
