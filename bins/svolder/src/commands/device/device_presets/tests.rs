// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::time::Duration;

use svolder_profiles::{Registry, describe};
use svolder_proto::{Message, Value};

use super::*;

/// The built-in profile called `name`.
fn profile(name: &str) -> Profile {
    Registry::builtin()
        .expect("the built-in profiles load")
        .get(name)
        .expect("a built-in profile")
        .clone()
}

fn words(key: &str, slot: u8) -> Words {
    let profile = profile("lokimax");
    let action = profile.action_by_key(key).expect("an action");
    Words::of(action, slot, &profile.display)
}

/// A save is asked about with the slot and the device in the question, and a refusal says how
/// to answer without a terminal.
#[test]
fn a_save_asks_with_its_slot_and_its_device_and_refuses_without_a_terminal() {
    let save = words("save", 2);
    assert_eq!(
        save.question,
        "Save the bands to preset 2 on Loki Max? [y/N]"
    );
    assert_eq!(
        save.refusal,
        "saving to preset 2 overwrites what Loki Max holds there; add --yes to save without asking"
    );
    assert_eq!(save.done, "saved the bands to preset 2");
    assert_eq!(words("recall", 3).done, "recalled preset 3");
}

/// `--yes` answers without asking, a terminal is asked and its answer taken, and anything else
/// is refused without asking.
#[test]
fn the_answer_to_an_overwrite_comes_from_yes_a_terminal_or_nowhere() {
    let save = words("save", 1);
    let never = |_: &str| -> std::io::Result<bool> { panic!("nobody was to be asked") };
    assert!(overwrite_answer(&save, true, false, never).expect("yes answers"));
    let refused = overwrite_answer(&save, false, false, never).expect_err("no terminal");
    assert!(refused.contains("add --yes"), "{refused}");
    let asked = |question: &str| {
        assert_eq!(question, "Save the bands to preset 1 on Loki Max? [y/N]");
        Ok(false)
    };
    assert!(!overwrite_answer(&save, false, true, asked).expect("a terminal is asked"));
    assert!(overwrite_answer(&save, false, true, |_| Ok(true)).expect("a yes is taken"));
}

/// An action of a user's profile that is neither a recall nor a save is worded from its label.
#[test]
fn an_action_that_is_neither_recall_nor_save_is_worded_from_its_label() {
    let mut other = profile("lokimax")
        .action_by_key("save")
        .expect("an action")
        .clone();
    other.key = "erase".to_string();
    other.label = "Erase preset".to_string();
    let words = Words::of(&other, 3, "Loki Max");
    assert_eq!(words.question, "Erase preset 3 on Loki Max? [y/N]");
    assert_eq!(words.done, "sent erase 3");
    assert!(words.refusal.contains("add --yes"), "{}", words.refusal);
}

/// A save that was refused leaves the device alone: of two saves to one slot, the one that was
/// asked about and turned down never reaches it, so a recall brings back the first one's bands.
#[tokio::test]
async fn a_refused_save_leaves_the_slot_as_the_last_save_left_it() {
    let (mut backend, _gate) = Backend::fake_over(profile("lokimax"), Duration::from_secs(5)).await;
    let save = backend
        .profile()
        .action_by_key("save")
        .cloned()
        .expect("an action");
    let recall = backend
        .profile()
        .action_by_key("recall")
        .cloned()
        .expect("an action");
    backend
        .set("band1", "28", 1, false)
        .await
        .expect("a band is set");
    let never = |_: &str| -> std::io::Result<bool> { panic!("nobody was to be asked") };
    let sent = send_action_asking(&mut backend, &save, "1", false, (true, false, never)).await;
    assert!(matches!(sent, Ok(code) if code == ExitCode::SUCCESS));
    backend
        .set("band1", "10", 1, false)
        .await
        .expect("a band is set");

    // No terminal and no --yes: the verb stops where the question would be.
    let refused = send_action_asking(&mut backend, &save, "1", false, (false, false, never)).await;
    assert!(
        matches!(&refused, Err(words) if words.contains("--yes")),
        "{refused:?}"
    );

    let sent = send_action_asking(&mut backend, &recall, "1", false, (false, false, never)).await;
    assert!(matches!(sent, Ok(code) if code == ExitCode::SUCCESS));
    for _ in 0..40 {
        if band1(&mut backend).await == Some(28) {
            return;
        }
        let _ = backend.next_status().await;
    }
    panic!(
        "the recall did not bring back the first save: {:?}",
        band1(&mut backend).await
    );
}

/// The raw step the device last reported for band 1.
async fn band1(backend: &mut Backend) -> Option<i64> {
    let values = backend.state().await.ok()?;
    let id = backend.profile().control_by_key("band1")?.id.to_string();
    values.iter().find(|vo| vo.id == id)?.raw.as_i64()
}

/// The readings of the bands `bands` (1 to 6), each at the raw step `raw`.
fn readings(profile: &Profile, bands: &[u8], raw: u8) -> Vec<ValueObject> {
    bands
        .iter()
        .map(|band| {
            let id = format!("6.{band}.3").parse().expect("an id");
            let message = Message::status(id, Value::U8(raw));
            ValueObject::from_reading(&describe(profile, &message), &message)
        })
        .collect()
}

/// A built-in setting is `on` only against readings of every band it sets: with none, or with a
/// band unread, no row says yes or no, and with all of them the setting the bands read says yes
/// and the others no. A slot is never marked.
#[test]
fn a_setting_is_marked_on_only_when_every_band_it_sets_has_been_read() {
    let profile = profile("lokimax");
    let on = |values: Option<&[ValueObject]>| -> Vec<Option<bool>> {
        rows(&profile, values)
            .expect("the Loki Max has presets")
            .iter()
            .map(|row| row.on)
            .collect()
    };
    let slots = [None; 3];
    let unmarked: Vec<Option<bool>> = slots.iter().copied().chain([None; 5]).collect();
    assert_eq!(on(None), unmarked, "no readings");
    assert_eq!(on(Some(&[])), unmarked, "a daemon with no bands read");
    let five = readings(&profile, &[1, 2, 3, 4, 5], 15);
    assert_eq!(on(Some(&five)), unmarked, "one band unread");

    let flat = readings(&profile, &[1, 2, 3, 4, 5, 6], 15);
    let marked: Vec<Option<bool>> = slots
        .iter()
        .copied()
        .chain([
            Some(true),
            Some(false),
            Some(false),
            Some(false),
            Some(false),
        ])
        .collect();
    assert_eq!(on(Some(&flat)), marked, "every band read, flat");
}
