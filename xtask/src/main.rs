use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const CONTAINER_IMAGE: &str = "loadwhat-com-tests:local";
const CONTAINER_CLSIDS: &[&str] = &[
    "{7F4D0001-4C57-4A54-9000-000000000001}",
    "{7F4D0002-4C57-4A54-9000-000000000002}",
    "{7F4D0003-4C57-4A54-9000-000000000003}",
    "{7F4D0004-4C57-4A54-9000-000000000004}",
    "{7F4D0005-4C57-4A54-9000-000000000005}",
    "{7F4D0006-4C57-4A54-9000-000000000006}",
    "{7F4D0007-4C57-4A54-9000-000000000007}",
    "{7F4D0008-4C57-4A54-9000-000000000008}",
    "{7F4D0009-4C57-4A54-9000-000000000009}",
    "{7F4D0010-4C57-4A54-9000-000000000010}",
    "{7F4D0011-4C57-4A54-9000-000000000011}",
];

fn main() {
    let mut args = env::args().skip(1);
    let Some(command) = args.next() else {
        eprintln!("usage: cargo xtask <test|test-container>");
        std::process::exit(2);
    };

    let result = match command.as_str() {
        "test" => cmd_test(),
        "test-container" => cmd_test_container(),
        _ => Err(format!("unknown xtask command: {command}")),
    };

    if let Err(err) = result {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

fn cmd_test_container() -> Result<(), String> {
    if !cfg!(windows) {
        return Err("xtask test-container is supported on Windows only".to_string());
    }

    check_docker_windows_mode()?;
    check_host_registry_sentinels()?;

    let repo_root = repo_root()?;
    let container_root = repo_root.join("target").join("loadwhat-container-tests");
    let context_root = container_root.join("context");
    let fixture_bin_root = container_root.join("fixture-build");

    if container_root.exists() {
        fs::remove_dir_all(&container_root)
            .map_err(|e| format!("failed to clean {}: {e}", container_root.display()))?;
    }
    fs::create_dir_all(&context_root)
        .map_err(|e| format!("failed to create {}: {e}", context_root.display()))?;
    fs::create_dir_all(&fixture_bin_root)
        .map_err(|e| format!("failed to create {}: {e}", fixture_bin_root.display()))?;

    run_command(
        "cargo",
        &["build", "--release", "--locked"],
        Some(&repo_root),
        &[],
    )?;
    build_fixtures(&repo_root, &fixture_bin_root)?;
    build_com_fixtures(&repo_root, &fixture_bin_root)?;
    build_com_transitive_fixtures(&repo_root, &fixture_bin_root)?;
    stage_container_context(&repo_root, &context_root, &fixture_bin_root)?;

    run_command(
        "docker",
        &[
            "build",
            "--tag",
            CONTAINER_IMAGE,
            context_root.to_string_lossy().as_ref(),
        ],
        Some(&repo_root),
        &[],
    )?;

    let run_result = run_command(
        "docker",
        &["run", "--rm", "--isolation=hyperv", CONTAINER_IMAGE],
        Some(&repo_root),
        &[],
    );
    let sentinel_result = check_host_registry_sentinels();

    match (run_result, sentinel_result) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(run), Ok(())) => Err(run),
        (Ok(()), Err(sentinel)) => Err(sentinel),
        (Err(run), Err(sentinel)) => Err(format!("{run}\n{sentinel}")),
    }
}

