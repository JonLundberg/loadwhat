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
fn imports_truncated_pe_header_fails_cleanly() {
    let paths = harness::paths::require_from_env();
    let case = harness::case::TestCase::new(&paths, "imports_truncated_pe_header")
        .expect("failed to initialize test case");

    let mut bytes = harness::pe_builder::build_import_test_pe(&["kernel32.dll"]);
    bytes.truncate(96); // Past MZ header (0x40) but before PE header at 0x80
    let truncated = case.root().join("truncated.exe");
    fs::write(&truncated, &bytes).expect("failed to write truncated PE");

    let args = vec![OsString::from("imports"), harness::case::os(&truncated)];
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
        "truncated PE should not produce diagnosis or summary tokens.\n{}",
        result.stdout
    );
}

#[test]
fn imports_corrupted_import_table_rva_fails_cleanly() {
    let paths = harness::paths::require_from_env();
    let case = harness::case::TestCase::new(&paths, "imports_corrupted_import_rva")
        .expect("failed to initialize test case");

    let mut bytes = harness::pe_builder::build_import_test_pe(&["kernel32.dll"]);
    // IMPORT_DIRECTORY_RVA_OFFSET = DATA_DIR_START + 8 = (0x98 + 112) + 8 = 0x110
    // Overwrite import directory RVA with an unmappable value.
    let rva_offset = 0x110;
    bytes[rva_offset..rva_offset + 4].copy_from_slice(&0xFFFF_0000u32.to_le_bytes());

    let bad_rva = case.root().join("bad_rva.exe");
    fs::write(&bad_rva, &bytes).expect("failed to write corrupted PE");

    let args = vec![OsString::from("imports"), harness::case::os(&bad_rva)];
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
        "corrupted import RVA should not produce diagnosis or summary tokens.\n{}",
        result.stdout
    );
}

#[test]
fn imports_transitive_non_pe_file_reports_bad_image() {
    let paths = harness::paths::require_from_env();
    let case = harness::case::TestCase::new(&paths, "imports_transitive_non_pe")
        .expect("failed to initialize test case");

    let dir = case.mkdir("app").expect("failed to create app directory");

    let root = dir.join("root.exe");
    harness::pe_builder::write_import_test_pe(&root, &["good.dll"])
        .expect("failed to write root.exe");

    let good = dir.join("good.dll");
    harness::pe_builder::write_import_test_pe(&good, &["corrupt.dll"])
        .expect("failed to write good.dll");

    // Write junk bytes — is_probably_pe_file returns false, so search classifies
    // it as BadImage. A structurally valid PE with a corrupted import RVA would
    // pass is_probably_pe_file and cause direct_imports to Err, aborting the
    // entire diagnose_static_imports call, so we use plain non-PE bytes instead.
    fs::write(dir.join("corrupt.dll"), b"definitely not a PE file")
        .expect("failed to write corrupt.dll");

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
    assert!(
        lines
            .iter()
            .any(|line| line.starts_with("STATIC_BAD_IMAGE ")
                && line.contains(r#"dll="corrupt.dll""#)),
        "transitive non-PE file should be reported as bad image.\n{}",
        result.stdout
    );
}

#[test]
fn run_malformed_target_exe_exits_cleanly() {
    let paths = harness::paths::require_from_env();
    let case = harness::case::TestCase::new(&paths, "run_malformed_target")
        .expect("failed to initialize test case");

    let broken = case.root().join("broken.exe");
    fs::write(&broken, b"this is definitely not a PE file")
        .expect("failed to write malformed exe");

    let args = vec![OsString::from("run"), harness::case::os(&broken)];
    let result =
        harness::run_loadwhat::run_public(&paths, case.root(), &args, Duration::from_secs(20))
            .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 21);
    assert!(
        !token_lines(&result.stdout).iter().any(|line| {
            line.starts_with("STATIC_MISSING ")
                || line.starts_with("STATIC_BAD_IMAGE ")
                || line.starts_with("DYNAMIC_")
        }),
        "malformed target should not produce DLL diagnosis tokens.\n{}",
        result.stdout
    );
}

#[test]
fn imports_junk_bytes_dll_in_chain_reports_bad_image() {
    let paths = harness::paths::require_from_env();
    let case = harness::case::TestCase::new(&paths, "imports_junk_dll_bad_image")
        .expect("failed to initialize test case");

    let dir = case.mkdir("app").expect("failed to create app directory");

    let root = dir.join("root.exe");
    harness::pe_builder::write_import_test_pe(&root, &["lwtest_a.dll"])
        .expect("failed to write root.exe");

    fs::write(dir.join("lwtest_a.dll"), b"not a pe image")
        .expect("failed to write junk dll");

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
    assert!(
        lines.iter().any(|line| line.starts_with("STATIC_BAD_IMAGE ")
            && line.contains(r#"dll="lwtest_a.dll""#)
            && line.contains(r#"reason="BAD_IMAGE""#)),
        "junk bytes DLL should be reported as bad image.\n{}",
        result.stdout
    );
    assert!(
        !lines.iter().any(|line| line.starts_with("DYNAMIC_")),
        "imports command should not produce dynamic tokens.\n{}",
        result.stdout
    );
}
