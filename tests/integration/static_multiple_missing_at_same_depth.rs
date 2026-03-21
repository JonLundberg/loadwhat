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
fn imports_multiple_missing_at_same_depth_selects_lexicographic_first() {
    let paths = harness::paths::require_from_env();
    let case = harness::case::TestCase::new(&paths, "imports_multi_missing_same_depth")
        .expect("failed to initialize test case");

    let dir = case.mkdir("app").expect("failed to create app directory");

    // root.exe imports two DLLs, both missing. "a_missing.dll" should be selected
    // as first issue because it's lexicographically before "z_missing.dll".
    let root = dir.join("root.exe");
    harness::pe_builder::write_import_test_pe(&root, &["z_missing.dll", "a_missing.dll"])
        .expect("failed to write root.exe");

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

    // The imports command uses Full emit mode, so both STATIC_MISSING lines should appear.
    let missing_lines: Vec<&str> = token_lines(&result.stdout)
        .into_iter()
        .filter(|line| line.starts_with("STATIC_MISSING "))
        .collect();
    assert!(
        missing_lines.len() >= 2,
        "expected at least two STATIC_MISSING lines.\n{}",
        result.stdout
    );
    assert!(
        missing_lines
            .iter()
            .any(|line| line.contains(r#"dll="a_missing.dll""#)),
        "expected STATIC_MISSING for a_missing.dll.\n{}",
        result.stdout
    );
    assert!(
        missing_lines
            .iter()
            .any(|line| line.contains(r#"dll="z_missing.dll""#)),
        "expected STATIC_MISSING for z_missing.dll.\n{}",
        result.stdout
    );

    // Verify the SUMMARY reports both missing
    let summary = token_lines(&result.stdout)
        .into_iter()
        .find(|line| line.starts_with("SUMMARY "))
        .expect("expected SUMMARY line");
    assert!(
        summary.contains("static_missing=2"),
        "SUMMARY should report static_missing=2.\n{}",
        summary
    );
}