fn check_docker_windows_mode() -> Result<(), String> {
    let output = Command::new("docker")
        .args(["info", "--format", "{{.OSType}}"])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| {
            format!("Docker is unavailable: {e}. See docs/windows_docker_container_setup.md")
        })?;
    if !output.status.success() {
        return Err(format!(
            "Docker is not ready: {}See docs/windows_docker_container_setup.md",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let os_type = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if os_type != "windows" {
        return Err(format!(
            "Docker is in {os_type:?} container mode; switch to Windows containers. See docs/windows_docker_container_setup.md"
        ));
    }
    Ok(())
}

fn stage_container_context(
    repo_root: &Path,
    context_root: &Path,
    fixture_bin_root: &Path,
) -> Result<(), String> {
    copy_file(
        &repo_root.join("tests/com/container/Dockerfile"),
        &context_root.join("Dockerfile"),
    )?;

    let scripts_out = context_root.join("scripts");
    fs::create_dir_all(&scripts_out)
        .map_err(|e| format!("failed to create {}: {e}", scripts_out.display()))?;
    for script in [
        "assert.ps1",
        "setup_registry.ps1",
        "run_container_tests.ps1",
    ] {
        copy_file(
            &repo_root.join("tests/com/container").join(script),
            &scripts_out.join(script),
        )?;
    }

    copy_file(
        &repo_root.join("target/release/loadwhat.exe"),
        &context_root.join("loadwhat.exe"),
    )?;

    let healthy = context_root.join("fixtures/healthy");
    let broken = context_root.join("fixtures/broken");
    let transitive = context_root.join("fixtures/transitive");
    let target = context_root.join("fixtures/target");
    let target_has_dep = context_root.join("fixtures/context/target_has_dep");
    let target_missing_dep = context_root.join("fixtures/context/target_missing_dep");
    let server_missing_dep = context_root.join("fixtures/context/server_missing_dep");
    let server_has_dep = context_root.join("fixtures/context/server_has_dep");
    let x86 = context_root.join("fixtures/x86");
    fs::create_dir_all(&healthy)
        .map_err(|e| format!("failed to create {}: {e}", healthy.display()))?;
    fs::create_dir_all(&broken)
        .map_err(|e| format!("failed to create {}: {e}", broken.display()))?;
    fs::create_dir_all(&transitive)
        .map_err(|e| format!("failed to create {}: {e}", transitive.display()))?;
    fs::create_dir_all(&target)
        .map_err(|e| format!("failed to create {}: {e}", target.display()))?;
    for directory in [
        &target_has_dep,
        &target_missing_dep,
        &server_missing_dep,
        &server_has_dep,
        &x86,
    ] {
        fs::create_dir_all(directory)
            .map_err(|e| format!("failed to create {}: {e}", directory.display()))?;
    }

    copy_file(
        &repo_root.join("tests/com/container/target_x64.exe.manifest"),
        &target.join("lwtest_com_target_x64.exe.manifest"),
    )?;

    copy_file(
        &fixture_bin_root.join("lwtest_b.dll"),
        &healthy.join("lwtest_com_server_x64.dll"),
    )?;
    copy_file(
        &fixture_bin_root.join("lwtest_a_v1.dll"),
        &broken.join("lwtest_com_server_dep_missing.dll"),
    )?;
    copy_file(
        &fixture_bin_root.join("lwtest_a_v1.dll"),
        &transitive.join("lwtest_com_server_dep_transitive.dll"),
    )?;
    copy_file(
        &fixture_bin_root.join("lwtest_b_dep_missing.dll"),
        &transitive.join("lwtest_b.dll"),
    )?;
    copy_file(
        &fixture_bin_root.join("host_echo_argv_cwd.exe"),
        &healthy.join("lwtest_com_localserver_x64.exe"),
    )?;
    copy_file(
        &fixture_bin_root.join("lwtest_com_target_x64.exe"),
        &target.join("lwtest_com_target_x64.exe"),
    )?;
    copy_file(
        &fixture_bin_root.join("lwtest_b.dll"),
        &target.join("lwtest_manifest_server.dll"),
    )?;

    for destination in [&target_has_dep, &target_missing_dep] {
        copy_file(
            &fixture_bin_root.join("lwtest_com_target_x64.exe"),
            &destination.join("lwtest_com_target_x64.exe"),
        )?;
    }
    copy_file(
        &fixture_bin_root.join("lwtest_b.dll"),
        &target_has_dep.join("lwtest_b.dll"),
    )?;
    for destination in [&server_missing_dep, &server_has_dep] {
        copy_file(
            &fixture_bin_root.join("lwtest_a_v1.dll"),
            &destination.join("lwtest_com_context_server.dll"),
        )?;
    }
    copy_file(
        &fixture_bin_root.join("lwtest_b.dll"),
        &server_has_dep.join("lwtest_b.dll"),
    )?;

    copy_file(
        &fixture_bin_root.join("lwtest_com_server_x86.dll"),
        &x86.join("lwtest_com_server_x86.dll"),
    )?;
    copy_file(
        &fixture_bin_root.join("lwtest_com_target_x86.exe"),
        &x86.join("lwtest_com_target_x86.exe"),
    )?;

    Ok(())
}

fn copy_file(source: &Path, destination: &Path) -> Result<(), String> {
    fs::copy(source, destination).map_err(|e| {
        format!(
            "failed to copy {} to {}: {e}",
            source.display(),
            destination.display()
        )
    })?;
    Ok(())
}

#[cfg(windows)]
fn check_host_registry_sentinels() -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;

    type Hkey = isize;
    const HKEY_CURRENT_USER: Hkey = 0x80000001u32 as isize;
    const HKEY_LOCAL_MACHINE: Hkey = 0x80000002u32 as isize;
    const KEY_READ: u32 = 0x0002_0019;
    const KEY_WOW64_64KEY: u32 = 0x0100;
    const KEY_WOW64_32KEY: u32 = 0x0200;
    const ERROR_SUCCESS: i32 = 0;
    const ERROR_FILE_NOT_FOUND: i32 = 2;
    const ERROR_PATH_NOT_FOUND: i32 = 3;

    #[link(name = "advapi32")]
    unsafe extern "system" {
        fn RegOpenKeyExW(
            hkey: Hkey,
            subkey: *const u16,
            options: u32,
            sam_desired: u32,
            result: *mut Hkey,
        ) -> i32;
        fn RegCloseKey(hkey: Hkey) -> i32;
    }

    let mut subkeys = vec!["Software\\Classes\\LoadWhat.Container.ComTests".to_string()];
    subkeys.extend(
        CONTAINER_CLSIDS
            .iter()
            .map(|clsid| format!("Software\\Classes\\CLSID\\{clsid}")),
    );

    for (hive_name, hive) in [("HKCU", HKEY_CURRENT_USER), ("HKLM", HKEY_LOCAL_MACHINE)] {
        for (view_name, view_flag) in [("64", KEY_WOW64_64KEY), ("32", KEY_WOW64_32KEY)] {
            for subkey in &subkeys {
                let wide: Vec<u16> = OsStr::new(subkey).encode_wide().chain(Some(0)).collect();
                let mut opened: Hkey = 0;
                // Read-only open. This function never creates, updates, or deletes registry data.
                let status = unsafe {
                    RegOpenKeyExW(hive, wide.as_ptr(), 0, KEY_READ | view_flag, &mut opened)
                };
                match status {
                    ERROR_SUCCESS => {
                        unsafe { RegCloseKey(opened) };
                        return Err(format!(
                            "HOST REGISTRY SENTINEL FAILED: {hive_name}\\{subkey} exists in the {view_name}-bit view. No keys were deleted. Inspect the host before continuing."
                        ));
                    }
                    ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND => {}
                    other => {
                        return Err(format!(
                            "HOST REGISTRY SENTINEL INDETERMINATE: could not inspect {hive_name}\\{subkey} in the {view_name}-bit view (Win32 error {other}). No container was run."
                        ));
                    }
                }
            }
        }
    }
    println!("LWTEST:HOST_REGISTRY_SENTINEL PASS");
    Ok(())
}

#[cfg(not(windows))]
fn check_host_registry_sentinels() -> Result<(), String> {
    Err("host registry sentinel checks require Windows".to_string())
}

fn cmd_test() -> Result<(), String> {
    if !cfg!(windows) {
        return Err("xtask test is supported on Windows only".to_string());
    }

    let repo_root = repo_root()?;
    let test_root = repo_root.join("target").join("loadwhat-tests");
    let fixture_bin_root = test_root.join("fixtures").join("bin");

    if test_root.exists() {
        std::fs::remove_dir_all(&test_root)
            .map_err(|e| format!("failed to clean {}: {e}", test_root.display()))?;
    }
    std::fs::create_dir_all(&fixture_bin_root)
        .map_err(|e| format!("failed to create {}: {e}", fixture_bin_root.display()))?;

    build_fixtures(&repo_root, &fixture_bin_root)?;
    run_rust_tests(&repo_root, &test_root, &fixture_bin_root)?;
    Ok(())
}

fn repo_root() -> Result<PathBuf, String> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "failed to determine repository root".to_string())
}

