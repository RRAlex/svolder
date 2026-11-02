// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The route read from `pw-dump` fixtures: real output captured on the development machine,
//! and the files derived from it (see `tests/fixtures/pw-dump/README.md`).

use super::*;

const ONE_STREAM: &str = include_str!("../../tests/fixtures/pw-dump/one-stream.json");
const TWO_STREAMS: &str = include_str!("../../tests/fixtures/pw-dump/two-streams.json");
const HOST_EQ: &str = include_str!("../../tests/fixtures/pw-dump/host-eq.json");
const HOST_EQ_DEFAULT: &str = include_str!("../../tests/fixtures/pw-dump/host-eq-default.json");
const SWITCHED: &str = include_str!("../../tests/fixtures/pw-dump/sink-switched.json");
const ELSEWHERE: &str = include_str!("../../tests/fixtures/pw-dump/stream-elsewhere.json");
const SCHIIT: &str = include_str!("../../tests/fixtures/pw-dump/schiit-sink.json");

/// The `pw-cat` client's pid in `one-stream.json`, its only running stream.
const PW_CAT_PID: u32 = 1_790_719;
/// The two `pw-cat` clients' pids in `two-streams.json`: 44.1 kHz, then 48 kHz ("Fixture Other").
const FIRST_PID: u32 = 1_790_841;
const SECOND_PID: u32 = 1_790_842;
/// The idle Firefox stream's pid in both.
const FIREFOX_PID: u32 = 123_123;

const USB_SPEAKERS: &str = "alsa_output.usb-Generic_USB_Audio-00.HiFi__Speaker__sink";
const HDMI: &str = "alsa_output.pci-0000_11_00.1.hdmi-stereo";

fn facts(dump: &str, pid: Option<u32>, identity: &str) -> RouteFacts {
    route(dump, pid, identity).expect("the fixture is a pw-dump")
}

/// A stream's facts: its negotiated rate and format and its own volume, unmuted.
fn stream(rate_hz: u32, format: &str, volume_milli: u32) -> Option<StreamFacts> {
    Some(StreamFacts {
        rate_hz: Some(rate_hz),
        format: Some(format.to_string()),
        volume_milli: Some(volume_milli),
        muted: false,
    })
}

/// `dump` with the node named `node_name` edited by `edit`, as the text `route` reads.
fn edited(dump: &str, node_name: &str, edit: impl FnOnce(&mut serde_json::Value)) -> String {
    let mut objects: Vec<serde_json::Value> = serde_json::from_str(dump).expect("valid JSON");
    let node = objects
        .iter_mut()
        .find(|object| object["info"]["props"]["node.name"] == node_name)
        .expect("the node is in the fixture");
    edit(node);
    serde_json::to_string(&objects).expect("it serialises")
}

/// The first `Props` entry of a node, where its volumes and mutes are.
fn props_of(node: &mut serde_json::Value) -> &mut serde_json::Value {
    &mut node["info"]["params"]["Props"][0]
}

#[test]
fn the_default_sink_and_its_rate_are_read() {
    let found = facts(ONE_STREAM, None, "");
    let sink = found.sink.expect("the default sink is found");
    assert_eq!(sink.name, HDMI);
    assert_eq!(
        sink.description,
        "Radeon High Definition Audio Controller Digital Stereo (HDMI)"
    );
    assert_eq!(sink.rate_hz, Some(48_000));
    assert_eq!(sink.format.as_deref(), Some("S32LE"));
    assert!(!sink.muted);
    assert_eq!(sink.usb_vendor_id, None);
    assert_eq!(sink.usb_product, None);
}

/// `wpctl status` showed this sink at 0.24, which is its linear 0.013822 cube-rooted.
#[test]
fn the_volume_is_the_cube_root_scale_wpctl_shows() {
    let sink = facts(ONE_STREAM, None, "").sink.expect("a sink");
    assert_eq!(sink.volume_milli, Some(240));
}

#[test]
fn the_player_is_matched_by_pid() {
    let found = facts(TWO_STREAMS, Some(SECOND_PID), "unrelated name");
    assert_eq!(found.player_match, PlayerMatch::Pid);
    assert_eq!(found.player, stream(48_000, "S16LE", 0));
    let found = facts(TWO_STREAMS, Some(FIRST_PID), "");
    assert_eq!(found.player_match, PlayerMatch::Pid);
    assert_eq!(found.player, stream(44_100, "S16LE", 0));
}

