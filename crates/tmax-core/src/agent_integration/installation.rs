//! Native executable resolution, scoped commands, and installation evidence.
use super::{adapters, scope::read_native};
use crate::{Error, Result};
use serde_json::{Value, json};
#[cfg(windows)]
use std::path::{Component, Path, PathBuf};
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
        return json!({"state":"not_installed","version":null});
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
    json!({"executable":exe.script.as_ref().unwrap_or(&exe.program),"version":found,"state":if found.is_some(){"detected"}else if output.is_none(){"probe_failed"}else{"version_unknown"},"source":adapters::source(agent)})
}
pub(super) fn schema_blocker(agent: &str, installation: &Value) -> Option<Value> {
    if agent == "opencode"
        && installation["version"]
            .as_str()
            .is_some_and(|version| version.starts_with("1."))
    {
        return Some(
            json!({"code":"native_schema_incompatible","message":"OpenCode 1 uses a different configuration format. Upgrade to OpenCode 2 before applying"}),
        );
    }
    if agent == "dsh"
        && let Some(version) = installation["version"].as_str()
        && !version_at_least(version, adapters::DSH_QUALIFIED)
    {
        return Some(json!({
            "code":"native_version_unqualified",
            "message":format!("DSH {version} predates the qualified {}; this tmax supports only that version and newer. Upgrade DSH before applying", adapters::DSH_QUALIFIED),
            "detected":version,
            "qualified":adapters::DSH_QUALIFIED,
        }));
    }
    None
}

/// What the user is told when a DSH home has no plugin to configure.
pub(super) const INSTALL_TMAX_PLUGIN: &str = "Install the tmax plugin into a DSH profile: from Plugins in DSH Desktop, or `dsh plugin --profile <profile> add <repository>`; then apply again";

/// The installed tmax plugin, per profile of this DSH home.
///
/// DSH activates a plugin listed in `dsh.profile.bundles`, so a package
/// directory alone is not an installation: it is reported as `partial` rather
/// than counted, and an unlisted directory is not evidence at all.
pub(super) fn dsh_plugin(scope: &Path) -> Value {
    let mut profiles = Vec::new();
    for profile in profile_directories(scope) {
        let manifest = read_json(&profile.join("package.json"));
        let listed = manifest.as_ref().is_some_and(lists_plugin);
        let installed = read_json(&profile.join("node_modules/tmax/package.json"));
        profiles.push(json!({
            "profile": profile.file_name().map(|name| name.to_string_lossy().into_owned()),
            "listed": listed,
            "package": installed.is_some(),
            "version": installed.as_ref().and_then(|package| package["version"].as_str()),
        }));
    }
    let installed = profiles
        .iter()
        .any(|row| row["listed"] == true && row["package"] == true);
    let partial = profiles.iter().any(|row| row["listed"] == true);
    json!({
        "state": if installed { "installed" } else if partial { "partial" } else { "absent" },
        "profiles": profiles,
    })
}

/// A plugin is active when the profile lists it as a bundle or as a dependency.
fn lists_plugin(manifest: &Value) -> bool {
    manifest
        .pointer("/dsh/profile/bundles")
        .and_then(Value::as_array)
        .is_some_and(|bundles| bundles.iter().any(|bundle| bundle.as_str() == Some("tmax")))
        || manifest
            .get("dependencies")
            .and_then(Value::as_object)
            .is_some_and(|dependencies| dependencies.contains_key("tmax"))
}

fn profile_directories(scope: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(scope.join("profiles")) else {
        return Vec::new();
    };
    let mut profiles: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.is_dir() && path.join("package.json").is_file())
        .collect();
    profiles.sort();
    profiles
}

