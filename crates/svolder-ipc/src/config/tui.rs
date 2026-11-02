// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The writer of the `[tui]` table, which the TUI's settings screen saves a change with:
//! each key, the `effects` switches, the family accents and the spectrum stops written in
//! the shape a hand edit would use.

use std::path::Path;

use toml_edit::{DocumentMut, Item, Table, Value};

use super::edit::{clear_key_at, edit_file, set_key_at, table_at, table_at_mut};
use super::template::{UNSET, stops_value, template_stops};
use super::{
    Config, ConfigError, EffectsConfig, FamilyAccents, TUI_SETTINGS, TuiConfig, TuiSettingKind,
};

impl Config {
    /// Change the `[tui]` table of the file at `path` with `edit`, and rewrite only the keys
    /// that change alters.
    ///
    /// This is what the TUI's settings screen saves a change with. The table `edit` is handed
    /// is the one the file holds at this moment, read as [`Config::parse`] reads it (a file
    /// with no `[tui]` gives the defaults), not the session's: a key edited by hand while the
    /// TUI runs stays as it was edited, and a file that has been deleted comes back as
    /// [`Config::template`] with the one change. Every key whose value `edit` leaves alone is
    /// left alone in the text too, quote style, comments and layout included, and so is
    /// everything outside `[tui]`; an edit that changes nothing leaves the file byte for byte.
    /// The keys it does change are written in the shape a hand edit would use — `effects` as
    /// one word while every switch is uniformly on or off, an accent only for a family that
    /// overrides one, stops only for a spectrum of the user's own, each colour depth as its
    /// own name. A file with its own `[tui.effects]` table keeps it, with the switches that
    /// changed written one at a time instead.
    ///
    /// A missing file starts from [`Config::template`], and a missing `[tui]` table is added at
    /// the end of the document when there is something to write into it. A file that does not
    /// parse, or whose result would not, is left as it was and the error returned.
    pub fn update_tui(path: &Path, edit: impl FnOnce(&mut TuiConfig)) -> Result<(), ConfigError> {
        edit_file(path, |doc| {
            let before = Config::parse(&doc.to_string())?.tui;
            let mut after = before.clone();
            edit(&mut after);
            write_tui(doc, &before, &after)
        })
    }
}

/// Write the keys of `[tui]` in which `after` differs from `before` into `doc`, and leave
/// everything else as it was. The keys of [`TUI_SETTINGS`] go in the table's order, then `effects`,
/// the family accents and the spectrum.
///
/// `before` is the table the file holds. Keys are set one at a time by [`set_key`]: an existing
/// key keeps its place and its comments, with only the value rewritten; a key commented out at
/// its default is written where that line stood; a key the file says nothing about is
/// appended. A key whose value is the same in both is not touched, and when nothing differs
/// nor is the document. Nothing outside `[tui]` is touched.
///
/// [`set_key`]: super::edit::set_key
fn write_tui(
    doc: &mut DocumentMut,
    before: &TuiConfig,
    after: &TuiConfig,
) -> Result<(), ConfigError> {
    if before == after {
        return Ok(());
    }
    let root = doc.as_table_mut();
    if !root.contains_key("tui") {
        root.insert("tui", Item::Table(Table::new()));
    }
    for key in TUI_SETTINGS {
        match key.kind {
            TuiSettingKind::Bool { get, .. } if get(before) != get(after) => {
                set_key_at(doc, &["tui"], key.key, Value::from(get(after)))?;
            }
            TuiSettingKind::Word { get, .. } if get(before) != get(after) => {
                set_key_at(doc, &["tui"], key.key, Value::from(get(after)))?;
            }
            TuiSettingKind::Accent if before.accent != after.accent => {
                match after.accent.as_deref() {
                    Some(accent) => set_key_at(doc, &["tui"], key.key, Value::from(accent))?,
                    None => clear_key_at(doc, &["tui"], key.key, UNSET)?,
                }
            }
            _ => {}
        }
    }
    if before.effects != after.effects {
        write_effects(doc, before.effects, after.effects)?;
    }
    write_accents(doc, &before.accents, &after.accents)?;
    if before.spectrum != after.spectrum {
        write_spectrum(doc, &after.spectrum)?;
    }
    Ok(())
}