/// A paused player's stream is idle, but it is still the player's.
#[test]
fn an_idle_stream_is_matched_by_pid_too() {
    let found = facts(ONE_STREAM, Some(FIREFOX_PID), "Mozilla Firefox");
    assert_eq!(found.player_match, PlayerMatch::Pid);
    assert_eq!(found.others_mixing, 1);
}

#[test]
fn the_player_is_matched_by_name_when_the_pid_is_unknown() {
    let found = facts(TWO_STREAMS, None, "fixture other");
    assert_eq!(found.player_match, PlayerMatch::Name);
    assert_eq!(found.player, stream(48_000, "S16LE", 0));
    let found = facts(TWO_STREAMS, Some(7), "Mozilla Firefox");
    assert_eq!(found.player_match, PlayerMatch::Name);
    assert_eq!(found.player, stream(48_000, "F32LE", 909));
}

#[test]
fn a_lone_running_stream_is_assumed_to_be_the_player() {
    let found = facts(ONE_STREAM, Some(7), "Nobody");
    assert_eq!(found.player_match, PlayerMatch::Assumed);
    assert_eq!(found.player, stream(44_100, "S16LE", 0));
    assert_eq!(found.others_mixing, 0);
}

#[test]
fn two_unmatched_streams_leave_the_player_not_found() {
    let found = facts(TWO_STREAMS, Some(7), "Nobody");
    assert_eq!(found.player_match, PlayerMatch::NotFound);
    assert_eq!(found.player, None);
}

#[test]
fn others_mixing_counts_running_streams_but_not_the_player() {
    let found = facts(TWO_STREAMS, Some(FIRST_PID), "");
    assert_eq!(found.others_mixing, 1);
    let found = facts(ONE_STREAM, Some(PW_CAT_PID), "");
    assert_eq!(found.others_mixing, 0);
    // The idle Firefox and speech dispatcher streams are linked but not running.
    let found = facts(TWO_STREAMS, Some(7), "Nobody");
    assert_eq!(found.others_mixing, 2);
}

#[test]
fn no_followed_player_means_no_match_and_every_running_stream_is_another() {
    let found = facts(TWO_STREAMS, None, "");
    assert_eq!(found.player_match, PlayerMatch::NoPlayer);
    assert_eq!(found.player, None);
    assert_eq!(found.others_mixing, 2);
}

#[test]
fn the_host_eq_node_in_the_path_is_seen() {
    assert!(!facts(ONE_STREAM, Some(PW_CAT_PID), "").host_eq);
    for dump in [HOST_EQ, HOST_EQ_DEFAULT] {
        let found = facts(dump, Some(PW_CAT_PID), "");
        assert!(found.host_eq);
        assert_eq!(found.player_match, PlayerMatch::Pid);
        assert_eq!(found.player, stream(44_100, "S16LE", 0));
        assert_eq!(found.others_mixing, 0);
        // The sink is the real one behind the filter, whichever of them is the default.
        assert_eq!(found.sink.expect("a sink").name, HDMI);
    }
}

/// The computer volume is the sink's times the filter node's: 0.24 of the sink and 0.5 of the
/// filter's node make 0.12.
#[test]
fn the_filter_nodes_volume_counts_when_it_is_in_the_path() {
    let found = facts(HOST_EQ, Some(PW_CAT_PID), "");
    assert_eq!(found.sink.expect("a sink").volume_milli, Some(120));
}

#[test]
fn a_default_sink_switch_is_seen() {
    let before = facts(ONE_STREAM, Some(PW_CAT_PID), "");
    let after = facts(SWITCHED, Some(PW_CAT_PID), "");
    assert_eq!(before.sink.expect("a sink").name, HDMI);
    let sink = after.sink.expect("a sink");
    assert_eq!(sink.name, USB_SPEAKERS);
    assert_eq!(sink.description, "USB Audio Speakers");
    assert_eq!(after.player_match, PlayerMatch::Pid);
    assert_eq!(after.player, stream(44_100, "S16LE", 0));
}

