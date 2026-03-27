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
fn imports_deep_transitive_chain_reports_correct_depth() {
    let paths = harness::paths::require_from_env();
    let case = harness::case::TestCase::new(&paths, "imports_deep_chain")
        .expect("failed to initialize test case");

    let dir = case.mkdir("app").expect("failed to create app directory");

    // root.exe -> a.dll -> b.dll -> c.dll -> d.dll -> missing.dll
    harness::pe_builder::write_import_test_pe(&dir.join("root.exe"), &["a.dll"])
        .expect("failed to write root.exe");
    harness::pe_builder::write_import_test_pe(&dir.join("a.dll"), &["b.dll"])
        .expect("failed to write a.dll");
    harness::pe_builder::write_import_test_pe(&dir.join("b.dll"), &["c.dll"])
        .expect("failed to write b.dll");
    harness::pe_builder::write_import_test_pe(&dir.join("c.dll"), &["d.dll"])
        .expect("failed to write c.dll");
    harness::pe_builder::write_import_test_pe(&dir.join("d.dll"), &["missing.dll"])
        .expect("failed to write d.dll");
    // Do NOT create missing.dll

    let root = dir.join("root.exe");
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
        missing_line.contains(r#"via="d.dll""#),
        "STATIC_MISSING should report via=\"d.dll\".\n{}",
        missing_line
    );
    assert!(
        missing_line.contains("depth=5"),
        "STATIC_MISSING should report depth=5 for a 5-level chain.\n{}",
        missing_line
    );
}
