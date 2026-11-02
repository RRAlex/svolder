// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `svolder profiles list|show`.

use std::process::ExitCode;

use svolder_profiles::Zero;

use crate::cli::ProfilesCommand;

/// Carry out one of the `profiles` sub-commands.
pub(crate) fn run(command: ProfilesCommand) -> Result<ExitCode, String> {
    match command {
        ProfilesCommand::List => list().map(|()| ExitCode::SUCCESS),
        ProfilesCommand::Show { name } => show(&name).map(|()| ExitCode::SUCCESS),
    }
}

fn list() -> Result<(), String> {
    let registry = crate::registry()?;
    let mut names = registry.names();
    names.sort_unstable();
    for name in names {
        let p = registry.get(name).ok_or_else(|| name.to_string())?;
        match p.product_id {
            Some(id) => println!(
                "{name} product {id} firmware>={} {}",
                p.min_firmware, p.display
            ),
            None => println!("{name} (base)"),
        }
    }
    Ok(())
}

fn show(name: &str) -> Result<(), String> {
    let registry = crate::registry()?;
    let p = registry
        .get(name)
        .ok_or_else(|| format!("no profile named {name}; try `svolder profiles list`"))?;
    println!("{} ({})", p.display, p.name);
    if let Some(id) = p.product_id {
        println!("product {id}, firmware >= {}", p.min_firmware);
    }
    println!();
    // The key column is as wide as the longest key shown, and never narrower than 22.
    let w = p
        .controls
        .iter()
        .map(|c| c.key.len())
        .max()
        .unwrap_or(0)
        .max(22);
    println!("{:<9} {:<w$} {:<8} label", "id", "key", "kind");
    for c in &p.controls {
        let kind = format!("{:?}", c.kind).to_ascii_lowercase();
        println!(
            "{:<9} {:<w$} {:<8} {}",
            c.id.to_string(),
            c.key,
            kind,
            c.label
        );
        if !c.values.is_empty() {
            let names: Vec<String> = c.values.iter().map(|(v, n)| format!("{v}={n}")).collect();
            println!("{:<9} {:<w$} {:<8}   {}", "", "", "", names.join(", "));
        }
        if let Some((lo, hi)) = c.range {
            println!("{:<9} {:<w$} {:<8}   range {lo}..={hi}", "", "", "");
        }
        if !c.reads.is_empty() {
            let ids: Vec<String> = c.reads.iter().map(ToString::to_string).collect();
            println!("{:<9} {:<w$} {:<8}   reads {}", "", "", "", ids.join(", "));
        }
        if let Some(scale) = &c.log_scale {
            let zero = match &scale.zero {
                Zero::Fixed(raw) => format!("zero {raw}"),
                Zero::Floor(key) => format!("zero from {key}"),
            };
            println!(
                "{:<9} {:<w$} {:<8}   scale {zero}, full {}, class-A {}",
                "", "", "", scale.full, scale.class_a
            );
        }
    }
    for a in &p.actions {
        println!(
            "{:<9} {:<w$} {:<8} {}",
            format!("{}.<{}>", a.id, a.slots_word()),
            a.key,
            "action",
            a.label
        );
        if a.overwrites {
            println!(
                "{:<9} {:<w$} {:<8}   overwrites what the device holds",
                "", "", ""
            );
        }
        if !a.changes.is_empty() {
            println!(
                "{:<9} {:<w$} {:<8}   changes {}",
                "",
                "",
                "",
                a.changes.join(", ")
            );
        }
    }
    if let Some(table) = &p.builtin_settings {
        println!();
        println!("built-in settings of {}", table.keys.join(", "));
        let ids: Vec<String> = table.ids.iter().map(ToString::to_string).collect();
        println!("  written to {}", ids.join(", "));
        let name_width = table
            .settings
            .iter()
            .map(|s| s.name.chars().count())
            .max()
            .unwrap_or(0);
        for setting in &table.settings {
            let raw: Vec<String> = setting.raw.iter().map(ToString::to_string).collect();
            println!("  {:<name_width$}  {}", setting.name, raw.join(" "));
        }
    }
    if let Some(eq) = &p.eq {
        println!();
        println!("eq on function {}: {} bands", eq.id, eq.bands.len());
        for (i, b) in eq.bands.iter().enumerate() {
            println!("  band {} centre {} Hz", i + 1, b.center);
        }
    }
    Ok(())
}
