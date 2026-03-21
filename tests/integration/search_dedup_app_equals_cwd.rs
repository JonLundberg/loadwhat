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

fn quoted_field_value<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!(r#"{key}=""#);
    let start = line.find(&needle)? + needle.len();
    let rest = &line[start..];
    let end = rest.find('"')?;
    Some(&rest[..end])
}

#[test]
fn imports_app_dir_equals_cwd_no_duplicate_search_paths() {
    let paths = harness::paths::require_from_env();
    let case = harness::case::TestCase::new(&paths, "search_dedup_app_cwd")
        .expect("failed to initialize test case");

    let dir = case.mkdir("app").expect("failed to create app directory");

    let root = dir.join("root.exe");
    harness::pe_builder::write_import_test_pe(&root, &["missing.dll"])
        .expect("failed to write root.exe");

    // Set --cwd to the same directory as the exe (app_dir == cwd)
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

    // Collect all SEARCH_PATH lines for missing.dll and extract the path values
    let search_paths: Vec<&str> = token_lines(&result.stdout)
        .into_iter()
        .filter(|line| {
            line.starts_with("SEARCH_PATH ") && line.contains(r#"dll="missing.dll""#)
        })
        .collect();

    let path_values: Vec<String> = search_paths
        .iter()
        .filter_map(|line| quoted_field_value(line, "path"))
        .map(|p| harness::win_path::normalize_for_compare(p))
        .collect();

    // Check for duplicates: each normalized path should appear at most once
    let mut seen = std::collections::HashSet::new();
    for path in &path_values {
        assert!(
            seen.insert(path.as_str()),
            "duplicate search path found: {}\nall search paths:\n{}",
            path,
            search_paths.join("\n")
        );
    }
}