fn read_json(path: &Path) -> Option<Value> {
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Semver ordering over the dotted numeric core and prerelease identifiers
/// tmax qualifies against. A prerelease sorts below its release; a numeric
/// identifier sorts below an alphanumeric one; a shorter identifier list
/// sorts below a longer one with the same prefix.
pub(super) fn version_at_least(found: &str, floor: &str) -> bool {
    fn split(version: &str) -> (Vec<u64>, Vec<String>) {
        // Semver build metadata never affects precedence, and leaving it in the
        // core would make `0.2.3+abc` parse as 0.2.0.
        let version = version.split_once('+').map_or(version, |(rest, _)| rest);
        let (core, pre) = version.split_once('-').unwrap_or((version, ""));
        let core = core
            .split('.')
            .map(|part| part.parse::<u64>().unwrap_or(0))
            .collect();
        let pre = if pre.is_empty() {
            Vec::new()
        } else {
            pre.split('.').map(str::to_owned).collect()
        };
        (core, pre)
    }
    let (found_core, found_pre) = split(found);
    let (floor_core, floor_pre) = split(floor);
    for index in 0..found_core.len().max(floor_core.len()) {
        let a = found_core.get(index).copied().unwrap_or(0);
        let b = floor_core.get(index).copied().unwrap_or(0);
        if a != b {
            return a > b;
        }
    }
    if found_pre.is_empty() || floor_pre.is_empty() {
        // A release outranks any prerelease; two releases are equal here.
        return found_pre.is_empty();
    }
    for index in 0..found_pre.len().max(floor_pre.len()) {
        match (found_pre.get(index), floor_pre.get(index)) {
            (None, Some(_)) => return false,
            (Some(_), None) => return true,
            (Some(a), Some(b)) if a == b => {}
            (Some(a), Some(b)) => {
                return match (a.parse::<u64>(), b.parse::<u64>()) {
                    (Ok(a), Ok(b)) => a > b,
                    (Ok(_), Err(_)) => false,
                    (Err(_), Ok(_)) => true,
                    _ => a > b,
                };
            }
            (None, None) => unreachable!(),
        }
    }
    true
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
    fn only_opencode_one_schema_blocks_application() {
        for evidence in [
            json!({"state":"not_installed","version":null}),
            json!({"state":"probe_failed","version":null}),
            json!({"state":"version_unknown","version":null}),
            json!({"state":"detected","version":"2.0.19"}),
        ] {
            assert!(schema_blocker("opencode", &evidence).is_none());
        }
        assert_eq!(
            schema_blocker("opencode", &json!({"version":"1.9.0"})).unwrap()["code"],
            "native_schema_incompatible"
        );
        assert!(schema_blocker("pi", &json!({"version":"1.9.0"})).is_none());
    }
    #[test]
    fn dsh_below_the_qualified_floor_blocks_and_missing_versions_do_not() {
        for evidence in [
            json!({"state":"not_installed","version":null}),
            json!({"state":"probe_failed","version":null}),
            json!({"state":"version_unknown","version":null}),
            json!({"state":"detected","version":adapters::DSH_QUALIFIED}),
            json!({"state":"detected","version":"0.2.4"}),
            json!({"state":"detected","version":"0.3.0-alpha.1"}),
        ] {
            assert!(schema_blocker("dsh", &evidence).is_none(), "{evidence}");
        }
        for older in ["0.2.1-rc.2", "0.2.2-rc.5", "0.2.3-alpha.1"] {
            let blocker = schema_blocker("dsh", &json!({"version":older})).unwrap();
            assert_eq!(blocker["code"], "native_version_unqualified", "{older}");
            assert_eq!(blocker["detected"], older);
            assert_eq!(blocker["qualified"], adapters::DSH_QUALIFIED);
        }
    }
    #[test]
    fn version_ordering_follows_numeric_cores_and_prerelease_identifiers() {
        for (found, floor, expected) in [
            ("0.2.3-alpha.2", "0.2.3-alpha.2", true),
            ("0.2.3-alpha.1", "0.2.3-alpha.2", false),
            ("0.2.3-alpha.10", "0.2.3-alpha.2", true),
            ("0.2.3-alpha.2.1", "0.2.3-alpha.2", true),
            ("0.2.3-alpha.2", "0.2.3-alpha.2.1", false),
            ("0.2.2-rc.5", "0.2.3-alpha.1", false),
            ("0.2.3", "0.2.3-alpha.2", true),
            ("0.2.3-alpha.2", "0.2.3", false),
            ("0.2.10", "0.2.9", true),
            ("0.2.3-alpha.1", "0.2.3-beta.1", false),
            ("0.2.3-1", "0.2.3-alpha", false),
            ("0.2.3-alpha", "0.2.3-1", true),
            // Build metadata never changes precedence.
            ("0.2.3+abc", "0.2.3-alpha.2", true),
            ("0.2.3-alpha.2+abc", "0.2.3-alpha.2", true),
            ("0.2.3-alpha.1+abc", "0.2.3-alpha.2", false),
            ("0.2.3-alpha.2", "0.2.3-alpha.2+build.7", true),
        ] {
            assert_eq!(
                version_at_least(found, floor),
                expected,
                "{found} >= {floor}"
            );
        }
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
    #[cfg(unix)]
    #[test]
    fn tool_invoking_commands_still_require_the_executable() {
        const CHILD: &str = "TMAX_MISSING_COMMAND_CHILD";
        if std::env::var_os(CHILD).is_none() {
            let empty = tempfile::tempdir().unwrap();
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "agent_integration::installation::tests::tool_invoking_commands_still_require_the_executable",
                    "--nocapture",
                ])
                .env(CHILD, "1")
                .env("PATH", empty.path())
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "isolated native command check failed: {}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        let directory = tempfile::tempdir().unwrap();
        let error = native_command("pi", directory.path()).unwrap_err();
        assert_eq!(error.code, "native_not_installed");
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