/// A USB sink gives the vendor id and the product name of its device object.
#[test]
fn a_usb_sink_reports_its_usb_identity() {
    let sink = facts(SWITCHED, None, "").sink.expect("a sink");
    assert_eq!(sink.usb_vendor_id, Some(0x0414));
    assert_eq!(sink.usb_product.as_deref(), Some("USB Audio"));
}

#[test]
fn a_schiit_sink_reports_its_usb_identity() {
    let sink = facts(SCHIIT, Some(PW_CAT_PID), "").sink.expect("a sink");
    assert_eq!(sink.usb_vendor_id, Some(0x30be));
    assert_eq!(sink.usb_product.as_deref(), Some("Gungnir 2"));
    assert_eq!(sink.description, "Schiit Gungnir 2 Analog Stereo");
    assert_eq!(sink.rate_hz, Some(48_000));
}

#[test]
fn garbage_json_is_none() {
    for text in ["", "not json", "{}", "42", "\"pw\""] {
        assert_eq!(route(text, None, ""), None, "{text:?}");
    }
}

#[test]
fn a_dump_without_a_default_sink_has_no_sink() {
    let found = route("[]", Some(1), "Spotify").expect("an empty graph is a graph");
    assert_eq!(found.sink, None);
    assert_eq!(found.player_match, PlayerMatch::NotFound);
    assert_eq!(found.others_mixing, 0);
}

#[test]
fn the_facts_survive_the_wire() {
    let found = facts(HOST_EQ, Some(PW_CAT_PID), "");
    let text = serde_json::to_string(&found).expect("it serialises");
    assert_eq!(
        serde_json::from_str::<RouteFacts>(&text).expect("it reads back"),
        found
    );
}

/// The idle Firefox stream carries a channel volume of 0.749985 (linear, which `wpctl` shows
/// as 0.91), and the silent `pw-cat` stream a master volume of 0.0 with its channels at full.
#[test]
fn a_streams_own_volume_is_read_on_the_wpctl_scale() {
    let firefox = facts(ONE_STREAM, Some(FIREFOX_PID), "");
    assert_eq!(firefox.player, stream(48_000, "F32LE", 909));
    let silent = facts(ONE_STREAM, Some(PW_CAT_PID), "");
    assert_eq!(silent.player, stream(44_100, "S16LE", 0));
}

#[test]
fn a_streams_mute_is_read_hard_or_soft() {
    for key in ["mute", "softMute"] {
        let dump = edited(ONE_STREAM, "pw-cat", |node| {
            props_of(node)[key] = true.into()
        });
        let found = facts(&dump, Some(PW_CAT_PID), "");
        assert!(found.player.expect("the stream").muted, "{key}");
    }
    assert!(
        !facts(ONE_STREAM, Some(PW_CAT_PID), "")
            .player
            .expect("a stream")
            .muted
    );
}

#[test]
fn a_player_that_plays_to_another_sink_is_elsewhere() {
    let found = facts(ELSEWHERE, Some(PW_CAT_PID), "");
    assert_eq!(
        found.player_match,
        PlayerMatch::Elsewhere {
            sink: "USB Audio Speakers".to_string()
        }
    );
    assert_eq!(found.player, None);
    assert!(!found.host_eq);
    // The default sink is still the HDMI one, and nothing running plays into it.
    assert_eq!(found.sink.expect("a sink").name, HDMI);
    assert_eq!(found.others_mixing, 0);
    // By name too.
    let found = facts(ELSEWHERE, None, "pw-cat");
    assert!(matches!(found.player_match, PlayerMatch::Elsewhere { .. }));
}

/// A stream of the player's own that is somewhere else is never swapped for another stream
/// that happens to be the only one running into the sink.
#[test]
fn a_lone_stream_is_not_assumed_when_the_player_has_its_own_elsewhere() {
    // Firefox's idle stream is linked to the default sink; make it run, and move the player's
    // (pw-cat's) to the USB sink: the player is elsewhere and Firefox is another sound.
    let dump = edited(ELSEWHERE, "Firefox", |node| {
        node["info"]["state"] = "running".into();
    });
    let found = facts(&dump, Some(PW_CAT_PID), "");
    assert!(matches!(found.player_match, PlayerMatch::Elsewhere { .. }));
    assert_eq!(found.others_mixing, 1);
    // Without a stream of its own anywhere, the lone running one is assumed.
    let found = facts(&dump, Some(7), "Nobody");
    assert_eq!(found.player_match, PlayerMatch::Assumed);
    assert_eq!(found.player, stream(48_000, "F32LE", 909));
}

