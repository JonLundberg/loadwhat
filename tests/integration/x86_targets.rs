use crate::harness;
use crate::harness::case::{os, TestCase};
use crate::harness::fixture::{self, x86};
use crate::harness::paths::HarnessPaths;
use crate::harness::run_loadwhat::{run_public, run_public_with_env, RunResult};
use std::ffi::OsString;
use std::path::Path;
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(20);

fn run_args(cwd: &Path, exe: &Path, flags: &[&str]) -> Vec<OsString> {
    let mut args = vec![OsString::from("run")];
    args.extend(flags.iter().map(OsString::from));
    args.push(OsString::from("--cwd"));
    args.push(os(cwd));
    args.push(os(exe));
    args
}

fn run_target(paths: &HarnessPaths, case: &TestCase, args: &[OsString]) -> RunResult {
    let result = run_public(paths, case.root(), args, TIMEOUT).expect("failed to run loadwhat");
    harness::assert::assert_not_timed_out(&result);
    result
}

/// Sets PATH for the target to `dir` followed by the inherited value.
fn run_with_path_prefix(
    paths: &HarnessPaths,
    case: &TestCase,
    args: &[OsString],
    dir: &Path,
) -> RunResult {
    let path = format!(
        "{};{}",
        dir.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let result = run_public_with_env(paths, case.root(), args, TIMEOUT, &[("PATH", &path)])
        .expect("failed to run loadwhat");
    harness::assert::assert_not_timed_out(&result);
    result
}

fn assert_line(stdout: &str, prefix: &str, needles: &[&str]) {
    let found = stdout
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with(prefix))
        .any(|line| needles.iter().all(|needle| line.contains(needle)));
    assert!(
        found,
        "expected a {prefix} line containing {needles:?}.\n{stdout}"
    );
}

fn search_paths(stdout: &str) -> Vec<&str> {
    stdout
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("SEARCH_PATH "))
        .collect()
}

fn loadwhat_lines(stdout: &str) -> Vec<&str> {
    stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("HOST:"))
        .collect()
}

#[test]
fn x86_target_success_with_dependencies_present() {
    let paths = harness::paths::require_from_env();
    let case = TestCase::new(&paths, "x86_success").expect("failed to initialize test case");
    let app = case.mkdir("app").expect("failed to create app directory");
    let exe = case
        .copy_fixture(
            &x86(fixture::HOST_STATIC_IMPORTS_A_EXE),
            r"app\host_static_imports_a.exe",
        )
        .expect("failed to copy host fixture");
    case.copy_fixture_as(&x86(fixture::DLL_LWTEST_A), "app", "lwtest_a.dll")
        .expect("failed to copy lwtest_a.dll fixture");
    case.copy_fixture_as(&x86(fixture::DLL_LWTEST_B), "app", "lwtest_b.dll")
        .expect("failed to copy lwtest_b.dll fixture");

    let result = run_target(&paths, &case, &run_args(&app, &exe, &[]));

    harness::assert::assert_exit_code(&result, 0);
    assert_line(&result.stdout, "SUCCESS", &["status=0"]);
}

