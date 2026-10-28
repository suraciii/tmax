mod grammar;

use std::{
    ffi::{OsStr, OsString},
    io::{self, Write},
    process::ExitCode,
};

use clap::error::{ContextKind, ContextValue, ErrorKind};
use serde_json::json;
use tmax_core::{Response, service};

#[tokio::main]
async fn main() -> ExitCode {
    let argv: Vec<OsString> = std::env::args_os().collect();
    if argv
        .get(1)
        .is_some_and(|arg| arg.as_os_str() == OsStr::new("__autostart"))
    {
        return run_autostart(&argv).await;
    }
    if argv
        .get(1)
        .is_some_and(|arg| arg.as_os_str() == OsStr::new("__daemon"))
    {
        return run_daemon(&argv).await;
    }

    match run_management(argv).await {
        Ok(code) => ExitCode::from(code),
        Err(_) => ExitCode::from(4),
    }
}

async fn run_daemon(argv: &[OsString]) -> ExitCode {
    if argv.len() != 2 {
        eprintln!("Invalid internal service invocation.");
        return ExitCode::from(2);
    }
    match service::run_daemon().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => {
            eprintln!("The daemon service could not start.");
            ExitCode::from(4)
        }
    }
}
async fn run_autostart(argv: &[OsString]) -> ExitCode {
    if argv.len() != 2 {
        eprintln!("Invalid internal service invocation.");
        return ExitCode::from(2);
    }
    match service::run_autostart().await {
        Ok(_result) => ExitCode::SUCCESS,
        Err(_) => {
            eprintln!("The configured gateway could not start from autostart.");
            ExitCode::from(4)
        }
    }
}

async fn run_management(argv: Vec<OsString>) -> io::Result<u8> {
    let root_command = grammar::command();
    let (output_format, duplicate_global) =
        grammar::OutputFormat::scan_global_options(&argv, &root_command);

    if argv.len() == 1 {
        let mut command = root_command;
        command.print_long_help()?;
        io::stdout().write_all(b"\n")?;
        return Ok(0);
    }

    let matches = match root_command.clone().try_get_matches_from(argv.clone()) {
        Ok(matches) => matches,
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            ) =>
        {
            if let Some(option) = duplicate_global {
                let response = duplicate_global_response(&root_command, &argv, option);
                let format = if option == "--format" {
                    grammar::OutputFormat::Json
                } else {
                    output_format
                };
                return write_response(&response, format);
            }
            io::stdout().write_all(error.to_string().as_bytes())?;
            return Ok(0);
        }
        Err(error) => {
            let operation = grammar::operation_hint(&root_command, &argv);
            let response = parse_error_response(&root_command, operation, &error);
            return write_response(&response, output_format);
        }
    };
    if let Some(option) = duplicate_global {
        let response = duplicate_global_response(&root_command, &argv, option);
        let format = if option == "--format" {
            grammar::OutputFormat::Json
        } else {
            output_format
        };
        return write_response(&response, format);
    }

    if let Some(("help", help_matches)) = matches.subcommand() {
        let path = grammar::help_path(help_matches);
        if let Some(mut help_command) = grammar::help_target(&root_command, &path) {
            help_command.print_long_help()?;
            io::stdout().write_all(b"\n")?;
            return Ok(0);
        }
        let response = Response {
            exit_code: 2,
            body: json!({
                "schema_version": 1,
                "operation": null,
                "code": "invalid_arguments",
                "status": "error",
                "data": null,
                "message": "Unknown command in offline help path. Run `tmax --help` for supported commands.",
            }),
        };
        return write_response(&response, output_format);
    }

    let response = tmax_core::execute(grammar::request(&root_command, &matches)).await;
    write_response(&response, output_format)
}

fn parse_error_response(
    command: &clap::Command,
    operation: Option<String>,
    error: &clap::Error,
) -> Response {
    let message = parse_error_message(command, operation.as_deref(), error);
    Response {
        exit_code: 2,
        body: json!({
            "schema_version": 1,
            "operation": operation,
            "code": "invalid_arguments",
            "status": "error",
            "data": null,
            "message": message,
        }),
    }
}

fn duplicate_global_response(command: &clap::Command, argv: &[OsString], option: &str) -> Response {
    Response {
        exit_code: 2,
        body: json!({
            "schema_version": 1,
            "operation": grammar::operation_hint(command, argv),
            "code": "invalid_arguments",
            "status": "error",
            "data": null,
            "message": format!("Global option {option} may be supplied only once."),
        }),
    }
}

