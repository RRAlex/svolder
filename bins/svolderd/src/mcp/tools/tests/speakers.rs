// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The computer's virtual speakers through `get_host` and `set_host_speakers`.

use super::*;
use svolder_host::eq::{SetInfo, SpeakersView};

/// A fetched set, the sidecar's fields a list shows.
fn set_info(id: &str, room: svolder_host::eq::speakers::Room) -> SetInfo {
    SetInfo {
        id: id.to_string(),
        room,
        label: "Dummy head A".to_string(),
        licence: "CC-BY-4.0".to_string(),
        credit: Some("Credit words".to_string()),
        source_url: "https://example.invalid/set".to_string(),
        rate: 48_000,
        taps: 4_800,
        angles: "left 30.0 deg, right 330.0 deg".to_string(),
        loudness_db: -3.0,
        cut: false,
    }
}

/// A computer view whose virtual speakers have `sets` fetched, the first one chosen.
fn speakers_view(on: bool) -> svolder_ipc::host::HostView {
    use svolder_host::eq::speakers::Room;
    let sets = [set_info("a-brir", Room::With), set_info("b-hrtf", Room::No)];
    let mut view = computer_view();
    let curve = Default::default();
    view.eq.as_mut().unwrap().speakers =
        Some(SpeakersView::of(Some((&sets[0], &curve)), on, &sets, None));
    view
}

