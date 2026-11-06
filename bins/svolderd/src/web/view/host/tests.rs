// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use svolder_host::{PlayerMatch, Progress, RouteFacts, SinkFacts, StreamFacts};
use svolder_ipc::host::{HostView, SCHIIT_VENDOR_ID};

use super::super::tests::{OPEN, info, read, shot};
use super::*;
use crate::test_support::profile_named;
use crate::web::view::Access;

/// A track playing, with a cover and a position.
fn track_view() -> PlayingView {
    PlayingView {
        title: "Test Pressing".into(),
        artist: "The Fakes".into(),
        album: "Demo Tape".into(),
        player: "Fake Player".into(),
        status: "playing".into(),
        length_us: Some(1_200_000_000),
        progress: Some(Progress {
            position_us: 30_000_000,
            rate_milli: 1000,
            read_at_ms: 1_700_000_000_000,
        }),
        cover: Some("0123456789abcdef".into()),
    }
}

/// A route on a Schiit sink named `product` at 48 kHz, the player's stream matched, nothing in
/// the way: `untouched` when the device reads 48 kHz.
fn route(product: &str) -> RouteFacts {
    RouteFacts {
        sink: Some(SinkFacts {
            name: "alsa_output.usb-Schiit".into(),
            description: product.into(),
            rate_hz: Some(48_000),
            format: Some("S32LE".into()),
            volume_milli: Some(1000),
            muted: false,
            usb_vendor_id: Some(SCHIIT_VENDOR_ID),
            usb_product: Some(product.into()),
        }),
        player: Some(StreamFacts {
            rate_hz: Some(48_000),
            format: Some("S16LE".into()),
            volume_milli: Some(1000),
            muted: false,
        }),
        player_match: PlayerMatch::Pid,
        others_mixing: 0,
        host_eq: false,
    }
}

/// A host that is available with `playing` and `route`.
fn host(playing: Option<PlayingView>, route: Option<RouteFacts>) -> HostView {
    HostView {
        available: true,
        playing,
        route,
        now_ms: 1_700_000_003_000,
    }
}

/// The card `host` earns on a Gungnir 2 reading sample rate step `rate` (1 is 44.1 kHz, 2 is
/// 48 kHz), as `access` sees it.
fn card_on(host: &HostView, rate: Option<&str>, access: Access) -> Option<Playing> {
    card_with(host, Some("USB 1"), rate, access)
}

/// [`card_on`] with the input the device reads named too, and the computer on the USB port that
/// input names.
fn card_with(
    host: &HostView,
    input: Option<&str>,
    rate: Option<&str>,
    access: Access,
) -> Option<Playing> {
    let ports = match input {
        Some("USB 2") => ("false", "true"),
        _ => ("true", "false"),
    };
    card_over_ports(host, input, rate, ports, access)
}

/// [`card_with`] with the readings of the USB 1 and USB 2 ports (`true` or `false`) given.
fn card_over_ports(
    host: &HostView,
    input: Option<&str>,
    rate: Option<&str>,
    ports: (&str, &str),
    access: Access,
) -> Option<Playing> {
    let p = profile_named("gungnir2-v2");
    let name = info("Gungnir 2");
    let readings: Vec<_> = input
        .map(|input| read(&p, "input", input))
        .into_iter()
        .chain(rate.map(|rate| read(&p, "sample_rate", rate)))
        .chain([
            read(&p, "usb1_connected", ports.0),
            read(&p, "usb2_connected", ports.1),
        ])
        .collect();
    let s = Snapshot {
        host,
        access,
        ..shot(&name, &p, &readings)
    };
    playing(&s, Family::Dac)
}

/// With the host unavailable the page has no card at all.
#[test]
fn an_unavailable_host_has_no_card() {
    assert!(card_on(&HostView::default(), None, OPEN).is_none());
}

/// A host with no player gives the one quiet line: a card with no track, no path and (for one
/// who may write) the buttons the page does not draw without a track.
#[test]
fn no_player_is_a_card_without_a_track() {
    let card = card_on(&host(None, Some(route("Schiit Gungnir 2"))), None, OPEN).expect("a card");
    assert!(card.track.is_none());
    assert!(card.path.is_none());
}