/// Write the animation switches that differ between `before` and `after` into `doc`.
///
/// A file that opened a `[tui.effects]` table of its own — the config template does — keeps it
/// and has the switches that changed written one at a time, each where the file already
/// writes it. A file that says `effects` in one word, or says nothing about it, gets the one
/// key back in the shape [`effects_value`] gives it.
fn write_effects(
    doc: &mut DocumentMut,
    before: EffectsConfig,
    after: EffectsConfig,
) -> Result<(), ConfigError> {
    let path = ["tui", "effects"];
    if table_at(doc.as_table(), &path).is_none() {
        return set_key_at(doc, &["tui"], "effects", effects_value(after));
    }
    for (switch, was, now) in [
        ("enabled", before.enabled, after.enabled),
        ("eased", before.eased, after.eased),
        ("glow", before.glow, after.glow),
        ("shimmer", before.shimmer, after.shimmer),
        ("breathing", before.breathing, after.breathing),
        ("frames", before.frames, after.frames),
        (
            "fault_presence",
            before.fault_presence,
            after.fault_presence,
        ),
        ("peak_hold", before.peak_hold, after.peak_hold),
        ("hue_drift", before.hue_drift, after.hue_drift),
        (
            "spectrum_drift",
            before.spectrum_drift,
            after.spectrum_drift,
        ),
        ("list_fade", before.list_fade, after.list_fade),
        ("mascot", before.mascot, after.mascot),
    ] {
        if was != now {
            set_key_at(doc, &path, switch, Value::from(now))?;
        }
    }
    Ok(())
}

/// Write the family accents that differ between `before` and `after` into `doc`, or take
/// out the ones no family overrides any more.
///
/// A file that already carries an `accents` table keeps whatever shape it wrote it in — the
/// config template's `[tui.accents]` header among them — and each accent that changed,
/// commented out or live, is written where it sits. A file that carries none gets no table
/// for an accent that is gone; one with an accent to write gets dotted keys, which sit among
/// the `[tui]` table's own keys rather than opening a table of their own after it.
fn write_accents(
    doc: &mut DocumentMut,
    before: &FamilyAccents,
    after: &FamilyAccents,
) -> Result<(), ConfigError> {
    for (family, was, now) in [
        ("dac", &before.dac, &after.dac),
        ("amp", &before.amp, &after.amp),
        ("preamp", &before.preamp, &after.preamp),
    ] {
        if was == now {
            continue;
        }
        let tui = table_at_mut(doc.as_table_mut(), &["tui"])
            .ok_or_else(|| ConfigError::Parse("tui is not a table".into()))?;
        if !tui.contains_key("accents") {
            if now.is_none() {
                continue;
            }
            let mut fresh = Table::new();
            fresh.set_dotted(true);
            tui.insert("accents", Item::Table(fresh));
        }
        let path = ["tui", "accents"];
        match now.as_deref() {
            Some(accent) => set_key_at(doc, &path, family, Value::from(accent))?,
            None => clear_key_at(doc, &path, family, UNSET)?,
        }
    }
    Ok(())
}

/// Write the glow's stops into `doc`, or take them out again when the palette's own are what
/// the glow travels.
///
/// A file that already carries a `spectrum` table keeps whatever shape it wrote it in (the
/// config template's `[tui.spectrum]` header among them), with its stops written where they
/// sit, or put back as the commented line the template shows. A file with no such table and no
/// stops named gets none; one with stops to write gets a dotted key, sitting among the `[tui]`
/// table's own keys rather than opening a table of its own after it.
fn write_spectrum(doc: &mut DocumentMut, stops: &[String]) -> Result<(), ConfigError> {
    let tui = table_at_mut(doc.as_table_mut(), &["tui"])
        .ok_or_else(|| ConfigError::Parse("tui is not a table".into()))?;
    if !tui.contains_key("spectrum") {
        if stops.is_empty() {
            return Ok(());
        }
        let mut fresh = Table::new();
        fresh.set_dotted(true);
        tui.insert("spectrum", Item::Table(fresh));
    }
    let path = ["tui", "spectrum"];
    match stops.is_empty() {
        true => clear_key_at(doc, &path, "stops", stops_value(&template_stops())),
        false => set_key_at(doc, &path, "stops", stops_value(stops)),
    }
}

/// The `effects` key as the file writes it: one word while every switch is on or every switch
/// is off, and an inline table naming all twelve otherwise.
fn effects_value(effects: EffectsConfig) -> Value {
    use toml_edit::InlineTable;

    if effects == EffectsConfig::default() {
        return Value::from(true);
    }
    if effects == EffectsConfig::off() {
        return Value::from(false);
    }
    let mut table = InlineTable::new();
    for (switch, on) in [
        ("enabled", effects.enabled),
        ("eased", effects.eased),
        ("glow", effects.glow),
        ("shimmer", effects.shimmer),
        ("breathing", effects.breathing),
        ("frames", effects.frames),
        ("fault_presence", effects.fault_presence),
        ("peak_hold", effects.peak_hold),
        ("hue_drift", effects.hue_drift),
        ("spectrum_drift", effects.spectrum_drift),
        ("list_fade", effects.list_fade),
        ("mascot", effects.mascot),
    ] {
        table.insert(switch, Value::from(on));
    }
    Value::InlineTable(table)
}
