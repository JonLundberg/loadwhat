use std::collections::HashMap;
use std::path::Path;

use super::run_loadwhat::RunResult;

pub fn assert_not_timed_out(result: &RunResult) {
    assert!(
        !result.timed_out,
        "loadwhat command timed out.\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
}

pub fn assert_exit_code(result: &RunResult, expected: i32) {
    assert_eq!(
        result.code,
        Some(expected),
        "unexpected loadwhat exit code.\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
}

pub fn assert_missing_dll(stdout: &str, dll_name: &str) {
    let expected = dll_name.to_ascii_lowercase();
    let found = lwtest_lines(stdout).iter().any(|line| {
        if !line.starts_with("LWTEST:RESULT ") {
            return false;
        }
        let fields = parse_fields(line);
        field_eq(&fields, "kind", "missing_dll") && field_eq(&fields, "name", &expected)
    });

    assert!(
        found,
        "expected missing-dll result for {}.\nstdout:\n{}",
        dll_name,
        stdout
    );
}

pub fn assert_no_missing_result(stdout: &str) {
    let found = lwtest_lines(stdout).iter().any(|line| {
        if !line.starts_with("LWTEST:RESULT ") {
            return false;
        }
        let fields = parse_fields(line);
        field_eq(&fields, "kind", "missing_dll")
    });

    assert!(!found, "unexpected missing-dll result.\nstdout:\n{}", stdout);
}

pub fn assert_target_exit_code(stdout: &str, expected: i32) {
    let found = lwtest_lines(stdout).iter().any(|line| {
        if !line.starts_with("LWTEST:TARGET ") {
            return false;
        }
        let fields = parse_fields(line);
        fields
            .get("exit_code")
            .map(|v| v.parse::<i32>().ok() == Some(expected))
            .unwrap_or(false)
    });

    assert!(
        found,
        "expected LWTEST:TARGET exit_code={}.\nstdout:\n{}",
        expected,
        stdout
    );
}

pub fn assert_loaded_path(stdout: &str, dll_name: &str, expected_path: &Path) {
    let expected_name = dll_name.to_ascii_lowercase();
    let expected_path = normalize_path_value(&expected_path.display().to_string());
    let found = lwtest_lines(stdout).iter().any(|line| {
        if !line.starts_with("LWTEST:LOAD ") {
            return false;
        }
        let fields = parse_fields(line);
        if !field_eq(&fields, "name", &expected_name) {
            return false;
        }
        fields
            .get("path")
            .map(|v| normalize_path_value(v) == expected_path)
            .unwrap_or(false)
    });

    assert!(
        found,
        "expected LWTEST:LOAD name={} path={}.\nstdout:\n{}",
        dll_name,
        expected_path,
        stdout
    );
}

pub fn lwtest_lines(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .map(|line| line.trim())
        .filter(|line| line.starts_with("LWTEST:"))
        .map(|line| line.to_string())
        .collect()
}

fn parse_fields(line: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for part in line.split_whitespace().skip(1) {
        let Some((key, value)) = part.split_once('=') else {
            continue;
        };
        out.insert(key.to_string(), normalize_value(value));
    }
    out
}

fn normalize_value(value: &str) -> String {
    value.trim_matches('"').to_ascii_lowercase()
}

fn normalize_path_value(value: &str) -> String {
    normalize_value(value).replace('/', "\\")
}

fn field_eq(fields: &HashMap<String, String>, key: &str, expected: &str) -> bool {
    fields
        .get(key)
        .map(|v| v == &expected.to_ascii_lowercase())
        .unwrap_or(false)
}