fn build_fixtures(repo_root: &Path, fixture_bin_root: &Path) -> Result<(), String> {
    let msbuild = resolve_msbuild_program()?;
    let msvc_root = repo_root.join("tests").join("fixtures").join("msvc");
    let solution = msvc_root.join("LoadWhatFixtures.sln");
    let dll_a_proj = msvc_root.join("dll_lwtest_a").join("dll_lwtest_a.vcxproj");

    if !solution.exists() {
        return Err(format!(
            "fixture solution not found: {}",
            solution.display()
        ));
    }

    let outdir_prop = format!(
        "/p:Configuration=Release;Platform=x64;LWTEST_OUTDIR={}",
        msbuild_outdir_value(fixture_bin_root)
    );

    run_command(
        &msbuild,
        &[
            solution.to_string_lossy().as_ref(),
            "/m",
            "/nologo",
            "/t:Build",
            &outdir_prop,
        ],
        Some(repo_root),
        &[],
    )?;

    for (variant, target_name) in [
        ("1", "lwtest_a_v1"),
        ("2", "lwtest_a_v2"),
        ("3", "lwtest_a_initfail"),
        ("4", "lwtest_a_nested"),
    ] {
        let variant_prop = format!(
            "/p:Configuration=Release;Platform=x64;LWTEST_OUTDIR={};LWTEST_VARIANT={variant};TargetName={target_name};BuildProjectReferences=false",
            msbuild_outdir_value(fixture_bin_root)
        );
        run_command(
            &msbuild,
            &[
                dll_a_proj.to_string_lossy().as_ref(),
                "/m",
                "/nologo",
                "/t:Build",
                &variant_prop,
            ],
            Some(repo_root),
            &[],
        )?;
    }

    Ok(())
}

