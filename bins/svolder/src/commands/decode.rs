// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `svolder decode`: explain a captured frame.

use serde::Serialize;
use svolder_profiles::{Profile, Registry, describe};
use svolder_proto::{Direction, Notification, decode_command, decode_notification};

/// The JSON `raw` field for a message, the same one the wire protocol writes.
pub(crate) use svolder_ipc::raw_json;

#[derive(Serialize)]
struct Output {
    /// Present when the bytes were a command frame (passphrase prefix + body).
    passphrase_field: Option<String>,
    messages: Vec<Line>,
    /// One entry per message that failed to parse.
    dropped: Vec<String>,
}

#[derive(Serialize)]
struct Line {
    direction: &'static str,
    id: String,
    value_type: String,
    raw: serde_json::Value,
    label: String,
    key: Option<String>,
    display: String,
    /// The converted numeric value, when the control has a conversion.
    value: Option<f64>,
}

pub fn run(hex_text: &str, profile_name: Option<&str>, json: bool) -> Result<(), String> {
    let bytes = parse_hex(hex_text)?;
    let (passphrase_field, notification) = decode_any(&bytes)?;
    let profile = match profile_name {
        Some(name) => Some(load_profile(name)?),
        None => None,
    };
    let generic = Registry::generic(0);
    let profile_ref = profile.as_ref().unwrap_or(&generic);
    let lines = message_lines(&notification, profile_ref);
    let dropped = dropped_errors(&notification);
    if json {
        let out = Output {
            passphrase_field: passphrase_field.map(|p| format!("{p:#010x}")),
            messages: lines,
            dropped,
        };
        println!(
            "{}",
            serde_json::to_string_pretty(&out).map_err(|e| e.to_string())?
        );
        return Ok(());
    }
    for line in text_lines(passphrase_field, &notification, &lines, &dropped) {
        println!("{line}");
    }
    Ok(())
}

/// The text lines `svolder decode` prints for one frame, read with `profile`'s names.
///
/// A frame that does not decode is one line saying why.
pub(crate) fn describe_frame(bytes: &[u8], profile: &Profile) -> Vec<String> {
    match decode_any(bytes) {
        Ok((passphrase_field, notification)) => {
            let lines = message_lines(&notification, profile);
            let dropped = dropped_errors(&notification);
            text_lines(passphrase_field, &notification, &lines, &dropped)
        }
        Err(error) => vec![format!("does not decode: {error}")],
    }
}

/// The same for a frame the host sent, which is a command frame whatever its first bytes say:
/// with no passphrase they are zeros, which read as the length of a notification's first message.
pub(crate) fn describe_sent_frame(bytes: &[u8], profile: &Profile) -> Vec<String> {
    match decode_command(bytes) {
        Ok(command) => {
            let lines = message_lines(&command.body, profile);
            let dropped = dropped_errors(&command.body);
            text_lines(
                Some(command.passphrase_field),
                &command.body,
                &lines,
                &dropped,
            )
        }
        Err(error) => vec![format!("does not decode: {error}")],
    }
}

fn message_lines(notification: &Notification, profile: &Profile) -> Vec<Line> {
    notification
        .messages
        .iter()
        .map(|m| {
            let reading = describe(profile, m);
            Line {
                direction: match m.direction {
                    Direction::Command => "Command",
                    Direction::Status => "Status",
                },
                id: m.id.to_string(),
                value_type: m.value.value_type().to_string(),
                raw: raw_json(m),
                label: reading.label,
                key: reading.key,
                display: reading.display,
                value: reading.value,
            }
        })
        .collect()
}

fn dropped_errors(notification: &Notification) -> Vec<String> {
    notification
        .dropped
        .iter()
        .map(ToString::to_string)
        .collect()
}

fn text_lines(
    passphrase_field: Option<u32>,
    notification: &Notification,
    lines: &[Line],
    dropped: &[String],
) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(p) = passphrase_field {
        out.push(format!("command frame, passphrase field {p:#010x}"));
    }
    for (m, line) in notification.messages.iter().zip(lines) {
        let named = if line.label != "?" {
            format!("   {} = {}", line.label, line.display)
        } else {
            String::new()
        };
        out.push(format!(
            "{} {} {} {}{}",
            line.direction, line.id, line.value_type, m.value, named
        ));
    }
    out.extend(dropped.iter().map(|error| format!("dropped: {error}")));
    out
}

/// Read a frame's hex bytes, whitespace and `:` separators allowed.
fn parse_hex(text: &str) -> Result<Vec<u8>, String> {
    let clean: String = text
        .chars()
        .filter(|c| !c.is_whitespace() && *c != ':')
        .collect();
    if clean.is_empty()
        || !clean.len().is_multiple_of(2)
        || !clean.chars().all(|c| c.is_ascii_hexdigit())
    {
        return Err(format!("{text:?} is not hex"));
    }
    (0..clean.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&clean[i..i + 2], 16).map_err(|e| e.to_string()))
        .collect()
}

/// Try the bytes as a notification, then as a command frame with its 4-byte passphrase prefix.
///
/// A notification that holds no message is no reading of the bytes when they decode as a command
/// frame: a command frame with no passphrase begins with four zeros, which a notification reads
/// as a message of length zero.
fn decode_any(bytes: &[u8]) -> Result<(Option<u32>, Notification), String> {
    let read = match decode_notification(bytes) {
        Ok(notification) if !notification.messages.is_empty() => return Ok((None, notification)),
        other => other,
    };
    match decode_command(bytes) {
        Ok(command) => Ok((Some(command.passphrase_field), command.body)),
        Err(_) => read
            .map(|notification| (None, notification))
            .map_err(|error| error.to_string()),
    }
}

fn load_profile(name: &str) -> Result<Profile, String> {
    let registry = crate::registry()?;
    registry
        .get(name)
        .cloned()
        .ok_or_else(|| format!("no profile named {name}; try `svolder profiles list`"))
}

#[cfg(test)]
mod tests;
