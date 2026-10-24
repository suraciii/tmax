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
fn removed_unlock_options_are_rejected_as_unknown_and_redacted() {
    for args in [
        vec!["status", "--unlock-stdin"],
        vec![
            "proxy",
            "start",
            "--unlock-fd",
            "UNLOCK-SECRET-MUST-NOT-ECHO",
        ],
    ] {
        let output = tmax(&args);
        let response = json_error(&output);
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert_eq!(response["code"], "invalid_arguments");
        assert!(!stdout.contains("UNLOCK-SECRET-MUST-NOT-ECHO"));
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
fn repeated_global_options_are_rejected_across_command_levels() {
    for args in [
        vec!["--format", "json", "provider", "list", "--format", "text"],
        vec!["provider", "--format", "json", "list", "--format", "text"],
        vec![
            "--format", "json", "provider", "--format", "json", "list", "--format", "json",
        ],
    ] {
        let output = tmax(&args);
        let response = json_error(&output);
        assert_eq!(response["operation"], "provider list");
        assert!(response["message"].as_str().unwrap().contains("--format"));
    }

    let output = tmax(&[
        "--non-interactive",
        "provider",
        "--non-interactive",
        "list",
        "--non-interactive",
        "--format",
        "text",
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("--non-interactive"));
}

#[test]
fn help_path_and_leaf_help_both_expose_inherited_global_options() {
    for args in [
        vec!["provider", "key", "set", "--help"],
        vec!["help", "provider", "key", "set"],
    ] {
        let output = tmax(&args);
        assert_eq!(output.status.code(), Some(0));
        let help = String::from_utf8(output.stdout).unwrap();
        assert!(help.contains("--format"));
        assert!(help.contains("--non-interactive"));
    }
}

#[test]
fn one_text_format_is_supported_before_or_after_the_command_path() {
    let before = tmax(&["--format", "text", "status", "--unknown"]);
    assert_eq!(before.status.code(), Some(2));
    assert!(before.stdout.is_empty());
    assert!(!before.stderr.is_empty());

    let after = tmax(&["status", "--unknown", "--format", "text"]);
    assert_eq!(after.status.code(), Some(2));
    assert!(after.stdout.is_empty());
    assert!(!after.stderr.is_empty());
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

#[test]
fn alias_creation_requires_explicit_candidates_and_rejects_retired_selectors() {
    for args in [
        vec![
            "model",
            "alias",
            "add",
            "L0",
            "--protocol",
            "chat-completions",
        ],
        vec![
            "model",
            "alias",
            "add",
            "custom",
            "--protocol",
            "chat-completions",
            "--tier",
            "L0",
        ],
        vec![
            "model",
            "alias",
            "update",
            "custom",
            "--protocol",
            "chat-completions",
            "--tier",
            "L1",
        ],
        vec![
            "model",
            "explain",
            "--tier",
            "L0",
            "--protocol",
            "chat-completions",
            "--target",
            "target",
        ],
        vec!["model", "explain", "--protocol", "chat-completions"],
    ] {
        json_error(&tmax(&args));
    }
}

#[test]
fn explicit_model_native_context_selectors_reject_ambiguous_scopes_before_dispatch() {
    for selector in [
        vec![
            "--target",
            "target",
            "--agent",
            "pi",
            "--path",
            "/unused/native",
        ],
        vec!["--agent-provider", "entry"],
        vec!["--agent", "pi"],
        vec!["--path", "/unused/native"],
    ] {
        let mut args = vec![
            "model",
            "explain",
            "custom",
            "--protocol",
            "chat-completions",
        ];
        args.extend(selector);
        json_error(&tmax(&args));
    }
}
