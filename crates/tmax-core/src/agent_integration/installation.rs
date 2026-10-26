//! Native executable resolution, scoped commands, and installation qualification.
use super::{adapters, scope::read_native};
use crate::{Error, Result};
use serde_json::{Value, json};
#[cfg(windows)]
use std::path::Component;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
pub(super) struct NativeExecutable {
    program: PathBuf,
    script: Option<PathBuf>,
}
impl From<PathBuf> for NativeExecutable {
    fn from(program: PathBuf) -> Self {
        Self {
            program,
            script: None,
        }
    }
}
#[cfg(windows)]
fn npm_pi(directory: &Path, paths: &[PathBuf]) -> Option<NativeExecutable> {
    if !directory.join("pi.cmd").is_file() {
        return None;
    }
    let node = std::iter::once(directory.to_owned())
        .chain(paths.iter().cloned())
        .map(|directory| directory.join("node.exe"))
        .find(|path| path.is_file())?;
    for package in [
        "@earendil-works/pi-coding-agent",
        "@mariozechner/pi-coding-agent",
    ] {
        let root = directory.join("node_modules").join(package);
        let manifest = read_native(&root.join("package.json"))
            .ok()
            .flatten()
            .and_then(|text| serde_json::from_str::<Value>(&text).ok());
        let Some(manifest) = manifest else { continue };
        if manifest["name"] != package {
            continue;
        }
        let Some(bin) = manifest["bin"]["pi"].as_str() else {
            continue;
        };
        // Read npm metadata; never execute or interpolate the batch/PowerShell shim.
        let bin = Path::new(bin);
        if !bin
            .components()
            .all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
        {
            continue;
        }
        let script = root.join(bin);
        if script.is_file() {
            return Some(NativeExecutable {
                program: node,
                script: Some(script),
            });
        }
    }
    None
}
pub(super) fn executable(agent: &str) -> Option<NativeExecutable> {
    let paths = std::env::var_os("PATH")?;
    let paths: Vec<_> = std::env::split_paths(&paths).collect();
    for directory in &paths {
        #[cfg(not(windows))]
        if directory.join(agent).is_file() {
            return Some(directory.join(agent).into());
        }
        #[cfg(windows)]
        {
            let binary = directory.join(format!("{agent}.exe"));
            if binary.is_file() {
                return Some(binary.into());
            }
            if agent == "pi"
                && let Some(executable) = npm_pi(directory, &paths)
            {
                return Some(executable);
            }
            // npm's official package includes a native binary. Never execute its
            // POSIX shim as a Windows executable or interpolate a shell command.
            if agent == "codex" && directory.join("codex.cmd").is_file() {
                let (package, triple) = if cfg!(target_arch = "aarch64") {
                    ("codex-win32-arm64", "aarch64-pc-windows-msvc")
                } else {
                    ("codex-win32-x64", "x86_64-pc-windows-msvc")
                };
                for prefix in [
                    directory.join("node_modules/@openai"),
                    directory.join("node_modules/@openai/codex/node_modules/@openai"),
                ] {
                    for suffix in ["bin/codex.exe", "codex/codex.exe"] {
                        let binary = prefix
                            .join(package)
                            .join("vendor")
                            .join(triple)
                            .join(suffix);
                        if binary.is_file() {
                            return Some(binary.into());
                        }
                    }
                }
            }
        }
    }
    None
}
pub(super) fn native_command(agent: &str, scope: &Path) -> Result<tokio::process::Command> {
    let executable = executable(agent).ok_or_else(|| {
        Error::new(
            "native_not_installed",
            "The selected native executable is not on PATH",
        )
    })?;
    let mut cmd = tokio::process::Command::new(executable.program);
    if let Some(script) = executable.script {
        cmd.arg(script);
    }
    cmd.current_dir(scope).kill_on_drop(true);
    match agent {
        "codex" => {
            cmd.env("CODEX_HOME", scope);
        }
        "pi" => {
            cmd.env("PI_CODING_AGENT_DIR", scope);
        }
        "omp" => {
            cmd.env("PI_CODING_AGENT_DIR", scope)
                .env_remove("OMP_PROFILE")
                .env_remove("PI_PROFILE");
        }
        "opencode" => {
            cmd.env("OPENCODE_CONFIG_DIR", scope)
                .env("OPENCODE_DB", scope.join("opencode.db"))
                .env("XDG_DATA_HOME", scope.join("native-data"))
                .env("XDG_CONFIG_HOME", scope.join("native-config"))
                .env("XDG_CACHE_HOME", scope.join("native-cache"))
                .env("XDG_STATE_HOME", scope.join("native-state"));
        }
        "hermes" => {
            cmd.env("HERMES_HOME", scope);
        }
        "dsh" => {
            cmd.env("DSH_HOME", scope);
        }
        _ => return Err(Error::new("unsupported_agent", "Unknown native adapter")),
    }
    Ok(cmd)
}
fn detected_version(agent: &str, text: &str) -> Option<String> {
    text.split_whitespace().find_map(|word| {
        let word = if agent == "omp" {
            word.strip_prefix("omp/").unwrap_or(word)
        } else {
            word
        };
        let word = word.trim_start_matches('v');
        (word.starts_with(|c: char| c.is_ascii_digit())
            && word
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || ".+-_".contains(c)))
        .then(|| word.to_owned())
    })
}
fn hermes_provenance_runtime(executable: &Path) -> Option<(PathBuf, PathBuf)> {
    // Resolve only the concrete researched shell-launcher forms as inert code
    // metadata. Neither the repair-capable wrapper nor its bootstrap runs.
    let mut launcher = fs::canonicalize(executable).ok()?;
    for _ in 0..4 {
        let text = read_native(&launcher).ok()??;
        let exec = text.lines().find_map(|line| line.strip_prefix("exec "))?;
        if let Some(next) = exec.strip_suffix(" \"$@\"") {
            let next = next.trim_matches('"');
            if !Path::new(next).is_absolute() || next.chars().any(char::is_whitespace) {
                return None;
            }
            launcher = fs::canonicalize(next).ok()?;
            continue;
        }
        let (python, _) = exec.split_once(" -I -c '")?;
        if !Path::new(python).is_absolute() || python.chars().any(char::is_whitespace) {
            return None;
        }
        let decoded = text.replace("'\"'\"'", "'");
        let root = decoded.lines().find_map(|line| {
            line.strip_prefix("sys.path.insert(0, '")
                .and_then(|rest| rest.strip_suffix("')"))
        })?;
        if !Path::new(root).is_absolute() || !Path::new(root).is_dir() {
            return None;
        }
        return Some((PathBuf::from(python), PathBuf::from(root)));
    }
    None
}
async fn hermes_version(path: &Path) -> Option<Vec<u8>> {
    // --version performs network update checks; --print-runtime-command can
    // repair interrupted source updates. Read launcher metadata, then invoke
    // only the native stdlib provenance API, with no bootstrap or bytecode write.
    let (program, root) = hermes_provenance_runtime(&executable("hermes")?.program)?;
    let root = serde_json::to_string(root.to_str()?).ok()?;
    let code = format!(
        "import sys;sys.dont_write_bytecode=True;sys.path.insert(0,{root});from hermes_cli.version_info import get_version_info; print(get_version_info().derived_version)"
    );
    let mut cmd = tokio::process::Command::new(program);
    cmd.args(["-I", "-B", "-c", &code])
        .env("HERMES_HOME", path)
        .env("HERMES_DISABLE_LAZY_INSTALLS", "1")
        .env("GIT_NO_LAZY_FETCH", "1")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .current_dir(path)
        .kill_on_drop(true)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let output = tokio::time::timeout(Duration::from_secs(10), cmd.output())
        .await
        .ok()?
        .ok()?;
    (output.status.success() && output.stdout.len() < 4096).then_some(output.stdout)
}
pub(super) async fn version(agent: &str, path: &Path) -> Value {
    let Some(exe) = executable(agent) else {
        return json!({"state":"not_installed","version":null,"compatibility":"unknown","qualified_version":adapters::qualified_version(agent)});
    };
    let output = if agent == "hermes" {
        hermes_version(path).await
    } else {
        match native_command(agent, path) {
            Ok(mut cmd) => {
                cmd.arg("--version")
                    .stdin(Stdio::null())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null());
                match tokio::time::timeout(Duration::from_secs(10), cmd.output()).await {
                    Ok(Ok(output)) if output.status.success() && output.stdout.len() < 4096 => {
                        Some(output.stdout)
                    }
                    _ => None,
                }
            }
            Err(_) => None,
        }
    };
    let found = output
        .as_deref()
        .and_then(|b| std::str::from_utf8(b).ok())
        .and_then(|text| detected_version(agent, text));
    let exact = found.as_deref() == Some(adapters::qualified_version(agent));
    json!({"executable":exe.script.as_ref().unwrap_or(&exe.program),"version":found,"state":if found.is_some(){"detected"}else if output.is_none(){"probe_failed"}else{"version_unknown"},"compatibility":if exact{"documented"}else{"unknown"},"qualified_version":adapters::qualified_version(agent),"version_range":null,"source":adapters::source(agent),"runtime_inference":"not_verified"})
}
pub(super) fn version_blocker(agent: &str, installation: &Value) -> Option<Value> {
    if installation["state"] == "not_installed" {
        return Some(
            json!({"code":"native_not_installed","message":"Install the tool or correct PATH before applying"}),
        );
    }
    if agent == "opencode"
        && installation["version"]
            .as_str()
            .is_some_and(|version| version.starts_with("1."))
    {
        return Some(
            json!({"code":"native_schema_incompatible","message":"OpenCode 1 uses a different configuration format. Upgrade to OpenCode 2 before applying"}),
        );
    }
    None
}
pub(super) fn version_warnings(installation: &Value) -> Vec<Value> {
    if installation["compatibility"] == "documented" || installation["state"] == "not_installed" {
        return Vec::new();
    }
    vec![
        json!({"code":"native_version_unverified","message":"This tool version has not been verified. Configuration can still be applied; reload the tool to check it"}),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(windows)]
    #[test]
    fn npm_pi_resolves_official_package_without_executing_shims() {
        let directory = tempfile::tempdir().unwrap();
        let prefix = directory.path().join("npm with spaces");
        let node_dir = directory.path().join("node with spaces");
        fs::create_dir_all(&node_dir).unwrap();
        fs::create_dir_all(&prefix).unwrap();
        fs::write(prefix.join("pi.cmd"), "exit /b 1").unwrap();
        for package in [
            "@earendil-works/pi-coding-agent",
            "@mariozechner/pi-coding-agent",
        ] {
            let root = prefix.join("node_modules").join(package);
            fs::create_dir_all(root.join("dist/bundle")).unwrap();
            fs::write(root.join("dist/bundle/cli.js"), "console.log('1.4.0')").unwrap();
            fs::write(
                root.join("package.json"),
                serde_json::to_vec(&json!({"name":package,"bin":{"pi":"dist/bundle/cli.js"}}))
                    .unwrap(),
            )
            .unwrap();
            assert!(npm_pi(&prefix, &[]).is_none());
            fs::write(node_dir.join("node.exe"), "fixture").unwrap();
            let found = npm_pi(&prefix, std::slice::from_ref(&node_dir)).unwrap();
            assert_eq!(found.program, node_dir.join("node.exe"));
            assert_eq!(found.script, Some(root.join("dist/bundle/cli.js")));
            fs::write(prefix.join("node.exe"), "sibling").unwrap();
            assert_eq!(
                npm_pi(&prefix, std::slice::from_ref(&node_dir))
                    .unwrap()
                    .program,
                prefix.join("node.exe")
            );
            fs::write(
                root.join("package.json"),
                serde_json::to_vec(&json!({"name":package,"bin":{"pi":"../outside.js"}})).unwrap(),
            )
            .unwrap();
            assert!(npm_pi(&prefix, std::slice::from_ref(&node_dir)).is_none());
            fs::remove_file(prefix.join("node.exe")).unwrap();
            fs::remove_file(node_dir.join("node.exe")).unwrap();
        }
    }
    #[test]
    fn application_versions_are_advisory_except_known_incompatible_schemas() {
        for state in ["probe_failed", "version_unknown", "detected"] {
            let evidence = json!({"state":state,"version":"2.0.19","compatibility":"unknown"});
            assert!(version_blocker("opencode", &evidence).is_none());
            assert_eq!(version_warnings(&evidence).len(), 1);
        }
        assert_eq!(
            version_blocker("opencode", &json!({"version":"1.9.0"})).unwrap()["code"],
            "native_schema_incompatible"
        );
        assert_eq!(
            version_blocker("pi", &json!({"state":"not_installed"})).unwrap()["code"],
            "native_not_installed"
        );
        assert!(version_warnings(&json!({"compatibility":"documented"})).is_empty());
    }
    #[test]
    fn hermes_provenance_resolution_never_executes_repair_capable_launcher() {
        let directory = tempfile::tempdir().unwrap();
        let outer = directory.path().join("hermes");
        let inner = directory.path().join("native-launcher");
        let marker = directory.path().join("wrapper-executed");
        let python = directory.path().join("unexecuted-python");
        fs::write(
            &outer,
            format!(
                "#!/bin/sh\ntouch '{}'\nexec {} \"$@\"\n",
                marker.display(),
                inner.display()
            ),
        )
        .unwrap();
        fs::write(&inner,format!("#!/bin/sh\nexec {} -I -c 'import sys\nsys.path.insert(0, '\"'\"'{}'\"'\"')\nimport hermes_bootstrap\n' \"$@\"\n",python.display(),directory.path().display())).unwrap();
        let (resolved_python, source) = hermes_provenance_runtime(&outer).unwrap();
        assert_eq!(resolved_python, python);
        assert_eq!(source, directory.path());
        assert!(!marker.exists());
    }
    #[test]
    fn hermes_provenance_refuses_unqualified_dynamic_launcher_metadata() {
        let directory = tempfile::tempdir().unwrap();
        let launcher = directory.path().join("hermes");
        fs::write(&launcher, "#!/bin/sh\nexec \"$HERMES_PYTHON\" \"$@\"\n").unwrap();
        assert!(hermes_provenance_runtime(&launcher).is_none());
    }
    #[test]
    fn installed_version_prefixes_remain_exact() {
        assert_eq!(
            detected_version("omp", "omp/18.12.6\n").as_deref(),
            Some("18.12.6")
        );
        assert_eq!(
            detected_version("codex", "codex-cli 0.162.1\n").as_deref(),
            Some("0.162.1")
        );
        assert_eq!(
            detected_version("hermes", "Hermes v0.22.1+683.g848fc0a\n").as_deref(),
            Some("0.22.1+683.g848fc0a")
        );
    }
}