#[test]
fn x86_target_reports_static_missing_direct() {
    let paths = harness::paths::require_from_env();
    let case = TestCase::new(&paths, "x86_missing").expect("failed to initialize test case");
    let app = case.mkdir("app").expect("failed to create app directory");
    let exe = case
        .copy_fixture(
            &x86(fixture::HOST_STATIC_IMPORTS_MISSING_EXE),
            r"app\host_static_imports_missing.exe",
        )
        .expect("failed to copy host fixture");

    let result = run_target(&paths, &case, &run_args(&app, &exe, &[]));

    harness::assert::assert_exit_code(&result, 10);
    assert_line(
        &result.stdout,
        "STATIC_MISSING",
        &[r#"dll="lwtest_a.dll""#, r#"reason="NOT_FOUND""#],
    );
}

#[test]
fn x86_target_reports_static_missing_transitive() {
    let paths = harness::paths::require_from_env();
    let case = TestCase::new(&paths, "x86_transitive").expect("failed to initialize test case");
    let app = case.mkdir("app").expect("failed to create app directory");
    let exe = case
        .copy_fixture(
            &x86(fixture::HOST_STATIC_A_DEPENDS_ON_B_EXE),
            r"app\host_static_a_depends_on_b.exe",
        )
        .expect("failed to copy host fixture");
    case.copy_fixture_as(&x86(fixture::DLL_LWTEST_A), "app", "lwtest_a.dll")
        .expect("failed to copy lwtest_a.dll fixture");

    let result = run_target(&paths, &case, &run_args(&app, &exe, &[]));

    harness::assert::assert_exit_code(&result, 10);
    assert_line(&result.stdout, "STATIC_MISSING", &[r#"dll="lwtest_b.dll""#]);
}

#[test]
fn x86_target_reports_dynamic_missing_through_loader_snaps() {
    let paths = harness::paths::require_from_env();
    let case = TestCase::new(&paths, "x86_dynamic").expect("failed to initialize test case");
    let app = case.mkdir("app").expect("failed to create app directory");
    let exe = case
        .copy_fixture(
            &x86(fixture::HOST_DYNAMIC_LOADLIBRARY_NAME_EXE),
            r"app\host_dynamic_loadlibrary_name.exe",
        )
        .expect("failed to copy host fixture");

    let result = run_target(&paths, &case, &run_args(&app, &exe, &[]));

    harness::assert::assert_exit_code(&result, 10);
    assert_line(
        &result.stdout,
        "DYNAMIC_MISSING",
        &[r#"dll="lwtest_a.dll""#, r#"reason="NOT_FOUND""#],
    );
}

#[test]
fn dynamic_wrong_machine_probe_preserves_bad_image_diagnosis() {
    let paths = harness::paths::require_from_env();
    for (arch, host, wrong_dll) in [
        (
            "x86",
            x86(fixture::HOST_DYNAMIC_LOADLIBRARY_FULLPATH_EXE),
            fixture::DLL_LWTEST_B.to_string(),
        ),
        (
            "x64",
            fixture::HOST_DYNAMIC_LOADLIBRARY_FULLPATH_EXE.to_string(),
            x86(fixture::DLL_LWTEST_B),
        ),
    ] {
        let case = TestCase::new(&paths, &format!("{arch}_dynamic_wrong_machine"))
            .expect("failed to initialize test case");
        let app = case.mkdir("app").expect("failed to create app directory");
        let exe = case
            .copy_fixture(&host, r"app\host.exe")
            .expect("failed to copy host fixture");
        let wrong = case
            .copy_fixture(&wrong_dll, r"wrong\lwtest_b.dll")
            .expect("failed to copy wrong-machine DLL");
        let mut args = run_args(&app, &exe, &[]);
        args.push(os(&wrong));
        let first = run_target(&paths, &case, &args);
        let second = run_target(&paths, &case, &args);

        for result in [&first, &second] {
            harness::assert::assert_exit_code(result, 10);
            assert_eq!(loadwhat_lines(&result.stdout).len(), 1, "{}", result.stdout);
            assert_line(
                &result.stdout,
                "DYNAMIC_MISSING",
                &[
                    r#"dll="lwtest_b.dll""#,
                    r#"reason="BAD_IMAGE""#,
                    "status=0xC000007B",
                ],
            );
        }
        assert_eq!(first.stdout, second.stdout);

        let mut args = run_args(&app, &exe, &["-v"]);
        args.push(os(&wrong));
        let verbose = run_target(&paths, &case, &args);
        harness::assert::assert_exit_code(&verbose, 10);
        assert_line(
            &verbose.stdout,
            "RUNTIME_LOADED",
            &[r#"dll="lwtest_b.dll""#],
        );
        assert_line(
            &verbose.stdout,
            "DYNAMIC_MISSING",
            &[r#"dll="lwtest_b.dll""#, r#"reason="BAD_IMAGE""#],
        );
        assert_line(&verbose.stdout, "SUMMARY", &["dynamic_missing=1"]);
    }
}

#[test]
fn dynamic_wrong_machine_probe_is_cleared_by_later_compatible_load() {
    let paths = harness::paths::require_from_env();
    let case = TestCase::new(&paths, "x86_dynamic_wrong_machine_retry")
        .expect("failed to initialize test case");
    let app = case.mkdir("app").expect("failed to create app directory");
    let exe = case
        .copy_fixture(
            &x86(fixture::HOST_DYNAMIC_LOADLIBRARY_SEQUENCE_EXE),
            r"app\host.exe",
        )
        .expect("failed to copy host fixture");
    let wrong = case
        .copy_fixture(fixture::DLL_LWTEST_B, r"wrong\lwtest_b.dll")
        .expect("failed to copy wrong-machine DLL");
    let right = case
        .copy_fixture(&x86(fixture::DLL_LWTEST_B), r"right\lwtest_b.dll")
        .expect("failed to copy compatible DLL");
    let mut args = run_args(&app, &exe, &[]);
    args.push(OsString::from(format!("optional:{}", wrong.display())));
    args.push(os(&right));

    let result = run_target(&paths, &case, &args);

    harness::assert::assert_exit_code(&result, 0);
    assert_eq!(loadwhat_lines(&result.stdout), vec!["SUCCESS status=0"]);
}

#[test]
fn x86_target_verbose_reports_peb32_offset_and_both_ntdlls() {
    let paths = harness::paths::require_from_env();
    let case = TestCase::new(&paths, "x86_verbose").expect("failed to initialize test case");
    let app = case.mkdir("app").expect("failed to create app directory");
    let exe = case
        .copy_fixture(
            &x86(fixture::HOST_STATIC_IMPORTS_A_EXE),
            r"app\host_static_imports_a.exe",
        )
        .expect("failed to copy host fixture");
    case.copy_fixture_as(&x86(fixture::DLL_LWTEST_A), "app", "lwtest_a.dll")
        .expect("failed to copy lwtest_a.dll fixture");
    case.copy_fixture_as(&x86(fixture::DLL_LWTEST_B), "app", "lwtest_b.dll")
        .expect("failed to copy lwtest_b.dll fixture");

    let result = run_target(&paths, &case, &run_args(&app, &exe, &["-v"]));

    harness::assert::assert_exit_code(&result, 0);
    assert_line(
        &result.stdout,
        "NOTE",
        &[r#"detail="peb-ntglobalflag""#, "ntglobalflag_offset=0x68"],
    );
    assert_line(&result.stdout, "RUNTIME_LOADED", &["System32", "ntdll.dll"]);
    assert_line(&result.stdout, "RUNTIME_LOADED", &["SysWOW64", "ntdll.dll"]);
    assert_line(&result.stdout, "DEBUG_STRING", &["LdrpInitializeProcess"]);
}

#[test]
fn x86_target_searches_syswow64_instead_of_system32() {
    let paths = harness::paths::require_from_env();
    let case = TestCase::new(&paths, "x86_search").expect("failed to initialize test case");
    let app = case.mkdir("app").expect("failed to create app directory");
    let exe = case
        .copy_fixture(
            &x86(fixture::HOST_STATIC_IMPORTS_MISSING_EXE),
            r"app\host_static_imports_missing.exe",
        )
        .expect("failed to copy host fixture");

    let result = run_target(&paths, &case, &run_args(&app, &exe, &["--trace"]));

    harness::assert::assert_exit_code(&result, 10);
    let lines = search_paths(&result.stdout);
    assert!(
        lines.iter().any(|line| line
            .to_ascii_lowercase()
            .contains(r"\\syswow64\\lwtest_a.dll")),
        "expected a SysWOW64 candidate.\n{}",
        result.stdout
    );
    assert!(
        !lines
            .iter()
            .any(|line| line.to_ascii_lowercase().contains(r"\\system32\\")),
        "a 32-bit process cannot search System32.\n{}",
        result.stdout
    );
}

#[test]
fn x86_target_with_only_x64_dependency_reports_bad_image() {
    let paths = harness::paths::require_from_env();
    let case = TestCase::new(&paths, "x86_wrong_only").expect("failed to initialize test case");
    let app = case.mkdir("app").expect("failed to create app directory");
    let exe = case
        .copy_fixture(
            &x86(fixture::HOST_STATIC_IMPORTS_A_EXE),
            r"app\host_static_imports_a.exe",
        )
        .expect("failed to copy host fixture");
    case.copy_fixture_as(fixture::DLL_LWTEST_A, "app", "lwtest_a.dll")
        .expect("failed to copy x64 lwtest_a.dll fixture");

    let result = run_target(&paths, &case, &run_args(&app, &exe, &["--trace"]));

    harness::assert::assert_exit_code(&result, 10);
    assert_line(
        &result.stdout,
        "STATIC_BAD_IMAGE",
        &[r#"dll="lwtest_a.dll""#, r#"reason="BAD_IMAGE""#],
    );
    assert_line(
        &result.stdout,
        "SEARCH_PATH",
        &[r#"dll="lwtest_a.dll""#, r#"result="WRONG_MACHINE""#],
    );
}

#[test]
fn x86_target_skips_x64_dependency_like_the_loader() {
    let paths = harness::paths::require_from_env();
    let case = TestCase::new(&paths, "x86_wrong_skip").expect("failed to initialize test case");
    let app = case.mkdir("app").expect("failed to create app directory");
    let right = case
        .mkdir("right")
        .expect("failed to create PATH directory");
    let exe = case
        .copy_fixture(
            &x86(fixture::HOST_STATIC_IMPORTS_A_EXE),
            r"app\host_static_imports_a.exe",
        )
        .expect("failed to copy host fixture");
    case.copy_fixture_as(fixture::DLL_LWTEST_A, "app", "lwtest_a.dll")
        .expect("failed to copy x64 lwtest_a.dll fixture");
    case.copy_fixture_as(&x86(fixture::DLL_LWTEST_A), "right", "lwtest_a.dll")
        .expect("failed to copy x86 lwtest_a.dll fixture");
    case.copy_fixture_as(&x86(fixture::DLL_LWTEST_B), "right", "lwtest_b.dll")
        .expect("failed to copy x86 lwtest_b.dll fixture");

    let run = run_with_path_prefix(&paths, &case, &run_args(&app, &exe, &[]), &right);
    harness::assert::assert_exit_code(&run, 0);
    assert_line(&run.stdout, "SUCCESS", &["status=0"]);

    let imports = vec![OsString::from("imports"), os(&exe)];
    let result = run_with_path_prefix(&paths, &case, &imports, &right);
    harness::assert::assert_exit_code(&result, 0);
    let lines: Vec<_> = search_paths(&result.stdout)
        .into_iter()
        .filter(|line| line.contains(r#"dll="lwtest_a.dll""#))
        .collect();
    assert!(
        lines
            .first()
            .is_some_and(|line| line.contains(r#"result="WRONG_MACHINE""#)),
        "app-local x64 DLL should be skipped.\n{}",
        result.stdout
    );
    assert!(
        lines
            .last()
            .is_some_and(|line| line.contains(r#"result="HIT""#)),
        "x86 DLL on PATH should be chosen.\n{}",
        result.stdout
    );
}

#[test]
fn x64_target_with_only_x86_dependency_reports_bad_image() {
    let paths = harness::paths::require_from_env();
    let case = TestCase::new(&paths, "x64_wrong_only").expect("failed to initialize test case");
    let app = case.mkdir("app").expect("failed to create app directory");
    let exe = case
        .copy_fixture(
            fixture::HOST_STATIC_IMPORTS_A_EXE,
            r"app\host_static_imports_a.exe",
        )
        .expect("failed to copy host fixture");
    case.copy_fixture_as(&x86(fixture::DLL_LWTEST_A), "app", "lwtest_a.dll")
        .expect("failed to copy x86 lwtest_a.dll fixture");

    let result = run_target(&paths, &case, &run_args(&app, &exe, &[]));

    harness::assert::assert_exit_code(&result, 10);
    assert_line(
        &result.stdout,
        "STATIC_BAD_IMAGE",
        &[r#"dll="lwtest_a.dll""#, r#"reason="BAD_IMAGE""#],
    );
}

#[test]
fn imports_on_x86_image_is_deterministic_and_reports_missing() {
    let paths = harness::paths::require_from_env();
    let case = TestCase::new(&paths, "x86_imports").expect("failed to initialize test case");
    case.mkdir("app").expect("failed to create app directory");
    let exe = case
        .copy_fixture(
            &x86(fixture::HOST_STATIC_IMPORTS_MISSING_EXE),
            r"app\host_static_imports_missing.exe",
        )
        .expect("failed to copy host fixture");

    let args = vec![OsString::from("imports"), os(&exe)];
    let first = run_target(&paths, &case, &args);
    let second = run_target(&paths, &case, &args);

    harness::assert::assert_exit_code(&first, 10);
    assert_line(&first.stdout, "STATIC_MISSING", &[r#"dll="lwtest_a.dll""#]);
    assert_eq!(first.stdout, second.stdout);
}

#[test]
fn com_server_walks_x86_dependencies() {
    let paths = harness::paths::require_from_env();
    let case = TestCase::new(&paths, "x86_com_server").expect("failed to initialize test case");
    case.mkdir("app").expect("failed to create app directory");
    let healthy = case
        .copy_fixture(&x86(fixture::DLL_LWTEST_B), r"app\lwtest_b.dll")
        .expect("failed to copy x86 lwtest_b.dll fixture");
    let broken = case
        .copy_fixture(
            &x86(fixture::HOST_STATIC_IMPORTS_MISSING_EXE),
            r"app\host_static_imports_missing.exe",
        )
        .expect("failed to copy host fixture");

    let com_server = |path: &Path| {
        vec![
            OsString::from("com"),
            OsString::from("server"),
            OsString::from("--view"),
            OsString::from("32"),
            os(path),
        ]
    };

    let ok = run_target(&paths, &case, &com_server(&healthy));
    harness::assert::assert_exit_code(&ok, 0);
    assert_line(
        &ok.stdout,
        "COM_SERVER",
        &[r#"status="OK""#, r#"machine="x86""#],
    );

    let missing = run_target(&paths, &case, &com_server(&broken));
    harness::assert::assert_exit_code(&missing, 10);
    assert_line(
        &missing.stdout,
        "COM_SERVER",
        &[r#"status="SERVER_DEPS_MISSING""#, r#"machine="x86""#],
    );
}
