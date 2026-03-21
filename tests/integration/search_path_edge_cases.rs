use crate::harness;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
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

fn path_env_value(extra_dirs: &[&Path]) -> String {
    let mut dirs: Vec<PathBuf> = extra_dirs.iter().map(|dir| dir.to_path_buf()).collect();
    if let Some(existing) = env::var_os("PATH") {
        dirs.extend(env::split_paths(&existing));
    }
    env::join_paths(dirs)
        .expect("failed to join PATH entries")
        .to_string_lossy()
        .into_owned()
}

#[test]
fn imports_path_with_empty_segments_does_not_crash() {
    let paths = harness::paths::require_from_env();
    let case = harness::case::TestCase::new(&paths, "search_empty_path_segments")
        .expect("failed to initialize test case");

    let dir = case.mkdir("app").expect("failed to create app directory");
    let path_a = case.mkdir("path_a").expect("failed to create path_a");
    let path_b = case.mkdir("path_b").expect("failed to create path_b");

    let root = dir.join("root.exe");
    harness::pe_builder::write_import_test_pe(&root, &["lwtest_a.dll"])
        .expect("failed to write root.exe");

    // Place the DLL only in path_b
    harness::pe_builder::write_import_test_pe(&path_b.join("lwtest_a.dll"), &[])
        .expect("failed to write lwtest_a.dll");

    // Build PATH with an empty segment between path_a and path_b
    let custom_path = format!(
        "{};;{}",
        path_a.display(),
        path_b.display()
    );
    // Append the system PATH so kernel32.dll etc. are still found
    let full_path = if let Some(existing) = env::var_os("PATH") {
        format!("{};{}", custom_path, existing.to_string_lossy())
    } else {
        custom_path
    };

    let args = vec![
        OsString::from("imports"),
        harness::case::os(&root),
        OsString::from("--cwd"),
        harness::case::os(&dir),
    ];
    let result = harness::run_loadwhat::run_public_with_env(
        &paths,
        case.root(),
        &args,
        Duration::from_secs(20),
        &[("PATH", &full_path)],
    )
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
        summary.contains("static_missing=0"),
        "DLL should be found via path_b despite empty PATH segment.\n{}",
        summary
    );
}

#[test]
fn imports_bad_image_in_search_path_stops_search() {
    let paths = harness::paths::require_from_env();
    let case = harness::case::TestCase::new(&paths, "search_bad_image_stops")
        .expect("failed to initialize test case");

    let dir = case.mkdir("app").expect("failed to create app directory");
    let early_dir = case.mkdir("early").expect("failed to create early directory");
    let late_dir = case.mkdir("late").expect("failed to create late directory");

    let root = dir.join("root.exe");
    harness::pe_builder::write_import_test_pe(&root, &["target.dll"])
        .expect("failed to write root.exe");

    // Early directory has a bad image
    fs::write(early_dir.join("target.dll"), b"not pe")
        .expect("failed to write bad image target.dll");

    // Late directory has a valid PE
    harness::pe_builder::write_import_test_pe(&late_dir.join("target.dll"), &[])
        .expect("failed to write valid target.dll");

    // PATH: early before late
    let custom_path = path_env_value(&[early_dir.as_path(), late_dir.as_path()]);

    let args = vec![
        OsString::from("imports"),
        harness::case::os(&root),
        OsString::from("--cwd"),
        harness::case::os(&dir),
    ];
    let result = harness::run_loadwhat::run_public_with_env(
        &paths,
        case.root(),
        &args,
        Duration::from_secs(20),
        &[("PATH", &custom_path)],
    )
    .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 10);
    let lines = token_lines(&result.stdout);
    assert!(
        lines.iter().any(|line| line.starts_with("STATIC_BAD_IMAGE ")
            && line.contains(r#"dll="target.dll""#)),
        "bad image in early path position should be reported.\n{}",
        result.stdout
    );
    assert!(
        !lines.iter().any(|line| line.starts_with("STATIC_FOUND ")
            && line.contains(r#"dll="target.dll""#)),
        "valid copy later in search should NOT be found — search stops at bad image.\n{}",
        result.stdout
    );
}
