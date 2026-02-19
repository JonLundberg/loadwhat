use std::env;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn main() {
    let mut args = env::args().skip(1);
    let Some(command) = args.next() else {
        eprintln!("usage: cargo xtask <test>");
        std::process::exit(2);
    };

    let result = match command.as_str() {
        "test" => cmd_test(),
        _ => Err(format!("unknown xtask command: {command}")),
    };

    if let Err(err) = result {
        eprintln!("{err}");
        std::process::exit(1);
    }
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
        return Err(format!("fixture solution not found: {}", solution.display()));
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

    for (variant, target_name) in [("1", "lwtest_a_v1"), ("2", "lwtest_a_v2")] {
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

fn run_rust_tests(repo_root: &Path, test_root: &Path, fixture_bin_root: &Path) -> Result<(), String> {
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
        &["test", "--tests"],
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