/// The track's words, status, cover and the position the page counts from.
#[test]
fn the_track_carries_its_words_and_its_progress() {
    let card = card_on(&host(Some(track_view()), None), None, OPEN).expect("a card");
    let track = card.track.expect("a track");
    assert_eq!(card.now_ms, 1_700_000_003_000);
    assert_eq!(track.headline, "Test Pressing");
    assert_eq!(track.artist, "The Fakes");
    assert_eq!(track.source, "Demo Tape · Fake Player");
    assert_eq!(track.status, "playing");
    assert_eq!(track.cover.as_deref(), Some("0123456789abcdef"));
    assert_eq!(track.length_us, Some(1_200_000_000));
    assert_eq!(
        track.progress,
        Some(Progress {
            position_us: 30_000_000,
            rate_milli: 1000,
            read_at_ms: 1_700_000_000_000,
        })
    );
}

/// The album is a field of its own, so an album that holds the source's separator stays whole.
#[test]
fn the_track_names_its_album_apart() {
    let track = card_on(&host(Some(track_view()), None), None, OPEN)
        .and_then(|card| card.track)
        .expect("a track");
    assert_eq!(track.album, "Demo Tape");
    assert_eq!(track.source, "Demo Tape · Fake Player");
    let blank = PlayingView {
        album: "  ".into(),
        ..track_view()
    };
    let track = card_on(&host(Some(blank), None), None, OPEN)
        .and_then(|card| card.track)
        .expect("a track");
    assert_eq!(track.album, "");
}

/// A title that is not there is replaced by a word that says why, a status that is not known
/// reads as stopped, and a cover that is no plain hash is not put in an address.
#[test]
fn odd_tracks_read_plainly() {
    let odd = PlayingView {
        title: "  ".into(),
        status: "stopped".into(),
        cover: Some("../../etc".into()),
        ..PlayingView::default()
    };
    let track = card_on(&host(Some(odd), None), None, OPEN)
        .and_then(|card| card.track)
        .expect("a track");
    assert_eq!(track.headline, "Stopped");
    assert_eq!((track.artist.as_str(), track.source.as_str()), ("", ""));
    assert_eq!(track.cover, None);
    assert_eq!((track.length_us, track.progress), (None, None));
    let unnamed = PlayingView {
        status: "playing".into(),
        ..PlayingView::default()
    };
    let track = card_on(&host(Some(unnamed), None), None, OPEN)
        .and_then(|card| card.track)
        .expect("a track");
    assert_eq!(track.headline, "Unknown track");
    let strange = PlayingView {
        status: "buffering".into(),
        title: "x".into(),
        ..PlayingView::default()
    };
    let track = card_on(&host(Some(strange), None), None, OPEN)
        .and_then(|card| card.track)
        .expect("a track");
    assert_eq!(track.status, "stopped");
}

/// A route on the device's own sink at the rate the device reads is untouched, with each hop
/// named for the details.
#[test]
fn the_path_is_untouched_when_every_hop_is() {
    let view = host(Some(track_view()), Some(route("Schiit Gungnir 2")));
    let card = card_on(&view, Some("2"), OPEN).expect("a card");
    let path = card.path.expect("a path");
    assert_eq!((path.class, path.words.as_str()), ("ok", "untouched"));
    let labels: Vec<_> = path.hops.iter().map(|hop| hop.label.as_str()).collect();
    assert_eq!(labels, ["player", "computer", "DAC"]);
    assert!(path.hops.iter().all(|hop| !hop.changes), "{path:?}");
    assert!(path.hops[2].detail.contains("48 kHz"), "{:?}", path.hops[2]);
}

/// The device's own sample-rate reading is what the last hop is judged by: another rate names
/// itself, no reading says what is not known.
#[test]
fn the_path_is_judged_by_the_devices_own_readings() {
    let view = host(Some(track_view()), Some(route("Schiit Gungnir 2")));
    let path = card_on(&view, Some("1"), OPEN)
        .and_then(|card| card.path)
        .expect("a path");
    assert_eq!(
        (path.class, path.words.as_str()),
        ("changed", "DAC reads 44.1 kHz")
    );
    assert!(path.hops[2].changes);
    let path = card_on(&view, None, OPEN)
        .and_then(|card| card.path)
        .expect("a path");
    assert_eq!(
        (path.class, path.words.as_str()),
        ("unknown", "DAC rate not read")
    );
}

