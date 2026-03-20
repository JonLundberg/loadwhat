use crate::harness;
use std::ffi::OsString;
use std::time::Duration;

#[test]
fn shared_dependency_graph_walk_is_stable_and_deduplicated() {
    let paths = harness::paths::require_from_env();
    let case = harness::case::TestCase::new(&paths, "shared_dependency_graph")
        .expect("failed to initialize test case");
    let app_dir = case.mkdir("app").expect("failed to create app directory");
    let root = app_dir.join("root.exe");
    let a = app_dir.join("a.dll");
    let c = app_dir.join("c.dll");
    let shared = app_dir.join("shared.dll");

    harness::pe_builder::write_import_test_pe(&root, &["a.dll", "c.dll"])
        .expect("failed to write root image");
    harness::pe_builder::write_import_test_pe(&a, &["shared.dll"]).expect("failed to write a.dll");
    harness::pe_builder::write_import_test_pe(&c, &["shared.dll"]).expect("failed to write c.dll");
    harness::pe_builder::write_import_test_pe(&shared, &["leaf.dll"])
        .expect("failed to write shared.dll");

    let args = vec![
        OsString::from("imports"),
        harness::case::os(&root),
        OsString::from("--cwd"),
        harness::case::os(&app_dir),
    ];
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
    assert_eq!(
        first.stdout, second.stdout,
        "shared graph output changed between runs"
    );
    assert_eq!(
        first
            .stdout
            .matches(r#"STATIC_IMPORT module="shared.dll" needs="leaf.dll""#)
            .count(),
        1,
        "shared dependency should be scanned once.\n{}",
        first.stdout
    );
    assert_eq!(
        first
            .stdout
            .matches(r#"STATIC_MISSING module="shared.dll" dll="leaf.dll" reason="NOT_FOUND""#)
            .count(),
        1,
        "missing leaf dependency should be reported once.\n{}",
        first.stdout
    );
}