#[tokio::test]
async fn get_host_reads_the_speakers_with_their_terms_and_the_fetched_sets() {
    let fleet = computer_fleet(20);
    *fleet.host.lock().unwrap() = speakers_view(true);
    let reply = structured(&run(&fleet, None, call("get_host", json!({})), false).await);
    let sp = &reply["speakers"];
    assert_eq!(sp["on"], true);
    assert_eq!(sp["set"], "a-brir");
    assert_eq!(sp["room_name"], "With room");
    assert_eq!(sp["term"], "BRIR");
    assert_eq!(sp["licence"], "CC-BY-4.0");
    assert!(sp.get("curve").is_none(), "plot data is left out");
    let ids: Vec<&Value> = sp["sets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| &s["id"])
        .collect();
    assert_eq!(ids, [&json!("a-brir"), &json!("b-hrtf")]);
    assert_eq!(sp["sets"][1]["room_name"], "No room");
    assert_eq!(sp["sets"][1]["term"], "HRTF");
}

#[tokio::test]
async fn get_host_speakers_are_null_without_a_controller_or_with_the_eq_unavailable() {
    let fleet = computer_fleet(20);
    let reply = structured(&run(&fleet, None, call("get_host", json!({})), false).await);
    assert_eq!(reply["speakers"], Value::Null);
    let mut off = speakers_view(true);
    off.eq.as_mut().unwrap().unavailable = Some("no chain".into());
    *fleet.host.lock().unwrap() = off;
    let reply = structured(&run(&fleet, None, call("get_host", json!({})), false).await);
    assert_eq!(reply["speakers"], Value::Null);
}

#[tokio::test]
async fn set_host_speakers_sends_what_is_given_once_and_answers_with_the_view() {
    let fleet = computer_fleet(20);
    *fleet.host.lock().unwrap() = speakers_view(false);
    let reply = run(
        &fleet,
        None,
        call("set_host_speakers", json!({"on": true, "set": "b-hrtf"})),
        false,
    )
    .await;
    assert!(went(&reply), "{}", error_words(&reply));
    assert_eq!(structured(&reply)["speakers"]["on"], true);
    assert_eq!(structured(&reply)["speakers"]["set"], "b-hrtf");
    assert_eq!(
        host_sent(&fleet),
        [Request::HostEq {
            op: HostEqOp::Speakers {
                on: Some(true),
                set: Some("b-hrtf".into()),
                done: true,
            }
        }]
    );
}

#[tokio::test]
async fn set_host_speakers_refuses_an_empty_call_and_bad_words_before_sending() {
    let fleet = computer_fleet(20);
    *fleet.host.lock().unwrap() = speakers_view(false);
    for bad in [
        json!({}),
        json!({"on": null}),
        json!({"on": "yes"}),
        json!({"set": 3}),
        json!({"set": ""}),
        json!({"url": "https://example.invalid/x"}),
        json!({"fetch": true}),
        json!({"on": true, "confirm": true}),
        json!({"done": false}),
    ] {
        let reply = run(&fleet, None, call("set_host_speakers", bad.clone()), false).await;
        assert!(is_error(&reply), "{bad}");
    }
    assert!(host_sent(&fleet).is_empty());
}

#[tokio::test]
async fn set_host_speakers_is_listed_only_with_writes_and_its_words_name_the_terms() {
    let fleet = scripted("mesh-eq-5", &[]);
    assert!(!listed(&fleet, false).contains(&"set_host_speakers".to_string()));
    assert!(listed(&fleet, true).contains(&"set_host_speakers".to_string()));
    let tools = Tools::new(Arc::clone(&fleet) as Arc<dyn Fleet>, false, None);
    let reply = tools
        .call(
            call("set_host_speakers", json!({"on": true})),
            Caller { can_ask: false },
        )
        .await
        .unwrap();
    assert!(is_error(&reply));
    assert!(error_words(&reply).contains("[mcp] writes"));
    assert!(host_sent(&fleet).is_empty());
    use svolder_host::eq::speakers::Room;
    for name in ["set_host_speakers", "get_host"] {
        let all = format!("{:?}", super::schema::definition(name, name != "get_host"));
        for room in [Room::With, Room::No] {
            let words = format!("{} \u{b7} {}", room.name(), room.term());
            assert!(all.contains(&words), "{name}: {words}");
        }
    }
}

/// A view with crossfeed and the virtual speakers both there, `xf_on` and `sp_on` playing.
fn both_view(xf_on: bool, sp_on: bool) -> svolder_ipc::host::HostView {
    use svolder_host::eq::crossfeed::{CrossfeedSettings, CrossfeedView};
    let mut view = speakers_view(sp_on);
    view.eq.as_mut().unwrap().crossfeed = Some(CrossfeedView::of(
        &CrossfeedSettings::default(),
        xf_on,
        48_000,
        Some("Speakers".into()),
        None,
    ));
    view
}

#[tokio::test]
async fn each_tool_answers_with_both_so_the_other_going_off_is_seen() {
    let fleet = computer_fleet(20);
    *fleet.host.lock().unwrap() = both_view(false, true);
    let reply = run(
        &fleet,
        None,
        call("set_host_crossfeed", json!({"on": true})),
        false,
    )
    .await;
    assert!(went(&reply), "{}", error_words(&reply));
    let answer = structured(&reply);
    assert_eq!(answer["crossfeed"]["on"], true);
    assert_eq!(answer["speakers"]["on"], false);
    assert_eq!(
        answer["speakers"]["note"],
        "virtual speakers turned off: crossfeed is on"
    );

    *fleet.host.lock().unwrap() = both_view(true, false);
    let reply = run(
        &fleet,
        None,
        call("set_host_speakers", json!({"on": true})),
        false,
    )
    .await;
    assert!(went(&reply), "{}", error_words(&reply));
    let answer = structured(&reply);
    assert_eq!(answer["speakers"]["on"], true);
    assert_eq!(answer["crossfeed"]["on"], false);
    assert_eq!(
        answer["crossfeed"]["note"],
        "crossfeed turned off: virtual speakers are on"
    );
}

#[tokio::test]
async fn the_tools_say_each_turns_the_other_off() {
    let words = |name| format!("{:?}", super::schema::definition(name, true));
    assert!(
        words("set_host_crossfeed").contains("virtual speakers off"),
        "{}",
        words("set_host_crossfeed")
    );
    assert!(
        words("set_host_speakers").contains("crossfeed off"),
        "{}",
        words("set_host_speakers")
    );
}

#[tokio::test]
async fn a_set_that_was_never_fetched_is_refused_by_name_and_sends_nothing() {
    let fleet = computer_fleet(20);
    *fleet.host.lock().unwrap() = speakers_view(false);
    let reply = run(
        &fleet,
        None,
        call("set_host_speakers", json!({"set": "guessed"})),
        false,
    )
    .await;
    let words = error_words(&reply);
    assert!(is_error(&reply));
    assert!(
        words.contains("guessed") && words.contains("a-brir") && words.contains("b-hrtf"),
        "{words}"
    );
    assert!(
        words.contains("svolder host speakers") && words.contains("TUI") && words.contains("web"),
        "{words}"
    );
    assert!(!words.contains("damaged"), "{words}");
    assert!(host_sent(&fleet).is_empty());
    // An EQ that cannot be used gives its own words, not "not available on this computer".
    let mut down = speakers_view(false);
    down.eq.as_mut().unwrap().unavailable = Some("computer controls unavailable: no chain".into());
    *fleet.host.lock().unwrap() = down;
    let reply = run(
        &fleet,
        None,
        call("set_host_speakers", json!({"set": "a-brir"})),
        false,
    )
    .await;
    assert!(is_error(&reply));
    assert!(
        error_words(&reply).contains("no chain"),
        "{}",
        error_words(&reply)
    );
    assert!(host_sent(&fleet).is_empty());
    // With no speakers at all the words say so.
    *fleet.host.lock().unwrap() = computer_view();
    let reply = run(
        &fleet,
        None,
        call("set_host_speakers", json!({"set": "a-brir"})),
        false,
    )
    .await;
    assert!(is_error(&reply));
    assert!(
        error_words(&reply).contains("not available"),
        "{}",
        error_words(&reply)
    );
    assert!(host_sent(&fleet).is_empty());
}

#[tokio::test]
async fn set_host_speakers_passes_the_daemons_refusal_on() {
    let fleet = computer_fleet(20);
    *fleet.host.lock().unwrap() = speakers_view(false);
    *fleet.host_error.lock().unwrap() = Some("no set is chosen".into());
    let reply = run(
        &fleet,
        None,
        call("set_host_speakers", json!({"on": true})),
        false,
    )
    .await;
    assert!(is_error(&reply));
    assert!(error_words(&reply).contains("no set is chosen"));
}

#[tokio::test]
async fn with_no_set_chosen_the_set_fields_are_null_not_zero() {
    let fleet = computer_fleet(20);
    let mut view = computer_view();
    view.eq.as_mut().unwrap().speakers = Some(SpeakersView::of(None, false, &[], None));
    *fleet.host.lock().unwrap() = view;
    let reply = structured(&run(&fleet, None, call("get_host", json!({})), false).await);
    let sp = &reply["speakers"];
    for key in [
        "set",
        "label",
        "licence",
        "source_url",
        "angles",
        "taps_ms",
        "loudness_db",
    ] {
        assert_eq!(sp[key], Value::Null, "{key}");
    }
    assert_eq!(sp["sets"], json!([]));
}