fn build_com_fixtures(repo_root: &Path, fixture_bin_root: &Path) -> Result<(), String> {
    let msbuild = resolve_msbuild_program()?;
    let msvc_root = repo_root.join("tests").join("fixtures").join("msvc");
    let outdir = msbuild_outdir_value(fixture_bin_root);

    for (project, platform, target_name, extra_property) in [
        (
            "host_echo_argv_cwd/host_echo_argv_cwd.vcxproj",
            "x64",
            "lwtest_com_target_x64",
            ";GenerateManifest=false",
        ),
        (
            "dll_lwtest_b/dll_lwtest_b.vcxproj",
            "Win32",
            "lwtest_com_server_x86",
            "",
        ),
        (
            "host_echo_argv_cwd/host_echo_argv_cwd.vcxproj",
            "Win32",
            "lwtest_com_target_x86",
            ";GenerateManifest=false",
        ),
    ] {
        let project_path = msvc_root.join(project);
        let properties = format!(
            "/p:Configuration=Release;Platform={platform};LWTEST_OUTDIR={outdir};TargetName={target_name}{extra_property}"
        );
        run_command(
            &msbuild,
            &[
                project_path.to_string_lossy().as_ref(),
                "/nologo",
                "/verbosity:minimal",
                "/t:Build",
                &properties,
            ],
            Some(repo_root),
            &[],
        )?;
    }
    Ok(())
}

fn build_com_transitive_fixtures(repo_root: &Path, fixture_bin_root: &Path) -> Result<(), String> {
    let msbuild = resolve_msbuild_program()?;
    let msvc_root = repo_root.join("tests").join("fixtures").join("msvc");
    let outdir = msbuild_outdir_value(fixture_bin_root);

    for (project, properties) in [
        (
            "dll_lwtest_c/dll_lwtest_c.vcxproj",
            format!("/p:Configuration=Release;Platform=x64;LWTEST_OUTDIR={outdir}"),
        ),
        (
            "dll_lwtest_b/dll_lwtest_b.vcxproj",
            format!(
                "/p:Configuration=Release;Platform=x64;LWTEST_OUTDIR={outdir};LWTEST_B_DEPENDS_ON_C=1;TargetName=lwtest_b_dep_missing"
            ),
        ),
    ] {
        let project_path = msvc_root.join(project);
        run_command(
            &msbuild,
            &[
                project_path.to_string_lossy().as_ref(),
                "/nologo",
                "/verbosity:minimal",
                "/t:Rebuild",
                &properties,
            ],
            Some(repo_root),
            &[],
        )?;
    }
    Ok(())
}