/// A DAC that listens to another input is no verdict on the computer; one whose input is not
/// read is not untouched.
#[test]
fn a_dac_on_another_input_or_none_read_is_no_clean_line() {
    let view = host(Some(track_view()), Some(route("Schiit Gungnir 2")));
    let path = card_with(&view, Some("Optical"), Some("2"), OPEN)
        .and_then(|card| card.path)
        .expect("a path");
    assert_eq!(
        (path.class, path.words.as_str()),
        ("other", "the DAC listens to Optical")
    );
    assert!(path.hops.is_empty());
    let path = card_with(&view, Some("USB 2"), Some("2"), OPEN)
        .and_then(|card| card.path)
        .expect("a path");
    assert_eq!((path.class, path.words.as_str()), ("ok", "untouched"));
    let path = card_with(&view, None, Some("2"), OPEN)
        .and_then(|card| card.path)
        .expect("a path");
    assert_eq!(
        (path.class, path.words.as_str()),
        ("unknown", "DAC input not read")
    );
}

/// A DAC on a USB port the computer is not on is no verdict on the computer; both ports connected
/// is not untouched: the page judges the ports by the readings the daemon holds.
#[test]
fn a_dac_on_the_usb_port_the_computer_is_not_on_is_no_clean_line() {
    let view = host(Some(track_view()), Some(route("Schiit Gungnir 2")));
    let path = card_over_ports(&view, Some("USB 1"), Some("2"), ("false", "true"), OPEN)
        .and_then(|card| card.path)
        .expect("a path");
    assert_eq!(
        (path.class, path.words.as_str()),
        (
            "other",
            "the DAC listens to USB 1 · the computer is on USB 2"
        )
    );
    assert!(path.hops.is_empty());
    let path = card_over_ports(&view, Some("USB 1"), Some("2"), ("true", "true"), OPEN)
        .and_then(|card| card.path)
        .expect("a path");
    assert_eq!(
        (path.class, path.words.as_str()),
        ("unknown", "both USB ports connected")
    );
}

/// A computer that plays to another sink gives one sentence and no hops.
#[test]
fn a_sound_that_goes_elsewhere_says_so() {
    let view = host(Some(track_view()), Some(route("Schiit Bifrost 3")));
    let path = card_on(&view, Some("2"), OPEN)
        .and_then(|card| card.path)
        .expect("a path");
    assert_eq!(path.class, "other");
    assert_eq!(path.words, "the computer plays to Schiit Bifrost 3");
    assert!(path.hops.is_empty());
}

/// No route read yet says so, as the TUI's row does; no track to follow, no path line.
#[test]
fn no_route_says_so_and_no_track_has_no_path() {
    let unread = card_on(&host(Some(track_view()), None), Some("2"), OPEN).expect("a card");
    let path = unread.path.expect("a path");
    assert_eq!(
        (path.class, path.words.as_str()),
        ("unknown", "route not read")
    );
    assert!(path.hops.is_empty());
    let none = card_on(&host(None, None), Some("2"), OPEN).expect("a card");
    assert!(none.path.is_none());
}

/// An amp's card has no path line even while the route is unread.
#[test]
fn an_amp_with_no_route_has_no_path_either() {
    let p = profile_named("fake-amp");
    let name = info("Fake amp");
    let view = host(Some(track_view()), None);
    let s = Snapshot {
        host: &view,
        ..shot(&name, &p, &[])
    };
    assert!(playing(&s, Family::Amp).expect("a card").path.is_none());
}

/// An amp has no path: nothing it plays comes over the computer's audio.
#[test]
fn an_amp_has_the_card_but_no_path() {
    let p = profile_named("fake-amp");
    let name = info("Fake amp");
    let view = host(Some(track_view()), Some(route("Schiit Fake amp")));
    let s = Snapshot {
        host: &view,
        ..shot(&name, &p, &[])
    };
    let card = playing(&s, Family::Amp).expect("a card");
    assert!(card.track.is_some());
    assert!(card.path.is_none());
}

