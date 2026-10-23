use std::process::{Command, Output};

fn tmax(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tmax"))
        .args(args)
        .output()
        .expect("launch tmax")
}

fn json_error(output: &Output) -> serde_json::Value {
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stderr.is_empty());
    let response: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["schema_version"], 1);
    assert_eq!(response["status"], "error");
    assert_eq!(response["code"], "invalid_arguments");
    assert_eq!(response["data"], serde_json::Value::Null);
    response
}

#[test]
fn unknown_secret_option_tokens_are_redacted() {
    for option in ["--sk-CLI-SECRET-MUST-NOT-ECHO", "-CLI-SECRET-MUST-NOT-ECHO"] {
        let output = tmax(&[
            "provider",
            "inspect",
            "safe-provider",
            option,
            "--format",
            "json",
        ]);
        let response = json_error(&output);
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert_eq!(response["operation"], "provider inspect");
        assert!(!stdout.contains(option));
        assert!(!stdout.contains("CLI-SECRET-MUST-NOT-ECHO"));
    }
}
#[test]
fn invalid_enum_names_its_option_without_echoing_value() {
    let output = tmax(&[
        "provider",
        "auth",
        "set",
        "safe-provider",
        "--method",
        "INVALID-METHOD-SECRET",
        "--via",
        "tmax",
        "--format",
        "json",
    ]);
    let response = json_error(&output);
    assert_eq!(response["operation"], "provider auth set");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("--method"));
    assert!(!stdout.contains("INVALID-METHOD-SECRET"));
}

#[test]
fn unknown_commands_and_extra_positionals_are_not_echoed() {
    let output = tmax(&["proxy", "UNRECOGNIZED-SUBCOMMAND-SECRET"]);
    let response = json_error(&output);
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(!stdout.contains("UNRECOGNIZED-SUBCOMMAND-SECRET"));
    assert_eq!(response["operation"], "proxy");

    let output = tmax(&["status", "EXTRA-POSITIONAL-SECRET"]);
    let response = json_error(&output);
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(!stdout.contains("EXTRA-POSITIONAL-SECRET"));
    assert_eq!(response["operation"], "status");
}

#[test]
fn text_parse_errors_after_subcommands_use_stderr_and_redact_values() {
    let output = tmax(&[
        "provider",
        "inspect",
        "safe-provider",
        "--sk-TEXT-SECRET-MUST-NOT-ECHO",
        "--format",
        "text",
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(!stderr.contains("--sk-TEXT-SECRET-MUST-NOT-ECHO"));
    assert!(!stderr.contains("TEXT-SECRET-MUST-NOT-ECHO"));
}

#[test]
fn guarded_replacements_require_revision_before_dispatch() {
    let output = tmax(&[
        "model",
        "reasoning",
        "replace",
        "deepseek/deepseek-flash",
        "0",
        "--type",
        "effort",
    ]);
    let response = json_error(&output);
    assert_eq!(response["operation"], "model reasoning replace");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("if-revision"));
    assert!(!stdout.contains("deepseek/deepseek-flash"));
}

#[test]
fn rejects_duplicate_proxy_lifecycle_path() {
    let output = tmax(&["proxy", "proxy", "start"]);
    let response = json_error(&output);
    assert_eq!(response["operation"], "proxy");
}

#[test]
fn invalid_or_duplicate_format_falls_back_to_json() {
    for args in [
        vec!["status", "--format", "PRIVATE-FORMAT-SECRET"],
        vec!["status", "--format", "text", "--format", "json"],
    ] {
        let output = tmax(&args);
        let response = json_error(&output);
        assert_eq!(response["operation"], "status");
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(!stdout.contains("PRIVATE-FORMAT-SECRET"));
    }
}

#[test]
fn hidden_service_launchers_reject_extra_args() {
    for launcher in ["__resident", "__autostart"] {
        let output = tmax(&[launcher, "LAUNCH-SECRET-MUST-NOT-ECHO"]);
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(!stderr.contains("LAUNCH-SECRET-MUST-NOT-ECHO"));
    }
}