fn run_rust_tests(
    repo_root: &Path,
    test_root: &Path,
    fixture_bin_root: &Path,
) -> Result<(), String> {
    let mut extra_env = Vec::<(String, String)>::new();
    extra_env.push((
        "LOADWHAT_TEST_ROOT".to_string(),
        test_root.to_string_lossy().to_string(),
    ));
    extra_env.push((
        "LOADWHAT_FIXTURE_BIN_ROOT".to_string(),
        fixture_bin_root.to_string_lossy().to_string(),
    ));
    extra_env.push(("LOADWHAT_TEST_MODE".to_string(), "1".to_string()));

    if env::var_os("LOADWHAT_KEEP_TEST_ARTIFACTS").is_some() {
        extra_env.push((
            "LOADWHAT_KEEP_TEST_ARTIFACTS".to_string(),
            env::var("LOADWHAT_KEEP_TEST_ARTIFACTS").unwrap_or_else(|_| "1".to_string()),
        ));
    }

    if env::var_os("RUST_TEST_THREADS").is_none() {
        extra_env.push(("RUST_TEST_THREADS".to_string(), "1".to_string()));
    }

    let env_refs: Vec<(&str, &str)> = extra_env
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();

    run_command(
        "cargo",
        &["test", "--tests", "--features", "harness-tests"],
        Some(repo_root),
        &env_refs,
    )?;

    Ok(())
}

fn run_command(
    program: &str,
    args: &[&str],
    cwd: Option<&Path>,
    env_pairs: &[(&str, &str)],
) -> Result<(), String> {
    let mut cmd = Command::new(program);
    cmd.args(args);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    for (key, value) in env_pairs {
        cmd.env(key, value);
    }

    let status = cmd
        .status()
        .map_err(|e| format!("failed to run {program}: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "{program} failed with status {}",
            status
                .code()
                .map(|c| c.to_string())
                .unwrap_or_else(|| "terminated".to_string())
        ))
    }
}

fn msbuild_outdir_value(path: &Path) -> String {
    let mut value = path.to_string_lossy().replace('/', "\\");
    if !value.ends_with('\\') {
        value.push('\\');
    }
    value
}

fn resolve_msbuild_program() -> Result<String, String> {
    if command_exists("msbuild") {
        return Ok("msbuild".to_string());
    }

    if let Some(path) = env::var_os("MSBUILD_EXE_PATH").map(PathBuf::from) {
        if path.exists() {
            return Ok(path.to_string_lossy().to_string());
        }
    }

    if let Some(path) = find_msbuild_via_vswhere() {
        return Ok(path);
    }

    for candidate in known_msbuild_paths() {
        if candidate.exists() {
            return Ok(candidate.to_string_lossy().to_string());
        }
    }

    Err("failed to run msbuild: program not found".to_string())
}

fn command_exists(program: &str) -> bool {
    Command::new(program)
        .arg("/version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
}

fn find_msbuild_via_vswhere() -> Option<String> {
    let installer = env::var_os("ProgramFiles(x86)")?;
    let vswhere = PathBuf::from(installer)
        .join("Microsoft Visual Studio")
        .join("Installer")
        .join("vswhere.exe");
    if !vswhere.exists() {
        return None;
    }

    let output = Command::new(vswhere)
        .args([
            "-latest",
            "-products",
            "*",
            "-requires",
            "Microsoft.Component.MSBuild",
            "-find",
            r"MSBuild\**\Bin\MSBuild.exe",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && Path::new(OsStr::new(line)).exists())
        .map(|line| line.to_string())
}

fn known_msbuild_paths() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(program_files_x86) = env::var_os("ProgramFiles(x86)").map(PathBuf::from) {
        for edition in ["BuildTools", "Community", "Professional", "Enterprise"] {
            out.push(
                program_files_x86
                    .join("Microsoft Visual Studio")
                    .join("2022")
                    .join(edition)
                    .join("MSBuild")
                    .join("Current")
                    .join("Bin")
                    .join("MSBuild.exe"),
            );
            out.push(
                program_files_x86
                    .join("Microsoft Visual Studio")
                    .join("2019")
                    .join(edition)
                    .join("MSBuild")
                    .join("Current")
                    .join("Bin")
                    .join("MSBuild.exe"),
            );
        }
    }
    out
}
