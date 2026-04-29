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
fn imports_rejects_x86_root_until_v2() {
    let paths = harness::paths::require_from_env();
    let case = harness::case::TestCase::new(&paths, "imports_rejects_x86_root")
        .expect("failed to initialize test case");
    let image = case.root().join("x86.exe");
    harness::pe_builder::write_import_test_pe_x86(&image, &[]).expect("failed to write x86 PE");

    let args = vec![OsString::from("imports"), harness::case::os(&image)];
    let result =
        harness::run_loadwhat::run_public(&paths, case.root(), &args, Duration::from_secs(20))
            .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 22);
    assert!(
        token_lines(&result.stdout).is_empty(),
        "unsupported x86 imports root should not emit public diagnosis tokens.\n{}",
        result.stdout
    );
}

#[test]
fn run_no_loader_snaps_rejects_x86_target_until_v2() {
    let paths = harness::paths::require_from_env();
    let case = harness::case::TestCase::new(&paths, "run_rejects_x86_no_loader_snaps")
        .expect("failed to initialize test case");
    let image = case.root().join("x86.exe");
    harness::pe_builder::write_import_test_pe_x86(&image, &[]).expect("failed to write x86 PE");

    let args = vec![
        OsString::from("run"),
        OsString::from("--no-loader-snaps"),
        harness::case::os(&image),
    ];
    let result =
        harness::run_loadwhat::run_public(&paths, case.root(), &args, Duration::from_secs(20))
            .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 22);
    assert!(
        token_lines(&result.stdout).is_empty(),
        "unsupported x86 run target should not emit public diagnosis tokens in summary mode.\n{}",
        result.stdout
    );
}

#[test]
fn imports_x64_chain_reports_x86_dependency_as_bad_image() {
    let paths = harness::paths::require_from_env();
    let case = harness::case::TestCase::new(&paths, "imports_x86_dep_bad_image")
        .expect("failed to initialize test case");
    let app_dir = case.mkdir("app").expect("failed to create app directory");
    let root = app_dir.join("root.exe");
    harness::pe_builder::write_import_test_pe(&root, &["wrong_arch.dll"])
        .expect("failed to write x64 root PE");
    harness::pe_builder::write_import_test_pe_x86(&app_dir.join("wrong_arch.dll"), &[])
        .expect("failed to write x86 dependency PE");

    let args = vec![
        OsString::from("imports"),
        harness::case::os(&root),
        OsString::from("--cwd"),
        harness::case::os(&app_dir),
    ];
    let result =
        harness::run_loadwhat::run_public(&paths, case.root(), &args, Duration::from_secs(20))
            .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 10);
    assert!(
        result.stdout.contains(
            r#"STATIC_BAD_IMAGE module="root.exe" dll="wrong_arch.dll" reason="BAD_IMAGE""#
        ),
        "expected wrong-architecture dependency to be reported as bad image.\n{}",
        result.stdout
    );
}