fn parse_error_message(
    command: &clap::Command,
    operation: Option<&str>,
    error: &clap::Error,
) -> String {
    let location = operation
        .map(|path| format!("`tmax {path}`"))
        .unwrap_or_else(|| "`tmax`".to_owned());
    let detail = match error.kind() {
        ErrorKind::UnknownArgument => match error.get(ContextKind::InvalidArg) {
            Some(ContextValue::String(value)) => known_option_name(command, value)
                .map(|name| format!("Unknown option {name}."))
                .unwrap_or_else(|| "Unexpected argument or option.".to_owned()),
            _ => "Unexpected argument or option.".to_owned(),
        },
        ErrorKind::InvalidSubcommand | ErrorKind::MissingSubcommand => {
            format!("Unknown or missing subcommand under {location}.")
        }
        ErrorKind::MissingRequiredArgument
        | ErrorKind::TooFewValues
        | ErrorKind::WrongNumberOfValues => match error.get(ContextKind::InvalidArg) {
            Some(ContextValue::String(value)) => known_option_name(command, value)
                .map(|name| format!("Missing or incomplete value for {name}."))
                .unwrap_or_else(|| "A required operand or option value is missing.".to_owned()),
            Some(ContextValue::Strings(values)) => {
                let options: Vec<_> = values
                    .iter()
                    .filter_map(|value| known_option_name(command, value))
                    .collect();
                if options.is_empty() {
                    "A required operand or option value is missing.".to_owned()
                } else {
                    format!("Missing required value for {}.", options.join(", "))
                }
            }
            _ => "A required operand or option value is missing.".to_owned(),
        },
        ErrorKind::InvalidValue | ErrorKind::ValueValidation | ErrorKind::NoEquals => {
            match error.get(ContextKind::InvalidArg) {
                Some(ContextValue::String(value)) => known_option_name(command, value)
                    .map(|name| format!("Invalid value or option form for {name}."))
                    .unwrap_or_else(|| "Invalid command value.".to_owned()),
                _ => "Invalid command value.".to_owned(),
            }
        }
        ErrorKind::ArgumentConflict | ErrorKind::TooManyValues => {
            let option = error
                .get(ContextKind::InvalidArg)
                .and_then(|value| context_option(command, value))
                .or_else(|| {
                    error
                        .get(ContextKind::PriorArg)
                        .and_then(|value| context_option(command, value))
                });
            option
                .map(|name| format!("Conflicting or excess argument near {name}."))
                .unwrap_or_else(|| "Conflicting or excess command arguments.".to_owned())
        }
        _ => "Invalid command arguments.".to_owned(),
    };
    match operation {
        Some(path) => format!("{detail} Run `tmax {path} --help` for offline help."),
        None => format!("{detail} Run `tmax --help` for offline help."),
    }
}

fn context_option(command: &clap::Command, value: &ContextValue) -> Option<String> {
    match value {
        ContextValue::String(value) => known_option_name(command, value),
        ContextValue::Strings(values) => values
            .iter()
            .find_map(|value| known_option_name(command, value)),
        _ => None,
    }
}

fn known_option_name(command: &clap::Command, value: &str) -> Option<String> {
    let name = safe_option_name(value)?;
    option_is_registered(command, &name).then_some(name)
}

fn option_is_registered(command: &clap::Command, name: &str) -> bool {
    let long = name.strip_prefix("--");
    let short = name
        .strip_prefix('-')
        .filter(|value| !value.starts_with('-'));
    command.get_arguments().any(|argument| {
        long.is_some_and(|value| argument.get_long() == Some(value))
            || short
                .and_then(|value| {
                    let mut chars = value.chars();
                    let short = chars.next()?;
                    chars.next().is_none().then_some(short)
                })
                .is_some_and(|value| argument.get_short() == Some(value))
    }) || command
        .get_subcommands()
        .any(|child| option_is_registered(child, name))
}

fn safe_option_name(value: &str) -> Option<String> {
    let token = value.split_whitespace().next().unwrap_or(value);
    let name = token.split_once('=').map_or(token, |(name, _)| name);
    let (prefix, body) = name
        .strip_prefix("--")
        .map(|body| ("--", body))
        .or_else(|| name.strip_prefix('-').map(|body| ("-", body)))?;
    if body.is_empty()
        || !body
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return None;
    }
    Some(format!("{prefix}{body}"))
}

fn write_response(response: &Response, format: grammar::OutputFormat) -> io::Result<u8> {
    let mut stdout = io::stdout().lock();
    let mut stderr = io::stderr().lock();
    if format == grammar::OutputFormat::Json {
        serde_json::to_writer(&mut stdout, &response.body).map_err(io::Error::other)?;
        stdout.write_all(b"\n")?;
    } else {
        let rendered = tmax_core::render_text(response);
        if response.exit_code == 0 {
            stdout.write_all(rendered.as_bytes())?;
            stdout.write_all(b"\n")?;
        } else {
            stderr.write_all(rendered.as_bytes())?;
            stderr.write_all(b"\n")?;
        }
    }
    Ok(response.exit_code)
}
