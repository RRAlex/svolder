// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What a scene's refusals and notes say.

use svolder_profiles::Profile;

/// What the tone control is called in an unread part ([`svolder_profiles::scene::Unread`]).
const TONE_CONTROL: &str = "the tone control";

/// `text` with the computer EQ's name `custom` shown as `Custom`: a front end never shows the
/// lower-case word the EQ names a hand-made curve by. Only the whole word is changed, and only in
/// what the computer part says (a device or a preset the user named `custom stuff` is theirs).
pub(super) fn custom_cased(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut String| {
        match word.as_str() {
            "custom" => out.push_str("Custom"),
            other => out.push_str(other),
        }
        word.clear();
    };
    for c in text.chars() {
        match c.is_alphanumeric() || c == '_' || c == '-' {
            true => word.push(c),
            false => {
                flush(&mut word, &mut out);
                out.push(c);
            }
        }
    }
    flush(&mut word, &mut out);
    out
}

/// The note for a part of a device the daemon is not connected to.
pub(super) fn not_connected(who: &str) -> String {
    format!("{who} is not connected: its part was left as it is")
}

/// The note for a device that is configured and not connected when a scene is saved.
pub(super) fn not_in_scene(who: &str) -> String {
    format!("{who} is not connected: it is not in the scene")
}

/// The note for a computer part the computer's controls cannot take now.
pub(super) fn computer_away(why: &str) -> String {
    custom_cased(why)
}

/// Why a save waits for `device`: it is still reading `what` (`the tone control`, or a control's
/// key, which `profile` gives a label).
pub(super) fn still_reading(device: &str, profile: &Profile, what: &str) -> String {
    match what == TONE_CONTROL {
        true => format!("{device} is still reading its bands"),
        false => {
            let label = profile
                .control_by_key(what)
                .map_or_else(|| what.to_string(), |control| control.label.to_lowercase());
            format!("{device} is still reading its {label}")
        }
    }
}

/// Why a save waits for `device`: its writer is busy with a recall or a setting.
pub(super) fn still_applying(device: &str) -> String {
    format!("{device} is still applying a change")
}

/// The question a recall asks when it would play `preset` on the devices `held` (their names)
/// and on the computer: `scene` is the scene's name.
pub(super) fn plays_twice(scene: &str, preset: &str, held: &[String]) -> String {
    format!(
        "{scene} leaves {preset} on the {} and on the computer: it plays twice. Recall it anyway?",
        held.join(" and the ")
    )
}

/// The refusal of a save with nothing to save.
pub(super) const NOTHING_TO_SAVE: &str =
    "nothing to save: no device is connected and the computer's controls are not there";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_whole_word_custom_is_cased() {
        assert_eq!(custom_cased("custom"), "Custom");
        assert_eq!(
            custom_cased("curve custom, not customised or custom_x"),
            "curve Custom, not customised or custom_x"
        );
        assert_eq!(custom_cased("Custom stays"), "Custom stays");
    }
}
