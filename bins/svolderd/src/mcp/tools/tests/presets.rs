// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `list_presets` and `load_preset`: a user's presets, a Loki Max's slots and built-in settings.

use super::*;

/// `load_preset` recalls through the device's writer, which plans it and holds other front ends'
/// tone writes until it is done, and reports the plan the writer made.
#[tokio::test]
async fn load_preset_recalls_through_the_devices_writer() {
    let fleet = scripted("gungnir2-v2", &[]);
    let answer = run(
        &fleet,
        None,
        call("load_preset", json!({"name": "warm"})),
        false,
    )
    .await;
    assert!(went(&answer), "{answer:?}");
    assert_eq!(*fleet.recalls.lock().unwrap(), ["warm"]);
    let CallToolResponse::Complete(result) = &answer else {
        panic!("{answer:?}");
    };
    let reply = result
        .structured_content
        .as_ref()
        .expect("a structured reply");
    assert_eq!(reply["preset"], json!("Warm"));
    assert!(reply["writes"].as_u64().is_some_and(|n| n > 0), "{reply}");
}

/// The tone control's readings once `preset` is on: every value its recall writes on a device
/// that had read nothing.
fn tone_of(profile: &Profile, preset: &Preset) -> Vec<ValueObject> {
    let eq = profile.eq.as_ref().expect("a tone control");
    let recall = preset.eq_writes(profile, |_| None).expect("a plan");
    recall
        .plan
        .writes
        .iter()
        .filter_map(|write| {
            let m = svolder_proto::Message::status(
                write.id(eq)?,
                svolder_proto::Value::U8(write.raw()),
            );
            Some(ValueObject::from_reading(
                &svolder_profiles::describe(profile, &m),
                &m,
            ))
        })
        .collect()
}

/// `list_presets` marks the one preset that is on: the one recalled while it matches, else the
/// first that matches.
#[tokio::test]
async fn list_presets_marks_the_preset_that_is_on() {
    let fleet = scripted("gungnir2-v2", &[]);
    let eq = fleet.profile.eq.clone().expect("a tone control");
    let warm = svolder_profiles::tone::preset::builtins(&eq)
        .into_iter()
        .find(|p| p.name == "Warm")
        .expect("Warm");
    let mut twin = warm.clone();
    twin.name = "Warm too".into();
    *fleet.tone.lock().unwrap() = tone_of(&fleet.profile, &warm);
    *fleet.presets.lock().unwrap() = vec![twin];
    let on_of = |answer: &CallToolResponse| -> Vec<String> {
        let CallToolResponse::Complete(result) = answer else {
            panic!("{answer:?}");
        };
        let reply = result
            .structured_content
            .as_ref()
            .expect("a structured reply");
        reply["presets"]
            .as_array()
            .expect("a list")
            .iter()
            .filter(|p| p["on"] == json!(true))
            .map(|p| p["name"].as_str().unwrap().to_string())
            .collect()
    };
    let first = run(&fleet, None, call("list_presets", json!({})), false).await;
    assert_eq!(on_of(&first), ["Warm"]);
    *fleet.recalled.lock().unwrap() = Some("Warm too".into());
    let recalled = run(&fleet, None, call("list_presets", json!({})), false).await;
    assert_eq!(on_of(&recalled), ["Warm too"]);
}

/// `list_presets` makes the fit of an imported preset off the runtime's thread: the runtime
/// keeps ticking while a slow fit runs, and the fit is kept when the list comes.
#[tokio::test(flavor = "current_thread")]
async fn list_presets_fits_an_imported_preset_off_the_runtime_thread() {
    use crate::recall::fits::tests::{
        consumer_reads_the_fit_warm_made, eq, imported, is_warm, ticks_during,
    };
    let fleet = scripted("gungnir2-v2", &[]);
    let slow = imported("slow-fit-mcp-list", 2.5);
    assert!(!is_warm(&eq(), &slow));
    *fleet.presets.lock().unwrap() = vec![slow.clone()];
    let ticks = ticks_during(run(&fleet, None, call("list_presets", json!({})), false)).await;
    assert!(ticks >= 10, "the interval ticked {ticks} times in 300 ms");
    assert!(is_warm(&eq(), &slow));
    assert!(consumer_reads_the_fit_warm_made(&eq(), &slow));
}

