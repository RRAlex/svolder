// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `passphrase`: the one sanctioned path to function 250, asked for on a terminal first.

use std::io::IsTerminal;
use std::process::ExitCode;

use crate::backend::Globals;

use super::{ask, open};

/// Set the device's passphrase to `code` (four digits, or `none` to clear it), the only
/// sanctioned path to function 250.
///
/// A TTY without `--yes` is asked to confirm, naming the code that will be set (or that the
/// passphrase will be cleared); anything but `y`/`yes` cancels with exit 1. A non-TTY stdin
/// without `--yes` refuses outright, so a script cannot change the passphrase by accident. A
/// direct session is closed before returning, on every path.
pub async fn passphrase_set(g: &Globals<'_>, code: &str, yes: bool) -> Result<ExitCode, String> {
    let clearing = code.trim() == "none";
    if !yes {
        if std::io::stdin().is_terminal() {
            if !confirm(code, clearing).map_err(|e| e.to_string())? {
                eprintln!("svolder: cancelled");
                return Ok(ExitCode::from(1));
            }
        } else {
            eprintln!(
                "svolder: refusing to change the passphrase without confirmation; pass --yes"
            );
            return Ok(ExitCode::from(1));
        }
    }
    let mut backend = match open(g).await {
        Ok(backend) => backend,
        Err(exit) => return Ok(exit),
    };
    let outcome = backend.passphrase(code).await;
    backend.close().await;
    match outcome {
        Ok(()) => {
            if clearing {
                eprintln!("svolder: passphrase cleared");
            } else {
                eprintln!("svolder: passphrase set; tell the Forkbeard app the same code");
            }
            Ok(ExitCode::SUCCESS)
        }
        Err(e) => Ok(e.failure().report()),
    }
}

/// Ask on stderr and read one line from stdin; true only for `y`/`yes`, case-insensitive.
fn confirm(code: &str, clearing: bool) -> std::io::Result<bool> {
    let question = if clearing {
        "Clear the passphrase? The official Forkbeard app must be told the same code. [y/N]"
            .to_string()
    } else {
        format!(
            "Set the passphrase to {code}? The official Forkbeard app must be told the same \
             code. [y/N]"
        )
    };
    ask(&question)
}
