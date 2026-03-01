use crate::harness;
use std::ffi::OsString;
use std::time::Duration;

#[test]
fn imports_reports_transitive_missing_with_via_and_depth() {
    let Some(paths) = harness::paths::from_env() else {
        return;
    };

    let case = harness::case::TestCase::new(&paths, "imports_transitive_missing")
        .expect("failed to initialize test case");
    let app_dir = case.mkdir("app").expect("failed to create app directory");
    let exe = case
        .copy_fixture(
            harness::fixture::HOST_STATIC_A_DEPENDS_ON_B_EXE,
            "app\\host_static_a_depends_on_b.exe",
        )
        .expect("failed to copy host fixture");
    case.copy_fixture_as(harness::fixture::DLL_LWTEST_A, "app", "lwtest_a.dll")
        .expect("failed to copy lwtest_a.dll fixture");

    let args = vec![
        OsString::from("imports"),
        harness::case::os(&exe),
        OsString::from("--cwd"),
        harness::case::os(&app_dir),
    ];
    let result = harness::run_loadwhat::run(&paths, case.root(), &args, Duration::from_secs(20))
        .expect("failed to run loadwhat");

    harness::assert::assert_not_timed_out(&result);
    harness::assert::assert_exit_code(&result, 10);

    let found = result.stdout.lines().map(|line| line.trim()).any(|line| {
        line.starts_with("STATIC_MISSING ")
            && line.contains(r#"dll="lwtest_b.dll""#)
            && line.contains(r#"via="lwtest_a.dll""#)
            && line.contains("depth=2")
    });
    assert!(
        found,
        "expected transitive STATIC_MISSING for lwtest_b.dll with via/depth.\nstdout:\n{}",
        result.stdout
    );
}
