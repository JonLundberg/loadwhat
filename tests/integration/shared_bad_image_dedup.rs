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
fn imports_shared_bad_image_dep_reported_once() {
    let paths = harness::paths::require_from_env();
    let case = harness::case::TestCase::new(&paths, "imports_shared_bad_image_dedup")
        .expect("failed to initialize test case");

    let dir = case.mkdir("app").expect("failed to create app directory");

    // root.exe -> a.dll, b.dll
    // a.dll -> shared_bad.dll
    // b.dll -> shared_bad.dll
    // shared_bad.dll = junk bytes
    harness::pe_builder::write_import_test_pe(&dir.join("root.exe"), &["a.dll", "b.dll"])
        .expect("failed to write root.exe");
    harness::pe_builder::write_import_test_pe(&dir.join("a.dll"), &["shared_bad.dll"])
        .expect("failed to write a.dll");
    harness::pe_builder::write_import_test_pe(&dir.join("b.dll"), &["shared_bad.dll"])
        .expect("failed to write b.dll");
    fs::write(dir.join("shared_bad.dll"), b"not a pe").expect("failed to write shared_bad.dll");

    let root = dir.join("root.exe");
    let args = vec![
        OsString::from("imports"),
        harness::case::os(&root),
        OsString::from("--cwd"),
        harness::case::os(&dir),
    ];

    // Run twice to verify determinism
    let first =
        harness::run_loadwhat::run_public(&paths, case.root(), &args, Duration::from_secs(20))
            .expect("failed to run first imports command");
    let second =
        harness::run_loadwhat::run_public(&paths, case.root(), &args, Duration::from_secs(20))
            .expect("failed to run second imports command");

    harness::assert::assert_not_timed_out(&first);
    harness::assert::assert_not_timed_out(&second);
    harness::assert::assert_exit_code(&first, 10);
    harness::assert::assert_exit_code(&second, 10);

    let bad_image_lines: Vec<&str> = token_lines(&first.stdout)
        .into_iter()
        .filter(|line| {
            line.starts_with("STATIC_BAD_IMAGE ") && line.contains(r#"dll="shared_bad.dll""#)
        })
        .collect();
    // BadImage is reported per resolution attempt (once per importing parent),
    // so both a.dll and b.dll will trigger a report for shared_bad.dll.
    assert_eq!(
        bad_image_lines.len(),
        2,
        "shared bad-image DLL should be reported once per importing parent.\n{}",
        first.stdout
    );

    // Output should be stable across runs
    assert_eq!(
        token_lines(&first.stdout),
        token_lines(&second.stdout),
        "shared bad-image output should be deterministic across runs"
    );
}
