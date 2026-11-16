// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `set_control`, `mute` and the guards on a write: the largest rise, the cap, floors, gain
//! selects, unmutes, powering on and balances.

use super::*;

#[tokio::test]
async fn the_largest_rise_that_is_six_db_or_less_goes_through_and_the_next_asks() {
    let fleet = scripted("gungnir2-v2", &[("volume", 60)]);
    let profile = fleet.profile.clone();
    let within = step_above(&profile, 60, MAX_RISE_DB, false);
    let beyond = step_above(&profile, 60, MAX_RISE_DB, true);

    let answer = run(
        &fleet,
        None,
        set("volume", json!(db(&profile, within)), false),
        true,
    )
    .await;
    assert!(went(&answer), "{answer:?}");
    assert_eq!(fleet.sets.lock().unwrap().len(), 1);

    let answer = run(
        &fleet,
        None,
        set("volume", json!(db(&profile, beyond)), false),
        true,
    )
    .await;
    assert!(asks(&answer));
    assert_eq!(fleet.sets.lock().unwrap().len(), 1, "asking sets nothing");
}

#[tokio::test]
async fn a_level_that_cannot_be_read_or_is_silent_counts_as_the_largest_rise() {
    for current in [None, Some(0)] {
        let readings: Vec<(&str, i64)> = current.map(|c| ("volume", c)).into_iter().collect();
        let fleet = scripted("gungnir2-v2", &readings);
        let answer = run(&fleet, None, set("volume", json!(-60), false), false).await;
        assert!(is_error(&answer), "{current:?}: {answer:?}");
        assert!(fleet.sets.lock().unwrap().is_empty());
    }
}

#[tokio::test]
async fn setting_the_same_level_or_lower_never_asks() {
    let fleet = scripted("gungnir2-v2", &[("volume", 100)]);
    let profile = fleet.profile.clone();
    for raw in [100, 99, 50, 1] {
        let answer = run(
            &fleet,
            None,
            set("volume", json!(db(&profile, raw)), false),
            false,
        )
        .await;
        assert!(went(&answer), "{raw}: {answer:?}");
    }
    // Silence has no number; a mute is how it is asked for.
    let answer = run(&fleet, None, set("volume", json!(-200), false), false).await;
    assert!(is_error(&answer) && error_words(&answer).contains("between"));
}

#[tokio::test]
async fn small_steps_add_up_to_one_rise_within_the_minute() {
    let fleet = scripted("gungnir2-v2", &[("volume", 20)]);
    let profile = fleet.profile.clone();
    let tools = server(&fleet, None);
    let mut raw = 20;
    let mut asked_at = None;
    for _ in 0..30 {
        let next = step_above(&profile, raw, 3.0, false);
        fleet.readings.lock().unwrap().insert("volume".into(), raw);
        let answer = tools
            .call(
                set("volume", json!(db(&profile, next)), false),
                Caller { can_ask: true },
            )
            .await
            .unwrap();
        if asks(&answer) {
            asked_at = Some(db(&profile, next) - db(&profile, 20));
            break;
        }
        assert!(went(&answer), "{answer:?}");
        raw = next;
    }
    let rise = asked_at.expect("a staircase asks");
    assert!(
        rise > MAX_RISE_DB && rise < MAX_RISE_DB + 3.5,
        "asked at {rise} dB"
    );
}

#[tokio::test(start_paused = true)]
async fn the_floor_is_the_lowest_level_of_the_last_minute() {
    let floors = Floors::default();
    floors.record("amp", "volume", -50.0);
    floors.record("amp", "volume", -40.0);
    assert_eq!(floors.floor("AMP", "Volume"), Some(-50.0));
    tokio::time::advance(Duration::from_secs(59)).await;
    assert_eq!(floors.floor("amp", "volume"), Some(-50.0));
    tokio::time::advance(Duration::from_secs(2)).await;
    assert_eq!(floors.floor("amp", "volume"), None);
    floors.record("amp", "volume", -30.0);
    floors.reset("amp", "volume", -10.0);
    assert_eq!(floors.floor("amp", "volume"), Some(-10.0));
    assert_eq!(floors.floor("amp", "balance"), None);
}

