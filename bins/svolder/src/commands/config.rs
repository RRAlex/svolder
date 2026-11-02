// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The `config` verbs: show the loaded configuration, and print the template a new one starts
//! from.
//!
//! Neither verb opens a Bluetooth transport or a daemon connection; both work from
//! `Config::path`/`Config::load` alone.

use std::process::ExitCode;

use svolder_ipc::Config;
use svolder_ipc::config::{
    HopWhat, McpAuth, McpConfig, MqttConfig, REDACTED, WebAuth, WebConfig, WebGuest,
};

use crate::cli::ConfigCommand;

/// Carry out one of the `config` sub-commands.
pub(crate) fn run(command: ConfigCommand) -> Result<ExitCode, String> {
    match command {
        ConfigCommand::Path => {
            println!("{}", crate::config_dir().display());
            Ok(ExitCode::SUCCESS)
        }
        ConfigCommand::Show { json } => show(json),
        ConfigCommand::Template => {
            template();
            Ok(ExitCode::SUCCESS)
        }
    }
}

/// Print the config file's path, then either `no config file` or a readable rendering of
/// what `Config::load` parsed. `--json` prints the loaded `Config` as one JSON object
/// instead of the path and the rendering.
fn show(json: bool) -> Result<ExitCode, String> {
    if json {
        let config = Config::load().map_err(|e| e.to_string())?;
        crate::backend::print_warnings(&config);
        println!(
            "{}",
            serde_json::to_string(&config).map_err(|e| e.to_string())?
        );
        return Ok(ExitCode::SUCCESS);
    }
    let path = Config::path();
    println!("{}", path.display());
    if !path.exists() {
        println!("no config file");
        return Ok(ExitCode::SUCCESS);
    }
    let config = Config::load().map_err(|e| e.to_string())?;
    crate::backend::print_warnings(&config);
    print_readable(&config);
    Ok(ExitCode::SUCCESS)
}

/// Print a loaded config the way a person reads it: the top-level settings that are set, the
/// `[mqtt]`, `[mcp]` and `[web]` blocks when the file changes anything in them, then one block
/// per configured device and one per hop.
fn print_readable(config: &Config) {
    if let Some(adapter) = &config.adapter {
        println!("adapter = {adapter}");
    }
    if let Some(default_device) = &config.default_device {
        println!("default_device = {default_device}");
    }
    if let Some(socket) = &config.socket {
        println!("socket = {socket}");
    }
    if config.mqtt != MqttConfig::default() {
        println!();
        for line in mqtt_lines(&config.mqtt) {
            println!("{line}");
        }
    }
    if config.mcp != McpConfig::default() {
        println!();
        for line in mcp_lines(&config.mcp) {
            println!("{line}");
        }
    }
    if config.web != WebConfig::default() {
        println!();
        for line in web_lines(&config.web) {
            println!("{line}");
        }
    }
    for device in &config.devices {
        println!();
        println!("[[device]]");
        println!("name = {}", device.name);
        println!("udid = {}", device.udid);
        if let Some(profile) = &device.profile {
            println!("profile = {profile}");
        }
        println!("passphrase = {}", device.passphrase);
        if !device.inputs.is_empty() {
            println!("inputs:");
            for (key, value) in &device.inputs {
                println!("  {key} = {value}");
            }
        }
    }
    for hop in &config.hops {
        println!();
        println!("[[hop]]");
        match &hop.what {
            HopWhat::Declared { kind, name } => {
                println!("name = {name}");
                println!("kind = {}", kind.name());
            }
            HopWhat::Linked { device } => println!("device = {device}"),
        }
        if let Some(note) = &hop.note {
            println!("note = {note}");
        }
        if let Some(after) = &hop.after {
            println!("after = {after}");
        }
    }
}

/// The `[mqtt]` block as `config show` prints it: every key, the optional ones only when set,
/// and the password as `***`.
fn mqtt_lines(mqtt: &MqttConfig) -> Vec<String> {
    let mut lines = vec!["[mqtt]".to_string(), format!("enabled = {}", mqtt.enabled)];
    let optional = [
        ("host", mqtt.host.clone()),
        ("port", Some(mqtt.port.to_string())),
        ("username", mqtt.username.clone()),
        (
            "password",
            mqtt.password.as_ref().map(|_| REDACTED.to_string()),
        ),
        ("client_id", mqtt.client_id.clone()),
        ("base_topic", Some(mqtt.base_topic.clone())),
        ("discovery_prefix", Some(mqtt.discovery_prefix.clone())),
        ("commands", Some(mqtt.commands.to_string())),
        (
            "max_volume_db",
            mqtt.max_volume_db.map(|cap| cap.to_string()),
        ),
    ];
    lines.extend(
        optional
            .into_iter()
            .filter_map(|(key, value)| value.map(|value| format!("{key} = {value}"))),
    );
    lines
}

/// The `[web]` block as `config show` prints it: every key, the optional ones only when set.
fn web_lines(web: &WebConfig) -> Vec<String> {
    let auth = match web.auth {
        WebAuth::Token => "token",
        WebAuth::None => "none",
    };
    let guest = match web.guest {
        WebGuest::None => "none",
        WebGuest::Read => "read",
    };
    let mut lines = vec![
        "[web]".to_string(),
        format!("enabled = {}", web.enabled),
        format!("listen = {}", web.listen),
        format!("auth = {auth}"),
        format!("writes = {}", web.writes),
        format!("guest = {guest}"),
    ];
    if let Some(cap) = web.max_volume_db {
        lines.push(format!("max_volume_db = {cap}"));
    }
    if let Some(url) = &web.url {
        lines.push(format!("url = {url}"));
    }
    if !web.hosts.is_empty() {
        lines.push(format!("hosts = {}", web.hosts.join(", ")));
    }
    if !web.origins.is_empty() {
        lines.push(format!("origins = {}", web.origins.join(", ")));
    }
    if let Some(file) = &web.token_file {
        lines.push(format!("token_file = {file}"));
    }
    lines
}

/// The `[mcp]` block as `config show` prints it. The token is not part of the configuration,
/// so there is nothing in it to hide; `token_file` is a path.
fn mcp_lines(mcp: &McpConfig) -> Vec<String> {
    let auth = match mcp.auth {
        McpAuth::Token => "token",
        McpAuth::None => "none",
    };
    let mut lines = vec![
        "[mcp]".to_string(),
        format!("enabled = {}", mcp.enabled),
        format!("listen = {}", mcp.listen),
        format!("auth = {auth}"),
        format!("writes = {}", mcp.writes),
    ];
    if let Some(cap) = mcp.max_volume_db {
        lines.push(format!("max_volume_db = {cap}"));
    }
    if !mcp.origins.is_empty() {
        lines.push(format!("origins = {}", mcp.origins.join(", ")));
    }
    if !mcp.hosts.is_empty() {
        lines.push(format!("hosts = {}", mcp.hosts.join(", ")));
    }
    if let Some(file) = &mcp.token_file {
        lines.push(format!("token_file = {file}"));
    }
    lines
}

/// Print a whole configuration file with every key commented out at the value svolder uses
/// while the key is absent — the same text a file svolder creates for the first time starts
/// from.
fn template() {
    print!("{}", Config::template());
}