/// A player with a stream in the graph that is linked to no sink is not found, and nothing
/// else is taken for it.
#[test]
fn a_players_unlinked_stream_leaves_it_not_found() {
    let mut objects: Vec<serde_json::Value> = serde_json::from_str(ONE_STREAM).expect("JSON");
    let pw_cat = objects
        .iter()
        .find(|object| object["info"]["props"]["node.name"] == "pw-cat")
        .and_then(|node| node["id"].as_u64())
        .expect("pw-cat is in the fixture");
    objects.retain(|object| object["info"]["output-node-id"].as_u64() != Some(pw_cat));
    let dump = serde_json::to_string(&objects).expect("it serialises");
    let found = facts(&dump, Some(PW_CAT_PID), "");
    assert_eq!(found.player_match, PlayerMatch::NotFound);
    assert_eq!(found.player, None);
}

/// Two running streams of the player's own: one is its stream, the other is a sound mixing in.
#[test]
fn a_second_running_stream_of_the_player_counts_as_mixing() {
    // Both `pw-cat` streams carry the node name `pw-cat`.
    let found = facts(TWO_STREAMS, None, "pw-cat");
    assert_eq!(found.player_match, PlayerMatch::Name);
    assert!(found.player.is_some());
    assert_eq!(found.others_mixing, 1);
}

#[test]
fn a_muted_sink_is_muted_hard_or_soft() {
    assert!(!facts(ONE_STREAM, None, "").sink.expect("a sink").muted);
    for key in ["mute", "softMute"] {
        let dump = edited(ONE_STREAM, HDMI, |node| props_of(node)[key] = true.into());
        assert!(facts(&dump, None, "").sink.expect("a sink").muted, "{key}");
    }
}

#[test]
fn a_muted_filter_node_mutes_the_path_only_while_it_is_in_it() {
    let dump = edited(HOST_EQ, "svolder_eq", |node| {
        props_of(node)["mute"] = true.into()
    });
    assert!(
        facts(&dump, Some(PW_CAT_PID), "")
            .sink
            .expect("a sink")
            .muted
    );
    // A player that bypasses the filter is not muted by it.
    let dump = edited(HOST_EQ, "svolder_eq", |node| {
        props_of(node)["mute"] = true.into()
    });
    let found = facts(&dump, Some(FIREFOX_PID), "");
    assert!(!found.host_eq);
    assert!(!found.sink.expect("a sink").muted);
}

#[test]
fn the_sinks_master_volume_counts_with_its_channels() {
    let dump = edited(ONE_STREAM, HDMI, |node| {
        props_of(node)["volume"] = 0.5.into()
    });
    // 0.5 x 0.013822 linear, cube-rooted: 0.24 x cbrt(0.5).
    let sink = facts(&dump, None, "").sink.expect("a sink");
    assert_eq!(sink.volume_milli, Some(190));
}

/// A sink with no negotiated `Format` has no rate, even when its `node.rate` names one.
#[test]
fn a_sink_without_a_format_has_no_rate() {
    let dump = edited(ONE_STREAM, HDMI, |node| {
        node["info"]["params"]
            .as_object_mut()
            .expect("params")
            .remove("Format");
        node["info"]["props"]["node.rate"] = "1/44100".into();
    });
    let sink = facts(&dump, None, "").sink.expect("a sink");
    assert_eq!(sink.rate_hz, None);
    assert_eq!(sink.format, None);
}

#[test]
fn the_program_comes_from_the_environment_unless_host_audio_is_off() {
    use std::ffi::OsStr;
    let program = |dump: Option<PwDump>| dump.map(|dump| dump.program);
    assert_eq!(
        program(PwDump::resolve(true, Some(OsStr::new("/x/pw-dump")))),
        None
    );
    assert_eq!(
        program(PwDump::resolve(false, Some(OsStr::new("/x/pw-dump")))),
        Some("/x/pw-dump".into())
    );
    for unset in [None, Some(OsStr::new(""))] {
        assert_eq!(
            program(PwDump::resolve(false, unset)),
            Some("pw-dump".into())
        );
    }
}