#[tokio::test]
async fn a_lowering_is_allowed_above_the_cap_and_a_rise_to_it_is_not() {
    let fleet = scripted("gungnir2-v2", &[("volume", 127)]);
    let profile = fleet.profile.clone();
    // 0 dB with a cap of -20: a smaller drop that still ends above the cap goes through.
    let answer = run(&fleet, Some(-20.0), set("volume", json!(-3), false), false).await;
    assert!(went(&answer), "{answer:?}");
    // A rise that ends above it does not, confirmed or not.
    fleet.readings.lock().unwrap().insert("volume".into(), 10);
    for confirm in [false, true] {
        let answer = run(
            &fleet,
            Some(-20.0),
            set("volume", json!(-10), confirm),
            false,
        )
        .await;
        assert!(
            is_error(&answer)
                && error_words(&answer).contains(
                    "raising the volume to -10.0 dB is above this server's limit of -20.0 dB \
                     (max_volume_db); choose a level at or below it"
                ),
            "{answer:?}"
        );
    }
    let _ = profile;
}

#[tokio::test]
async fn a_step_that_rounds_above_the_cap_is_refused() {
    let fleet = scripted("gungnir2-v2", &[("volume", 10)]);
    let profile = fleet.profile.clone();
    let cap = (50..127)
        .map(|r| db(&profile, r))
        .find(|d| {
            let volume = profile.control_by_key("volume").unwrap();
            let raw = convert::display_to_raw(volume, *d - 0.01).unwrap();
            db(&profile, raw) > *d - 0.01
        })
        .expect("a cap between steps");
    let at = cap - 0.01;
    let answer = run(&fleet, Some(at), set("volume", json!(at), true), false).await;
    assert!(is_error(&answer), "{answer:?}");
    assert!(fleet.sets.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_client_that_can_ask_is_asked_whatever_the_model_confirms() {
    let fleet = scripted("gungnir2-v2", &[("volume", 20)]);
    let answer = run(&fleet, None, set("volume", json!(-5), true), true).await;
    assert!(asks(&answer), "{answer:?}");
    assert!(fleet.sets.lock().unwrap().is_empty());
    let answer = run(&fleet, None, set("volume", json!(-5), true), false).await;
    assert!(went(&answer), "{answer:?}");
}

#[tokio::test]
async fn a_declined_answer_is_final_and_an_accepted_one_must_match_the_question() {
    let fleet = scripted("gungnir2-v2", &[("volume", 20)]);
    let asked = run(&fleet, None, set("volume", json!(-5), false), true).await;
    let CallToolResponse::InputRequired(asked) = asked else {
        panic!("{asked:?}")
    };
    let state = asked.request_state.clone().unwrap();
    let retry = |action: Value, state: &str| {
        let mut request = set("volume", json!(-5), false);
        request.request_state = Some(state.to_string());
        request.input_responses = Some([(ASK.to_string(), action)].into_iter().collect());
        request
    };
    let declined = run(
        &fleet,
        None,
        retry(json!({"action": "decline"}), &state),
        true,
    )
    .await;
    assert!(is_error(&declined) && error_words(&declined).contains("declined"));
    let cancelled = run(
        &fleet,
        None,
        retry(json!({"action": "cancel"}), &state),
        true,
    )
    .await;
    assert!(is_error(&cancelled));
    let yes = json!({"action": "accept", "content": {"confirm": true}});
    let stale = run(
        &fleet,
        None,
        retry(yes.clone(), "level|amp|volume|1|none"),
        true,
    )
    .await;
    assert!(asks(&stale), "a state that is not the question's");
    assert!(fleet.sets.lock().unwrap().is_empty());
    let accepted = run(&fleet, None, retry(yes, &state), true).await;
    assert!(went(&accepted), "{accepted:?}");
}

#[tokio::test]
async fn a_band_in_db_follows_the_same_rule_with_its_own_window() {
    let fleet = scripted("lokimax", &[("band1", 3), ("band2", 15)]);
    // band1: -12 + 3*0.8 = -9.6 dB; +24 dB steps up is far more than 6.
    let answer = run(&fleet, None, set("band1", json!(12), false), true).await;
    assert!(asks(&answer), "{answer:?}");
    let answer = run(&fleet, None, set("band1", json!(-5), false), true).await;
    assert!(went(&answer), "{answer:?}");
    // Out of the band's own range is refused, not clamped.
    let answer = run(&fleet, None, set("band2", json!(12), false), false).await;
    assert!(
        is_error(&answer) && error_words(&answer).contains("between -9"),
        "{answer:?}"
    );
    // The cap is the volume's alone.
    let answer = run(&fleet, Some(-40.0), set("band2", json!(5), true), false).await;
    assert!(went(&answer), "{answer:?}");
}

#[tokio::test]
async fn a_gain_select_asks_when_it_goes_up_and_not_when_it_goes_down() {
    let fleet = scripted("hati", &[("gain", 2)]);
    let up = run(&fleet, None, set("gain", json!("70 dB"), false), true).await;
    assert!(asks(&up), "{up:?}");
    let up_old = run(&fleet, None, set("gain", json!("70 dB"), false), false).await;
    assert!(is_error(&up_old) && error_words(&up_old).contains("confirm"));
    let down = run(&fleet, None, set("gain", json!("40 dB"), false), true).await;
    assert!(went(&down), "{down:?}");
    let same = run(&fleet, None, set("gain", json!("50 dB"), false), true).await;
    assert!(went(&same), "{same:?}");
    // An unreadable setting may be a rise.
    let fleet = scripted("hati", &[]);
    let unread = run(&fleet, None, set("gain", json!("40 dB"), false), true).await;
    assert!(asks(&unread));
}

#[tokio::test]
async fn an_unmute_asks_and_is_refused_above_the_cap() {
    let fleet = scripted("gungnir2-v2", &[("volume", 100), ("mute", 1)]);
    let profile = fleet.profile.clone();
    let level = db(&profile, 100);
    let answer = run(&fleet, None, call("mute", json!({"on": false})), true).await;
    assert!(asks(&answer), "{answer:?}");
    let old = run(&fleet, None, call("mute", json!({"on": false})), false).await;
    assert!(is_error(&old));
    let old = run(
        &fleet,
        None,
        call("mute", json!({"on": false, "confirm": true})),
        false,
    )
    .await;
    assert!(went(&old), "{old:?}");
    // Through set_control too.
    let answer = run(&fleet, None, set("mute", json!(false), false), true).await;
    assert!(asks(&answer));
    // Muting never asks.
    let answer = run(&fleet, None, call("mute", json!({"on": true})), true).await;
    assert!(went(&answer), "{answer:?}");
    // Above the cap: refused whatever is confirmed.
    let capped = Some(level - 5.0);
    for can_ask in [true, false] {
        let answer = run(
            &fleet,
            capped,
            call("mute", json!({"on": false, "confirm": true})),
            can_ask,
        )
        .await;
        assert!(
            is_error(&answer)
                && error_words(&answer).contains(&format!(
                    "unmuting amp would play at {:.1} dB, above this server's limit of {:.1} dB \
                     (max_volume_db); lower the volume first",
                    level,
                    level - 5.0
                )),
            "{answer:?}"
        );
    }
    // A volume that cannot be read asks.
    let fleet = scripted("gungnir2-v2", &[]);
    let answer = run(&fleet, None, call("mute", json!({"on": false})), true).await;
    assert!(asks(&answer));
}

#[tokio::test]
async fn mute_finds_the_mute_of_a_lyr_5() {
    let fleet = scripted("lyr5", &[("volume", 10)]);
    let answer = run(&fleet, None, call("mute", json!({"on": true})), true).await;
    assert!(went(&answer), "{answer:?}");
    assert_eq!(fleet.sets.lock().unwrap()[0].0, "headphone_mute");
}

#[tokio::test]
async fn a_level_with_a_unit_in_text_is_read_in_that_unit_and_refused_outside_its_range() {
    let volume = Registry::builtin()
        .unwrap()
        .get("gungnir2-v2")
        .unwrap()
        .control_by_key("volume")
        .unwrap()
        .clone();
    assert_eq!(
        units::number_in("-20 dB", volume.unit.as_deref()),
        Some(-20.0)
    );
    assert_eq!(
        units::number_in(" -20DB", volume.unit.as_deref()),
        Some(-20.0)
    );
    assert_eq!(units::number_in("-20", volume.unit.as_deref()), Some(-20.0));
    assert_eq!(units::number_in("loud", volume.unit.as_deref()), None);
    assert!(level_raw(&volume, &json!("max")).is_err());
    assert!(level_raw(&volume, &json!(f64::NAN)).is_err());
    assert!(level_raw(&volume, &json!(true)).is_err());
    assert_eq!(level_raw(&volume, &json!(0)).unwrap(), 127);
    for out in [json!(3), json!(1e308), json!(-1e308), json!(-97)] {
        let refused = level_raw(&volume, &out).unwrap_err();
        assert!(refused.contains("between -96.33 and 0"), "{out}: {refused}");
    }
}

#[test]
fn the_write_tools_are_listed_only_with_writes() {
    let fleet: Arc<dyn Fleet> = Arc::new(Unavailable);
    let names = |writes| -> Vec<String> {
        Tools::new(Arc::clone(&fleet), writes, None)
            .list()
            .iter()
            .map(|t| t.name.to_string())
            .collect()
    };
    assert_eq!(names(false).len(), 5);
    assert_eq!(names(true).len(), 13);
    for tool in Tools::new(fleet, true, None).list() {
        let description = tool.description.expect("a description");
        assert!(description.ends_with('.'), "{description}");
    }
}

#[tokio::test]
async fn an_unmute_on_a_device_with_no_volume_asks_what_it_lets_through() {
    for profile in ["gungnir2-v1", "hati", "gjallarhorn"] {
        let fleet = scripted(profile, &[]);
        assert!(
            fleet
                .profile
                .control_by_key("mute")
                .is_some_and(|c| c.mutes_output),
            "{profile}"
        );
        assert!(
            fleet
                .profile
                .control_by_key("volume")
                .is_none_or(|c| !svolder_profiles::safety::in_db(c)),
            "{profile}"
        );
        let asked = run(&fleet, None, call("mute", json!({"on": false})), true).await;
        let CallToolResponse::InputRequired(asked) = asked else {
            panic!("{profile}: {asked:?}")
        };
        let text = format!("{:?}", asked.input_requests);
        assert!(text.contains("full output"), "{text}");
        let old = run(&fleet, None, call("mute", json!({"on": false})), false).await;
        assert!(
            is_error(&old) && error_words(&old).contains("confirm"),
            "{profile}"
        );
        let old = run(
            &fleet,
            None,
            call("mute", json!({"on": false, "confirm": true})),
            false,
        )
        .await;
        assert!(went(&old), "{profile}: {old:?}");
        // Muting never asks.
        let muted = run(&fleet, None, call("mute", json!({"on": true})), true).await;
        assert!(went(&muted), "{profile}");
    }
}

#[tokio::test]
async fn a_control_that_asks_on_change_asks_whatever_the_value() {
    let fleet = scripted("lyr5", &[("preout", 0)]);
    for value in [json!(true), json!(false)] {
        let answer = run(&fleet, None, set("preout", value, false), true).await;
        assert!(asks(&answer), "{answer:?}");
    }
    let old = run(&fleet, None, set("preout", json!(true), true), false).await;
    assert!(went(&old), "{old:?}");
    let fleet = scripted("lokimax", &[("mode", 1)]);
    let answer = run(&fleet, None, set("mode", json!("Bypass"), false), true).await;
    assert!(asks(&answer), "{answer:?}");
}

#[tokio::test]
async fn a_devices_own_reading_lowers_the_floor() {
    let fleet = scripted("gungnir2-v2", &[("volume", 60)]);
    let (tx, rx) = tokio::sync::mpsc::channel(4);
    *fleet.events.lock().unwrap() = Some(rx);
    let tools = server(&fleet, None);
    let stop = CancellationToken::new();
    let watching = tools.watch(stop.clone());
    let mut low = reading("volume", 10);
    low.unit = Some("dB".to_string());
    low.value = Some(-70.0);
    tx.send(Event::Status {
        device: "amp".to_string(),
        reading: low,
    })
    .await
    .unwrap();
    for _ in 0..200 {
        if tools.floors.floor("amp", "volume") == Some(-70.0) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert_eq!(tools.floors.floor("amp", "volume"), Some(-70.0));
    // The device is now at raw 60, far above that floor: a small step asks.
    let profile = fleet.profile.clone();
    let next = step_above(&profile, 60, 2.0, false);
    let answer = tools
        .call(
            set("volume", json!(db(&profile, next)), false),
            Caller { can_ask: true },
        )
        .await
        .unwrap();
    assert!(asks(&answer), "{answer:?}");
    stop.cancel();
    watching.await.unwrap();
}

fn power(on: bool) -> CallToolRequestParams {
    set("power", json!(on), false)
}

#[tokio::test]
async fn powering_on_a_device_with_no_volume_asks_what_it_lets_through() {
    // A standby amplifier (two of the five), Tyr, and the phono stages.
    for profile in ["vidar2", "kraken", "tyr", "hati", "stjarna"] {
        let fleet = scripted(profile, &[]);
        assert!(
            fleet
                .profile
                .control_by_key("power")
                .is_some_and(|c| c.powers_output),
            "{profile}"
        );
        let asked = run(&fleet, None, power(true), true).await;
        let CallToolResponse::InputRequired(asked) = asked else {
            panic!("{profile}: {asked:?}")
        };
        let text = format!("{:?}", asked.input_requests);
        assert!(
            text.contains("powering on") && text.contains("full output"),
            "{text}"
        );
        let old = run(&fleet, None, power(true), false).await;
        assert!(
            is_error(&old) && error_words(&old).contains("confirm"),
            "{profile}"
        );
        assert!(fleet.sets.lock().unwrap().is_empty(), "{profile}");
        let old = run(&fleet, None, set("power", json!(true), true), false).await;
        assert!(went(&old), "{profile}: {old:?}");
        // Powering off never asks, for either kind of client.
        for can_ask in [true, false] {
            let off = run(&fleet, None, power(false), can_ask).await;
            assert!(went(&off), "{profile}: {off:?}");
        }
    }
}

#[tokio::test]
async fn powering_on_a_freya_2_asks_at_its_current_volume_and_is_refused_above_the_cap() {
    let fleet = scripted("freya2", &[("volume", 100)]);
    let level = convert::raw_to_display(fleet.profile.control_by_key("volume").unwrap(), 100)
        .expect("a level");
    let asked = run(&fleet, None, power(true), true).await;
    let CallToolResponse::InputRequired(asked) = asked else {
        panic!("{asked:?}")
    };
    let text = format!("{:?}", asked.input_requests);
    assert!(text.contains("its current volume of"), "{text}");
    let old = run(&fleet, None, power(true), false).await;
    assert!(is_error(&old) && error_words(&old).contains("confirm"));
    let old = run(&fleet, None, set("power", json!(true), true), false).await;
    assert!(went(&old), "{old:?}");
    // Above the cap: refused whatever is confirmed, and powering off still goes through.
    for can_ask in [true, false] {
        let answer = run(
            &fleet,
            Some(level - 5.0),
            set("power", json!(true), true),
            can_ask,
        )
        .await;
        assert!(
            is_error(&answer)
                && error_words(&answer).contains(&format!(
                    "powering on amp would play at {:.1} dB, above this server's limit of {:.1} dB \
                     (max_volume_db); lower the volume first",
                    level,
                    level - 5.0
                )),
            "{answer:?}"
        );
        let off = run(&fleet, Some(level - 5.0), power(false), can_ask).await;
        assert!(went(&off), "{off:?}");
    }
}

#[tokio::test]
async fn an_unreported_volume_is_not_silence() {
    // The unmute and the power-on say the volume is not known, never "silence".
    for (profile, request) in [
        ("gungnir2-v2", call("mute", json!({"on": false}))),
        ("freya2", power(true)),
    ] {
        let fleet = scripted(profile, &[]);
        let asked = run(&fleet, None, request, true).await;
        let CallToolResponse::InputRequired(asked) = asked else {
            panic!("{profile}: {asked:?}")
        };
        let text = format!("{:?}", asked.input_requests);
        assert!(
            text.contains("a volume the device has not reported yet") && !text.contains("silence"),
            "{profile}: {text}"
        );
    }
    // A real silence still reads as silence.
    let fleet = scripted("gungnir2-v2", &[("volume", 0), ("mute", 1)]);
    let asked = run(&fleet, None, call("mute", json!({"on": false})), true).await;
    let text = format!("{asked:?}");
    assert!(asks(&asked) && text.contains("silence"), "{text}");
    // A rise from an unreported volume says so too.
    let fleet = scripted("gungnir2-v2", &[]);
    let asked = run(&fleet, None, set("volume", json!(-60), false), true).await;
    let text = format!("{asked:?}");
    assert!(
        asks(&asked) && text.contains("has not reported yet") && !text.contains("silence"),
        "{text}"
    );
}

/// An unreported volume is never a reason to refuse: with a cap set, a target at or below it, an
/// unmute and a power-on ask, in the words of an unknown volume; only a target above the cap is
/// refused.
#[tokio::test]
async fn with_a_cap_an_unreported_volume_asks_and_refuses_only_above_the_cap() {
    let cap = Some(-20.0);
    for (profile, request) in [
        ("gungnir2-v2", call("mute", json!({"on": false}))),
        ("gungnir2-v2", set("volume", json!(-60), false)),
        ("freya2", set("power", json!(true), false)),
    ] {
        let fleet = scripted(profile, &[]);
        let asked = run(&fleet, cap, request.clone(), true).await;
        let text = format!("{asked:?}");
        assert!(
            asks(&asked) && text.contains("has not reported yet"),
            "{profile}: {text}"
        );
        let old = run(&fleet, cap, request.clone(), false).await;
        let words = error_words(&old);
        assert!(
            is_error(&old) && words.contains("confirm") && words.contains("has not reported yet"),
            "{profile}: {words}"
        );
        assert!(fleet.sets.lock().unwrap().is_empty(), "{profile}");
        let mut confirmed = request.clone();
        if let Some(arguments) = confirmed.arguments.as_mut() {
            arguments.insert("confirm".into(), json!(true));
        }
        let old = run(&fleet, cap, confirmed, false).await;
        assert!(went(&old), "{profile}: {old:?}");
    }
    // A target above the cap is the cap's own business, whatever the volume reads.
    let fleet = scripted("gungnir2-v2", &[]);
    for can_ask in [true, false] {
        let answer = run(&fleet, cap, set("volume", json!(-10), true), can_ask).await;
        assert!(
            is_error(&answer)
                && error_words(&answer).contains(
                    "raising the volume to -10.0 dB is above this server's limit of -20.0 dB \
                     (max_volume_db); choose a level at or below it"
                ),
            "{answer:?}"
        );
    }
}

/// The kind-aware volume test and `roles::volume` agree on every built-in profile: a profile
/// has a volume control exactly when one of its controls passes `is_volume`.
#[test]
fn the_kind_aware_volume_test_agrees_with_the_policys_lookup() {
    let registry = Registry::builtin().expect("the built-in profiles");
    let mut with_volume = 0;
    for name in registry.names() {
        let profile = registry.get(name).unwrap();
        let by_kind = profile.controls.iter().any(roles::is_volume);
        assert_eq!(by_kind, roles::volume(profile).is_some(), "{name}");
        with_volume += usize::from(by_kind);
    }
    assert!(with_volume > 0, "no built-in profile has a volume");
}

/// What `get_state` says of `profile`'s balance when the device reads raw step `raw`.
fn balance_state(profile: &str, raw: i64) -> Value {
    let registry = Registry::builtin().unwrap();
    let profile = registry.get(profile).unwrap();
    let control = profile.control_by_key("balance").unwrap();
    let mut value = reading("balance", raw);
    value.id = control.id.to_string();
    describe_control(profile, control, &[value])
}

#[test]
fn a_balance_reads_from_its_centre_with_its_range_shifted_to_match() {
    let freya = balance_state("freya2", 100);
    assert_eq!(freya["value"], json!(0));
    assert_eq!(freya["range"], json!({"min": -100, "max": 100}));
    assert_eq!(balance_state("freya2", 0)["value"], json!(-100));
    let mesh = balance_state("mesh-eq-3", 64);
    assert_eq!(mesh["value"], json!(0));
    assert_eq!(mesh["range"], json!({"min": -63, "max": 63}));
}

/// A balance is read from its raw step, counted from its centre, whatever converted value a
/// reading happens to carry.
#[test]
fn a_balance_reads_from_its_raw_step_whatever_value_the_reading_carries() {
    let registry = Registry::builtin().unwrap();
    let control = registry
        .get("freya2")
        .unwrap()
        .control_by_key("balance")
        .unwrap();
    let mut right = reading("balance", 130);
    right.value = Some(130.0);
    assert_eq!(value_of(control, &right), json!(30));
}

#[tokio::test]
async fn setting_a_balance_to_zero_centres_it_and_minus_100_is_hard_left() {
    for (profile, centre) in [("freya2", "100"), ("mesh-eq-3", "64")] {
        let fleet = scripted(profile, &[]);
        let answer = run(&fleet, None, set("balance", json!(0), false), false).await;
        assert!(went(&answer), "{profile}: {answer:?}");
        assert_eq!(
            fleet.sets.lock().unwrap().last().unwrap(),
            &("balance".to_string(), centre.to_string()),
            "{profile}"
        );
    }
    let fleet = scripted("freya2", &[]);
    let answer = run(&fleet, None, set("balance", json!(-100), false), false).await;
    assert!(went(&answer), "{answer:?}");
    assert_eq!(fleet.sets.lock().unwrap()[0].1, "0");
    let answer = run(&fleet, None, set("balance", json!(100), false), false).await;
    assert!(went(&answer), "{answer:?}");
    assert_eq!(fleet.sets.lock().unwrap()[1].1, "200");
}

#[tokio::test]
async fn a_balance_past_either_end_is_refused_naming_the_shifted_bounds() {
    let fleet = scripted("freya2", &[]);
    for out in [json!(101), json!(-101)] {
        let answer = run(&fleet, None, set("balance", out.clone(), false), false).await;
        assert!(is_error(&answer), "{out}: {answer:?}");
        let words = error_words(&answer);
        assert!(words.contains("between -100 and 100"), "{out}: {words}");
    }
    assert!(fleet.sets.lock().unwrap().is_empty());
}

#[tokio::test]
async fn the_reply_to_setting_a_balance_counts_it_from_its_centre() {
    for (asked, shown) in [(0, "0"), (-10, "-10")] {
        let fleet = scripted("freya2", &[]);
        let answer = run(&fleet, None, set("balance", json!(asked), false), false).await;
        assert!(went(&answer), "{answer:?}");
        let CallToolResponse::Complete(result) = &answer else {
            panic!("{answer:?}");
        };
        let reply = result
            .structured_content
            .as_ref()
            .expect("a structured reply");
        assert_eq!(reply["value"], json!(asked), "{reply}");
        assert_eq!(reply["display"], json!(shown), "{reply}");
    }
}
