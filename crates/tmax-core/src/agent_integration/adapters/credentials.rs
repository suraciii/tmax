use super::super::document::Format;
use super::{Entry, File, dsh, hermes};
use crate::{Error, Result};
use serde_json::Value;
use std::path::{Path, PathBuf};

pub(super) fn credential_path(scope: &Path, entry: &Entry) -> PathBuf {
    use sha2::{Digest, Sha256};
    scope.join(format!(
        "tmax-{:x}.key",
        Sha256::digest(format!("{}\0{}", entry.name, entry.protocol).as_bytes())
    ))
}
pub(super) fn key_command(path: &Path) -> Result<String> {
    let path = path.to_str().ok_or_else(|| {
        Error::new(
            "invalid_native_scope",
            "Native credential references require a UTF-8 path",
        )
    })?;
    #[cfg(unix)]
    {
        Ok(format!("/bin/cat '{}'", path.replace('\'', "'\\''")))
    }
    #[cfg(not(unix))]
    {
        use base64::Engine;
        // Pi runs credential commands through Bash on Windows, while other
        // tools use cmd.exe. Transport the PowerShell expression without outer
        // shell expansion of backslashes, dollar signs, or quotes in the path.
        let script = format!(
            "[Console]::Out.Write([IO.File]::ReadAllText('{}'))",
            path.replace('\'', "''")
        );
        let encoded = base64::engine::general_purpose::STANDARD.encode(
            script
                .encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>(),
        );
        Ok(format!(
            "powershell.exe -NoProfile -NonInteractive -EncodedCommand {encoded}"
        ))
    }
}

pub(super) fn prepend_files(agent: &str, scope: &Path, entries: &[Entry], files: &mut Vec<File>) {
    for entry in entries {
        if !matches!(agent, "hermes" | "dsh")
            && let Some(key) = &entry.key
        {
            files.insert(
                0,
                File {
                    path: credential_path(scope, entry),
                    format: Format::Credential,
                    desired: Value::String(key.clone()),
                },
            );
        }
    }
}

pub(in crate::agent_integration) fn resolve_handoff(
    agent: &str,
    files: &mut [File],
    mut resolve: impl FnMut(&str) -> Result<Value>,
) -> Result<()> {
    for file in files {
        if file.format == Format::Credential {
            if let Some(reference) = file.desired.as_str() {
                file.desired = resolve(reference)?;
            }
        } else if agent == "dsh" {
            dsh::resolve_handoff(&mut file.desired, &mut resolve)?;
        } else if agent == "hermes" {
            hermes::resolve_handoff(&mut file.desired, &mut resolve)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[cfg(windows)]
    use super::*;
    #[cfg(windows)]
    #[test]
    fn windows_credential_command_survives_native_shells_and_literal_paths() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("key with '$value & space.key");
        std::fs::write(&path, b"synthetic-credential").unwrap();
        let path = std::fs::canonicalize(path).unwrap();
        let command = key_command(&path).unwrap();
        let mut shells = vec![("cmd.exe".to_owned(), "/c")];
        if let Some(program_files) = std::env::var_os("ProgramFiles") {
            let bash = PathBuf::from(program_files).join("Git/bin/bash.exe");
            if bash.is_file() {
                shells.push((bash.to_string_lossy().into(), "-c"));
            }
        }
        for (shell, flag) in shells {
            let output = std::process::Command::new(shell)
                .args([flag, &command])
                .output()
                .unwrap();
            assert!(output.status.success());
            assert_eq!(output.stdout, b"synthetic-credential");
        }
    }
}