/// The six bands of a Loki Max as raw steps, by key.
pub(super) fn bands(raw: [i64; 6]) -> Vec<(&'static str, i64)> {
    ["band1", "band2", "band3", "band4", "band5", "band6"]
        .into_iter()
        .zip(raw)
        .collect()
}

const FLAT: [i64; 6] = [15; 6];

fn load(name: &str, confirm: bool) -> CallToolRequestParams {
    call("load_preset", json!({"name": name, "confirm": confirm}))
}

/// `list_presets` on a Loki Max lists its three slots, never marked, then its five built-in
/// settings, the one the bands read marked on.
#[tokio::test]
async fn list_presets_on_a_loki_max_lists_three_slots_and_five_builtins() {
    let fleet = scripted("lokimax", &bands(FLAT));
    let answer = run(&fleet, None, call("list_presets", json!({})), false).await;
    assert!(went(&answer), "{answer:?}");
    let reply = structured(&answer);
    let rows = reply["presets"].as_array().expect("a list");
    assert_eq!(rows.len(), 8, "{reply}");
    assert_eq!(
        rows[0],
        json!({"name": "Preset 1", "slot": 1, "built_in": false, "on": false})
    );
    assert_eq!(rows[2]["name"], json!("Preset 3"));
    assert!(rows[..3].iter().all(|row| row["on"] == json!(false)));
    let names: Vec<&str> = rows[3..]
        .iter()
        .map(|row| row["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        ["Flat", "Bass Boost", "Treble Boost", "Smiley", "Loudness"]
    );
    assert!(rows[3..].iter().all(|row| row["built_in"] == json!(true)));
    let on: Vec<&str> = rows
        .iter()
        .filter(|row| row["on"] == json!(true))
        .map(|row| row["name"].as_str().unwrap())
        .collect();
    assert_eq!(on, ["Flat"]);
    // With a band unread, none is on.
    let fleet = scripted("lokimax", &bands(FLAT)[..5]);
    let reply = structured(&run(&fleet, None, call("list_presets", json!({})), false).await);
    assert!(
        reply["presets"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["on"] == json!(false)),
        "{reply}"
    );
}

#[tokio::test]
async fn a_builtin_setting_within_6_db_of_the_floor_goes_through_unasked() {
    // Band 1 reads 6.4 dB; Bass Boost takes it to 10.4 dB, 4 dB up, and no other band rises by
    // more than 6 dB.
    let mut raw = FLAT;
    raw[0] = 23;
    let fleet = scripted("lokimax", &bands(raw));
    let answer = run(&fleet, None, load("bass boost", false), false).await;
    assert!(went(&answer), "{answer:?}");
    assert_eq!(*fleet.applied.lock().unwrap(), ["Bass Boost"]);
    let reply = structured(&answer);
    assert_eq!(reply["preset"], json!("Bass Boost"));
    assert_eq!(reply["writes"], json!(3));
}

#[tokio::test]
async fn a_builtin_setting_that_raises_a_band_more_than_6_db_asks_first() {
    let fleet = scripted("lokimax", &bands(FLAT));
    // A client that cannot ask is told the question's words and to repeat with confirm.
    let answer = run(&fleet, None, load("Bass Boost", false), false).await;
    assert!(is_error(&answer), "{answer:?}");
    let words = error_words(&answer);
    assert!(
        words.contains(
            "raising 20 Hz on amp from 0.0 dB to 10.4 dB is more than 6 dB above its lowest \
             level in the last minute (0.0 dB)"
        ),
        "{words}"
    );
    assert!(
        !words.contains("120 Hz"),
        "only the band that asks is named: {words}"
    );
    assert!(words.contains("repeat this call with"), "{words}");
    assert!(fleet.applied.lock().unwrap().is_empty());
    // A client that can ask is sent the question, whatever the model confirms.
    let asked = run(&fleet, None, load("Bass Boost", true), true).await;
    let CallToolResponse::InputRequired(asked) = asked else {
        panic!("{asked:?}")
    };
    assert_eq!(
        asked.request_state.as_deref(),
        Some("setting|amp|Bass Boost|band1=10.4@0.0")
    );
    assert!(fleet.applied.lock().unwrap().is_empty());
    // The yes of a client that cannot ask goes through, and the floor is reset for the band.
    let tools = server(&fleet, None);
    let answer = tools
        .call(load("bass boost", true), Caller { can_ask: false })
        .await
        .expect("an answer");
    assert!(went(&answer), "{answer:?}");
    assert_eq!(*fleet.applied.lock().unwrap(), ["Bass Boost"]);
    assert_eq!(
        tools.floors.floor("amp", "band1"),
        Some(db_of(&fleet, "band1", 28))
    );
}

/// A band's level in dB at raw step `raw`.
pub(super) fn db_of(fleet: &Arc<Scripted>, key: &str, raw: i64) -> f64 {
    convert::raw_to_display(fleet.profile.control_by_key(key).unwrap(), raw).unwrap()
}

/// A band the device has not reported counts as the lowest there is, as it does for a band set
/// by hand, so a built-in setting over unread bands asks, wording each as a level.
#[tokio::test]
async fn a_builtin_setting_over_unread_bands_asks_as_a_band_set_by_hand_does() {
    let fleet = scripted("lokimax", &[]);
    let answer = run(&fleet, None, load("Flat", false), false).await;
    assert!(is_error(&answer), "{answer:?}");
    let words = error_words(&answer);
    assert!(
        words.contains("from a level the device has not reported yet to 0.0 dB"),
        "{words}"
    );
    assert!(fleet.applied.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_declined_builtin_setting_writes_nothing() {
    let fleet = scripted("lokimax", &bands(FLAT));
    let asked = run(&fleet, None, load("Bass Boost", false), true).await;
    let CallToolResponse::InputRequired(asked) = asked else {
        panic!("{asked:?}")
    };
    let state = asked.request_state.clone().unwrap();
    let retry = |action: Value| {
        let mut request = load("Bass Boost", false);
        request.request_state = Some(state.clone());
        request.input_responses = Some([(ASK.to_string(), action)].into_iter().collect());
        request
    };
    for no in [json!({"action": "decline"}), json!({"action": "cancel"})] {
        let answer = run(&fleet, None, retry(no), true).await;
        assert!(is_error(&answer) && error_words(&answer).contains("declined"));
    }
    assert!(fleet.applied.lock().unwrap().is_empty());
    assert!(fleet.submitted.lock().unwrap().is_empty());
    // The yes of the question asked goes through.
    let yes = json!({"action": "accept", "content": {"confirm": true}});
    let answer = run(&fleet, None, retry(yes), true).await;
    assert!(went(&answer), "{answer:?}");
    assert_eq!(*fleet.applied.lock().unwrap(), ["Bass Boost"]);
}

#[tokio::test]
async fn a_slot_recall_asks_first_naming_the_12_db_it_can_raise() {
    let fleet = scripted("lokimax", &bands(FLAT));
    let answer = run(&fleet, None, load("Preset 2", false), false).await;
    assert!(is_error(&answer), "{answer:?}");
    let words = error_words(&answer);
    assert!(
        words.contains(
            "recalling Preset 2 on amp sets its bands to what the device saved there, which can \
             raise a band to as much as 12.0 dB"
        ),
        "{words}"
    );
    let asked = run(&fleet, None, load("preset 2", false), true).await;
    let CallToolResponse::InputRequired(asked) = asked else {
        panic!("{asked:?}")
    };
    assert_eq!(asked.request_state.as_deref(), Some("recall|amp|2"));
    assert!(fleet.submitted.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_slot_recall_without_confirm_from_a_client_that_cannot_ask_is_refused() {
    let fleet = scripted("lokimax", &bands(FLAT));
    for name in ["Preset 3", "3"] {
        let answer = run(
            &fleet,
            None,
            call("load_preset", json!({"name": name})),
            false,
        )
        .await;
        assert!(is_error(&answer), "{answer:?}");
        assert!(error_words(&answer).contains("confirm"));
    }
    let answer = run(&fleet, None, load("Preset 3", false), false).await;
    assert!(is_error(&answer), "{answer:?}");
    assert!(fleet.submitted.lock().unwrap().is_empty());
    assert!(fleet.applied.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_confirmed_slot_recall_sends_the_recall() {
    let fleet = scripted("lokimax", &bands(FLAT));
    let answer = run(&fleet, None, load("Preset 2", true), false).await;
    assert!(went(&answer), "{answer:?}");
    assert_eq!(*fleet.submitted.lock().unwrap(), ["set recall 2"]);
    let reply = structured(&answer);
    assert_eq!(reply["preset"], json!("Preset 2"));
    assert_eq!(reply["slot"], json!(2));
    // A slot outside 1..=3 is no slot.
    let answer = run(&fleet, None, load("Preset 4", true), false).await;
    assert!(is_error(&answer), "{answer:?}");
    assert!(error_words(&answer).contains("Preset 1, Preset 2, Preset 3, Flat"));
    assert_eq!(fleet.submitted.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn load_preset_refuses_a_save() {
    let fleet = scripted("lokimax", &bands(FLAT));
    for name in ["save", "Save", "SAVE 2", "save preset 2", "Save Preset 1"] {
        for can_ask in [false, true] {
            let answer = run(&fleet, None, load(name, true), can_ask).await;
            assert!(is_error(&answer), "{name}: {answer:?}");
            assert!(
                error_words(&answer).contains(
                    "saving a preset is not offered here, because it overwrites what the device \
                     holds; amp can be saved to from its own front ends"
                ),
                "{name}: {answer:?}"
            );
        }
    }
    assert!(fleet.submitted.lock().unwrap().is_empty());
    assert!(fleet.applied.lock().unwrap().is_empty());
    assert!(fleet.sets.lock().unwrap().is_empty());
}

/// A name that only begins with the save action's key is no save: a built-in setting called
/// `Saved Mix` loads, and an unknown name is told what it can be, not that saving is not offered.
#[tokio::test]
async fn a_name_that_merely_starts_with_save_is_not_refused_as_a_save() {
    let mut profile = Registry::builtin()
        .expect("the built-in profiles")
        .get("lokimax")
        .expect("a built-in")
        .clone();
    profile
        .builtin_settings
        .as_mut()
        .expect("settings")
        .settings[0]
        .name = "Saved Mix".into();
    let fleet = scripted_over(profile, &bands(FLAT));
    let answer = run(&fleet, None, load("saved mix", false), false).await;
    assert!(went(&answer), "{answer:?}");
    assert_eq!(structured(&answer)["preset"], json!("Saved Mix"));
    for name in ["savings", "save the day", "saves 2"] {
        let answer = run(&fleet, None, load(name, true), false).await;
        assert!(is_error(&answer), "{name}: {answer:?}");
        assert!(
            !error_words(&answer).contains("saving a preset is not offered"),
            "{name}: {answer:?}"
        );
    }
}

/// A yes recorded under one floor does not answer the question asked under another: the state of
/// a built-in setting's question names each band that asks with its target and floor.
#[tokio::test]
async fn a_yes_under_one_floor_does_not_answer_the_question_under_another() {
    let fleet = scripted("lokimax", &bands(FLAT));
    let tools = server(&fleet, None);
    let asked = tools
        .call(load("Bass Boost", false), Caller { can_ask: true })
        .await
        .expect("an answer");
    let CallToolResponse::InputRequired(asked) = asked else {
        panic!("{asked:?}")
    };
    let state = asked.request_state.clone().expect("a state");
    let yes = json!({"action": "accept", "content": {"confirm": true}});
    let retry = |state: &str| {
        let mut request = load("Bass Boost", false);
        request.request_state = Some(state.to_string());
        request.input_responses = Some([(ASK.to_string(), yes.clone())].into_iter().collect());
        request
    };
    // Another front end lowers band 1 meanwhile, and the floor with it.
    tools.floors.record("amp", "band1", -12.0);
    let again = tools
        .call(retry(&state), Caller { can_ask: true })
        .await
        .expect("an answer");
    let CallToolResponse::InputRequired(again) = again else {
        panic!("a yes under the old floor went through: {again:?}")
    };
    assert_ne!(again.request_state.as_deref(), Some(state.as_str()));
    assert!(fleet.applied.lock().unwrap().is_empty());
    // The yes of the question asked under this floor goes through.
    let state = again.request_state.expect("a state");
    let answer = tools
        .call(retry(&state), Caller { can_ask: true })
        .await
        .expect("an answer");
    assert!(went(&answer), "{answer:?}");
    assert_eq!(*fleet.applied.lock().unwrap(), ["Bass Boost"]);
}

/// The state of a built-in setting's question names the floor each band was judged against, not
/// the floor of the moment it is built: a lower reading another front end lets the floors note
/// while a later band is read does not change it.
#[tokio::test]
async fn a_floor_noted_between_judging_and_the_state_does_not_change_the_state() {
    let flat = scripted("lokimax", &bands(FLAT));
    let tools = server(&flat, None);
    tools.floors.record("amp", "band1", -3.0);
    let state_of = |tools: &Tools| {
        let fleet = Arc::clone(&flat);
        let tools = tools.clone();
        async move {
            fleet.applied.lock().unwrap().clear();
            let asked = tools
                .call(load("Bass Boost", false), Caller { can_ask: true })
                .await
                .expect("an answer");
            let CallToolResponse::InputRequired(asked) = asked else {
                panic!("{asked:?}")
            };
            asked.request_state.expect("a state")
        }
    };
    let quiet = state_of(&tools).await;
    // The same call, with band 1's floor dropping as band 2 is read for its judgement, after
    // the plan's first read of every band and band 1's judgement.
    let floors = Arc::clone(&tools.floors);
    let band2_reads = std::sync::atomic::AtomicUsize::new(0);
    *flat.on_read.lock().unwrap() = Some(Box::new(move |key| {
        if key == "band2" && band2_reads.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 1 {
            floors.record("amp", "band1", -30.0);
        }
    }));
    let moving = state_of(&tools).await;
    // The hook fired during the call, or the test proves nothing: band 1's floor is the lowered one.
    assert_eq!(
        tools.floors.floor("amp", "band1"),
        Some(-30.0),
        "the floor never moved during the call"
    );
    assert_eq!(moving, quiet);
}

#[tokio::test]
async fn set_control_refuses_an_action_key() {
    let fleet = scripted("lokimax", &bands(FLAT));
    for key in ["recall", "save", "Recall"] {
        let answer = run(&fleet, None, set(key, json!(2), true), false).await;
        assert!(is_error(&answer), "{key}: {answer:?}");
        assert!(error_words(&answer).contains("has no control"), "{key}");
    }
    assert!(fleet.sets.lock().unwrap().is_empty());
    assert!(fleet.submitted.lock().unwrap().is_empty());
}

/// A failed read is the answer, not "no built-in is on".
#[tokio::test]
async fn list_presets_on_a_loki_max_answers_a_failed_read_with_its_error() {
    let fleet = scripted("lokimax", &bands(FLAT));
    *fleet.failing_reads.lock().unwrap() = Some("the device is not connected".to_string());
    let answer = run(&fleet, None, call("list_presets", json!({})), false).await;
    assert!(is_error(&answer), "{answer:?}");
    assert!(error_words(&answer).contains("the device is not connected"));
}

/// A setting the bands already read has nothing to write, and says so without a batch.
#[tokio::test]
async fn a_builtin_setting_the_device_is_already_on_says_so_and_sends_nothing() {
    let fleet = scripted("lokimax", &bands(FLAT));
    let answer = run(&fleet, None, load("flat", false), false).await;
    assert!(went(&answer), "{answer:?}");
    let reply = structured(&answer);
    assert_eq!(reply["writes"], json!(0));
    assert_eq!(reply["message"], json!("Flat is already on"));
    assert!(fleet.applied.lock().unwrap().is_empty());
}

/// The writer plans again from its own readings, so a band that reads its target now but was
/// more than 6 dB lower within the last minute is asked about too: another front end may lower
/// it before the writer sends the setting.
#[tokio::test]
async fn a_band_on_target_now_but_far_above_its_floor_still_asks() {
    // Bass Boost is 28 21 17 15 15 15. Band 1 already reads 28; band 2 still has to rise.
    let raw = [28, 15, 15, 15, 15, 15];
    let fleet = scripted("lokimax", &bands(raw));
    let tools = server(&fleet, None);
    tools.floors.record("amp", "band1", 0.0);
    let answer = tools
        .call(load("Bass Boost", false), Caller { can_ask: false })
        .await
        .expect("an answer");
    assert!(is_error(&answer), "{answer:?}");
    let words = error_words(&answer);
    assert!(
        words.contains(
            "writing 20 Hz on amp at 10.4 dB is more than 6 dB above its lowest level in the \
             last minute (0.0 dB)"
        ),
        "{words}"
    );
    assert!(!words.contains("120 Hz"), "{words}");
    assert!(fleet.applied.lock().unwrap().is_empty());
    // With no lower level in the last minute the same setting goes through unasked.
    let fleet = scripted("lokimax", &bands(raw));
    let answer = run(&fleet, None, load("Bass Boost", false), false).await;
    assert!(went(&answer), "{answer:?}");
    assert_eq!(*fleet.applied.lock().unwrap(), ["Bass Boost"]);
}