/// A view from a newer daemon still decodes: a way of finding the player this build has no name
/// for reads as not found, and a field that went missing reads as its default.
#[test]
fn a_route_from_a_newer_daemon_still_decodes() {
    let route: RouteFacts = serde_json::from_str(
        r#"{"sink": {"name": "s", "description": "S"}, "player": {"rate_hz": 48000},
            "player_match": {"Parked": {"since": 3}}, "future_field": true}"#,
    )
    .expect("an unknown way is not a decode failure");
    assert_eq!(route.player_match, PlayerMatch::NotFound);
    assert_eq!(
        route.player.map(|stream| stream.rate_hz),
        Some(Some(48_000))
    );
    assert_eq!((route.others_mixing, route.host_eq), (0, false));
    let sink = route.sink.expect("a sink");
    assert!(!sink.muted);

    let route: RouteFacts = serde_json::from_str(r#"{"player_match": "unheard_of"}"#).unwrap();
    assert_eq!(route.player_match, PlayerMatch::NotFound);
    let route: RouteFacts = serde_json::from_str("{}").unwrap();
    assert_eq!(route, RouteFacts::default());
}

#[test]
fn every_known_way_of_finding_the_player_round_trips() {
    for found in [
        PlayerMatch::Pid,
        PlayerMatch::Name,
        PlayerMatch::Assumed,
        PlayerMatch::Elsewhere { sink: "TV".into() },
        PlayerMatch::NotFound,
        PlayerMatch::NoPlayer,
    ] {
        let route = RouteFacts {
            player_match: found.clone(),
            ..RouteFacts::default()
        };
        let text = serde_json::to_string(&route).unwrap();
        let back: RouteFacts = serde_json::from_str(&text).unwrap();
        assert_eq!(back.player_match, found, "{text}");
    }
}

/// A stream into the computer EQ's filter, while the default sink is another and the filter
/// feeds the HDMI one, plays into the HDMI sink: the line names it, not the filter.
#[test]
fn a_stream_into_the_filter_that_feeds_another_sink_names_that_sink() {
    let mut objects: Vec<serde_json::Value> = serde_json::from_str(HOST_EQ).expect("JSON");
    let default = objects
        .iter_mut()
        .find(|object| object["props"]["metadata.name"] == "default")
        .expect("the default metadata");
    for entry in default["metadata"].as_array_mut().expect("entries") {
        if entry["key"] == "default.audio.sink" {
            entry["value"] = serde_json::json!({ "name": USB_SPEAKERS });
        }
    }
    let dump = serde_json::to_string(&objects).expect("it serialises");
    let found = facts(&dump, Some(PW_CAT_PID), "");
    assert_eq!(
        found.player_match,
        PlayerMatch::Elsewhere {
            sink: "Radeon High Definition Audio Controller Digital Stereo (HDMI)".to_string()
        }
    );
    assert!(!found.host_eq);
    assert_eq!(found.sink.expect("a sink").name, USB_SPEAKERS);
}

/// A player with one stream linked to no sink and another that plays into another sink is
/// elsewhere, not unidentified, whichever stream has the lower id.
#[test]
fn a_linked_stream_elsewhere_wins_over_an_unlinked_one() {
    let mut objects: Vec<serde_json::Value> = serde_json::from_str(ELSEWHERE).expect("JSON");
    let mut unlinked = objects
        .iter()
        .find(|object| object["info"]["props"]["node.name"] == "pw-cat")
        .expect("pw-cat is in the fixture")
        .clone();
    // Lower than every id in the graph, and running: it wins the old tie-break.
    unlinked["id"] = 1.into();
    unlinked["info"]["state"] = "running".into();
    objects.push(unlinked);
    let dump = serde_json::to_string(&objects).expect("it serialises");
    let found = facts(&dump, Some(PW_CAT_PID), "");
    assert_eq!(
        found.player_match,
        PlayerMatch::Elsewhere {
            sink: "USB Audio Speakers".to_string()
        }
    );
}