/// The visualizer names the sink it listens to unless it is this page's own DAC: another sink
/// and an amp (which plays nothing over the computer's audio) name it, and a DAC page whose
/// route is not read yet names nothing.
#[test]
fn the_visualizer_names_the_sink_unless_it_is_this_dac() {
    let label = |view: &HostView| card_on(view, Some("2"), OPEN).expect("a card").sink_label;
    assert!(!label(&host(
        Some(track_view()),
        Some(route("Schiit Gungnir 2"))
    )));
    assert!(label(&host(
        Some(track_view()),
        Some(route("Schiit Bifrost 3"))
    )));
    assert!(
        !label(&host(Some(track_view()), None)),
        "no route read: no label to blink"
    );
    assert!(!label(&host(None, None)), "no player, no route");
    let p = profile_named("fake-amp");
    let name = info("Fake amp");
    let view = host(Some(track_view()), Some(route("Schiit Fake amp")));
    let s = Snapshot {
        host: &view,
        ..shot(&name, &p, &[])
    };
    assert!(
        playing(&s, Family::Amp).expect("a card").sink_label,
        "an amp page"
    );
}

/// The label is part of what the part says, so a change of sink sends the part again.
#[test]
fn the_key_holds_the_sink_label() {
    let ours = host(Some(track_view()), Some(route("Schiit Gungnir 2")));
    let other = host(Some(track_view()), Some(route("Schiit Bifrost 3")));
    let key = |view: &HostView| card_on(view, Some("2"), OPEN).expect("a card").key();
    assert!(key(&ours).contains("sink_label=false"));
    assert!(key(&other).contains("sink_label=true"));
}

/// A hop's unread mark is something a viewer sees (Signal's DAC box), so it is in the key.
#[test]
fn the_key_holds_a_hops_unknown_mark() {
    let view = host(Some(track_view()), Some(route("Schiit Gungnir 2")));
    let mut card = card_on(&view, Some("2"), OPEN).expect("a card");
    let first = card.key();
    let hop = &mut card.path.as_mut().expect("a path").hops[2];
    hop.unknown = !hop.unknown;
    assert_ne!(card.key(), first);
}

/// Only a signed-in viewer with writes on gets the transport buttons.
#[test]
fn the_buttons_follow_what_the_viewer_may_do() {
    let view = host(Some(track_view()), None);
    let buttons = |access| card_on(&view, None, access).expect("a card").buttons;
    assert!(buttons(OPEN));
    assert!(!buttons(Access {
        writes: false,
        guest: true
    }));
    assert!(!buttons(Access {
        writes: false,
        guest: false
    }));
}

/// The key leaves out the clock and the position's read, which change with every look, and
/// holds everything a viewer sees: the status, the cover, the path.
#[test]
fn the_key_is_what_the_card_says_not_when() {
    let mut view = host(Some(track_view()), Some(route("Schiit Gungnir 2")));
    let key = |view: &HostView| card_on(view, Some("2"), OPEN).expect("a card").key();
    let first = key(&view);
    view.now_ms += 5000;
    assert_eq!(key(&view), first, "the clock");
    view.playing.as_mut().unwrap().status = "paused".into();
    let paused = key(&view);
    assert_ne!(paused, first, "the status");
    view.playing.as_mut().unwrap().cover = None;
    assert_ne!(key(&view), paused, "the cover");
    let seeked = |view: &mut HostView, to: u64| {
        let progress = view.playing.as_mut().unwrap().progress.as_mut().unwrap();
        progress.position_us = to;
        progress.read_at_ms += 1;
    };
    let before = key(&view);
    seeked(&mut view, 90_000_000);
    assert_ne!(key(&view), before, "a seek");
    view.playing.as_mut().unwrap().title = "Another".into();
    assert!(key(&view).contains("Another"), "the title");
    view.playing.as_mut().unwrap().length_us = None;
    assert!(!key(&view).contains("1200000000"), "the length");
    view.route.as_mut().unwrap().others_mixing = 2;
    assert!(
        card_on(&view, Some("2"), OPEN)
            .unwrap()
            .key()
            .contains("other sounds mixing")
    );
}

/// An Eitr 2 has no input choice and takes USB alone, so its card reads `untouched` once the
/// rate matches; a Gungnir 2 whose input is not read does not.
#[test]
fn a_dac_with_no_input_control_reads_untouched_on_the_page() {
    let p = profile_named("eitr2");
    let name = info("Eitr 2");
    let readings = [read(&p, "sample_rate", "2")];
    let view = host(Some(track_view()), Some(route("Eitr 2")));
    let s = Snapshot {
        host: &view,
        ..shot(&name, &p, &readings)
    };
    let path = playing(&s, Family::Dac)
        .and_then(|card| card.path)
        .expect("a path");
    assert_eq!((path.class, path.words.as_str()), ("ok", "untouched"));
}
