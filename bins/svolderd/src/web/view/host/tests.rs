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
        stages: None,
        setup: None,
    }
}

/// A host that is available with `playing` and `route`.
fn host(playing: Option<PlayingView>, route: Option<RouteFacts>) -> HostView {
    HostView {
        available: true,
        playing,
        route,
        now_ms: 1_700_000_003_000,
        volume: None,
        eq: None,
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
    assert_eq!(
        (path.verdict, path.words.join(" · ").as_str()),
        ("untouched", "untouched")
    );
    assert!(
        path.title.contains("may still change the sound itself"),
        "{}",
        path.title
    );
    let labels: Vec<_> = path.hops.iter().map(|hop| hop.label.as_str()).collect();
    assert_eq!(labels, ["player", "computer", "DAC"]);
    assert!(path.hops.iter().all(|hop| !hop.changes), "{path:?}");
    assert!(path.hops[2].detail.contains("48 kHz"), "{:?}", path.hops[2]);
}

/// The device's own volume below 0 dB reaches the card: the DAC hop names it and the title says
/// what the device then does; at 0 dB neither says anything of it.
#[test]
fn the_card_names_what_the_dac_itself_changes() {
    let view = host(Some(track_view()), Some(route("Schiit Gungnir 2")));
    let p = profile_named("gungnir2-v2");
    let name = info("Gungnir 2");
    let card = |volume: &str, connected: bool| {
        let mut name = name.clone();
        name.connected = connected;
        let readings = [
            read(&p, "input", "1"),
            read(&p, "sample_rate", "2"),
            read(&p, "usb1_connected", "true"),
            read(&p, "usb2_connected", "false"),
            read(&p, "volume", volume),
        ];
        let s = Snapshot {
            host: &view,
            access: OPEN,
            ..shot(&name, &p, &readings)
        };
        playing(&s, Family::Dac)
            .and_then(|card| card.path)
            .expect("a path")
    };
    // A link that is down claims nothing of the device's own stage from the readings it holds.
    let path = card("117", false);
    assert!(!path.title.contains("then"), "{}", path.title);
    assert!(!path.hops[2].changes);
    let path = card("117", true);
    assert_eq!(path.verdict, "untouched");
    assert!(
        path.title
            .contains("the Gungnir 2 then lowers it by 3.3 dB"),
        "{}",
        path.title
    );
    assert!(path.hops[2].changes);
    assert!(
        path.hops[2].detail.ends_with("volume \u{2212}3.3 dB"),
        "{:?}",
        path.hops[2]
    );
    let path = card("127", true);
    assert!(!path.title.contains("then"), "{}", path.title);
    assert!(!path.hops[2].changes);
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
        (path.verdict, path.words.join(" · ").as_str()),
        ("changed", "DAC reads 44.1 kHz")
    );
    assert!(path.hops[2].changes);
    let path = card_on(&view, None, OPEN)
        .and_then(|card| card.path)
        .expect("a path");
    assert_eq!(
        (path.verdict, path.words.join(" · ").as_str()),
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
        (path.verdict, path.words.join(" · ").as_str()),
        ("other_input", "the DAC listens to Optical")
    );
    assert!(path.hops.is_empty());
    let path = card_with(&view, Some("USB 2"), Some("2"), OPEN)
        .and_then(|card| card.path)
        .expect("a path");
    assert_eq!(
        (path.verdict, path.words.join(" · ").as_str()),
        ("untouched", "untouched")
    );
    let path = card_with(&view, None, Some("2"), OPEN)
        .and_then(|card| card.path)
        .expect("a path");
    assert_eq!(
        (path.verdict, path.words.join(" · ").as_str()),
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
        (path.verdict, path.words.join(" · ").as_str()),
        (
            "other_input",
            "the DAC listens to USB 1 · the computer is on USB 2"
        )
    );
    assert!(path.hops.is_empty());
    let path = card_over_ports(&view, Some("USB 1"), Some("2"), ("true", "true"), OPEN)
        .and_then(|card| card.path)
        .expect("a path");
    assert_eq!(
        (path.verdict, path.words.join(" · ").as_str()),
        ("unknown", "both USB ports connected")
    );
}

/// A player routed to another sink says `player → NAME`, and its title the whole sentence.
#[test]
fn a_player_routed_elsewhere_says_so() {
    let mut facts = route("Schiit Gungnir 2");
    facts.player_match = PlayerMatch::Elsewhere {
        sink: "LG TV".into(),
    };
    let view = host(Some(track_view()), Some(facts));
    let path = card_on(&view, Some("2"), OPEN)
        .and_then(|card| card.path)
        .expect("a path");
    assert_eq!(path.verdict, "not_this_dac");
    assert_eq!(path.words.join(" · "), "player \u{2192} LG TV");
    assert_eq!(path.title, "the player plays to LG TV");
    assert!(path.hops.is_empty());
}

/// A computer that plays to another sink gives short words, the whole sentence as the title, and no hops.
#[test]
fn a_sound_that_goes_elsewhere_says_so() {
    let view = host(Some(track_view()), Some(route("Schiit Bifrost 3")));
    let path = card_on(&view, Some("2"), OPEN)
        .and_then(|card| card.path)
        .expect("a path");
    assert_eq!(path.verdict, "not_this_dac");
    assert_eq!(path.words.join(" · "), "→ Schiit Bifrost 3");
    assert_eq!(path.title, "the computer plays to Schiit Bifrost 3");
    assert!(path.hops.is_empty());
}

/// No route read yet says so, as the TUI's row does; no track to follow, no path line.
#[test]
fn no_route_says_so_and_no_track_has_no_path() {
    let unread = card_on(&host(Some(track_view()), None), Some("2"), OPEN).expect("a card");
    let path = unread.path.expect("a path");
    assert_eq!(
        (path.verdict, path.words.join(" · ").as_str()),
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
    assert_eq!(
        (path.verdict, path.words.join(" · ").as_str()),
        ("untouched", "untouched")
    );
}

/// The link pill says the readings are old while the link is down and, once it is back, until the
/// daemon has heard the device again (the stale flag), and not otherwise.
#[test]
fn the_link_says_when_the_readings_are_from_before_a_drop() {
    let mut device = info("Gungnir 2");
    assert!(!crate::web::view::link_of(&device).old);
    device.stale = true;
    assert!(
        crate::web::view::link_of(&device).old,
        "back but not heard again"
    );
    device.connected = false;
    assert!(crate::web::view::link_of(&device).old, "down");
    device.stale = false;
    assert!(
        crate::web::view::link_of(&device).old,
        "down is old whatever the flag"
    );
}

/// The config facts of a sink that lets every line through: a USB DAC on `hw:Audio` taking
/// 44.1 to 384 kHz in 32, 24 and 16 bits, every rate allowed, no dither and no filter.
fn passing_setup() -> svolder_host::SetupFacts {
    use svolder_host::{AlsaFacts, ClockFacts, Dither, Layout, Rates, SetupFacts, SinkOffer};
    let stereo = Layout {
        channels: 2,
        positions: vec!["FL".into(), "FR".into()],
    };
    SetupFacts {
        offer: Some(SinkOffer {
            formats: vec!["S32LE".into(), "S24LE".into(), "S16LE".into()],
            rates: Some(Rates::Range {
                min: 44_100,
                max: 384_000,
            }),
            layout: Some(stereo.clone()),
        }),
        clock: Some(ClockFacts {
            rate: Some(48_000),
            allowed_rates: vec![
                44_100, 48_000, 88_200, 96_000, 176_400, 192_000, 352_800, 384_000,
            ],
            force_rate: None,
        }),
        sink_layout: Some(stereo.clone()),
        stream_layout: Some(stereo),
        dither: Some(Dither {
            method: "none".into(),
            noise: 0,
        }),
        sink_filter: Some(false),
        stream_filter: Some(false),
        alsa: Some(AlsaFacts {
            path: Some("hw:Audio".into()),
            ..AlsaFacts::default()
        }),
        ..SetupFacts::default()
    }
}

/// A route on the DAC's own sink with `setup` on it.
fn route_with(setup: Option<svolder_host::SetupFacts>) -> RouteFacts {
    RouteFacts {
        setup,
        ..route("Schiit Gungnir 2")
    }
}

/// The card of a Gungnir 2 whose route carries `setup`.
fn setup_card(setup: Option<svolder_host::SetupFacts>) -> Playing {
    let view = host(Some(track_view()), Some(route_with(setup)));
    card_on(&view, Some("2"), OPEN).expect("a card")
}

/// A DAC's card carries the config check of its sink: the class, the chip, the summary and the
/// seven lines in the judge's words.
#[test]
fn the_path_carries_the_config_check_while_the_sink_is_the_dac() {
    let card = setup_card(Some(passing_setup()));
    let setup = card.path.expect("a path").setup.expect("a check");
    assert_eq!(setup.outcome, "untouched");
    assert_eq!(setup.limit, svolder_ipc::host::SETUP_LIMIT);
    assert_eq!(setup.chip, "config ✓");
    assert_eq!(setup.summary, "PipeWire config lets it through untouched");
    assert_eq!(setup.lines.len(), 7);
    let keys: Vec<_> = setup.lines.iter().map(|line| line.key.as_str()).collect();
    assert_eq!(
        keys,
        [
            "rates",
            "forced_rate",
            "dither",
            "format",
            "plug",
            "stereo",
            "filter"
        ]
    );
    assert!(setup.lines.iter().all(|line| line.outcome == "pass"));
    assert_eq!(setup.lines[3].label, "24-bit output");
    assert_eq!(setup.lines[3].words, "S32LE in use");

    let mut facts = passing_setup();
    facts.dither.as_mut().expect("dither").method = "triangular".into();
    let setup = setup_card(Some(facts))
        .path
        .expect("a path")
        .setup
        .expect("a check");
    assert_eq!(setup.outcome, "changes");
    assert_eq!(setup.chip, "config 1");
    assert_eq!(setup.summary, "PipeWire config changes the sound: 1");
    assert_eq!(setup.lines[2].outcome, "fail");
    assert_eq!(setup.lines[2].words, "triangular");

    let mut facts = passing_setup();
    facts.sink_filter = None;
    let setup = setup_card(Some(facts))
        .path
        .expect("a path")
        .setup
        .expect("a check");
    assert_eq!((setup.outcome, setup.chip.as_str()), ("unread", "config ?"));
    assert_eq!(setup.lines[6].outcome, "unknown");
}

/// A route from an older daemon (no config facts) says so, never a clean check.
#[test]
fn an_older_daemons_route_has_an_unread_config() {
    let setup = setup_card(None)
        .path
        .expect("a path")
        .setup
        .expect("a check");
    assert_eq!(setup.outcome, "other_version");
    assert_eq!(
        setup.summary,
        "PipeWire config not read (svolderd's version differs)"
    );
}

/// The check is the DAC's only while the sink is the DAC: another sink gets none, and so does a
/// card with no route yet.
#[test]
fn another_sink_or_no_route_has_no_config_check() {
    let other = host(Some(track_view()), Some(route_with(Some(passing_setup()))));
    let mut other = other;
    let facts = other.route.as_mut().expect("a route");
    let sink = facts.sink.as_mut().expect("a sink");
    sink.usb_product = Some("Schiit Bifrost 3".into());
    sink.description = "Schiit Bifrost 3".into();
    let card = card_on(&other, Some("2"), OPEN).expect("a card");
    assert!(card.path.expect("a path").setup.is_none());

    let none = card_on(&host(Some(track_view()), None), Some("2"), OPEN).expect("a card");
    assert!(none.path.expect("a path").setup.is_none());
}

/// A line of the check that changes is something a viewer sees, so the card key changes with it.
#[test]
fn the_key_holds_the_config_check() {
    let mut card = setup_card(Some(passing_setup()));
    let first = card.key();
    let setup = card
        .path
        .as_mut()
        .and_then(|path| path.setup.as_mut())
        .expect("a check");
    setup.lines[0].words.push('!');
    let words = card.key();
    assert_ne!(words, first, "a line's words");
    let setup = card
        .path
        .as_mut()
        .and_then(|path| path.setup.as_mut())
        .expect("a check");
    setup.lines[0].outcome = "fail";
    assert_ne!(card.key(), words, "a line's outcome");
    card.path.as_mut().expect("a path").setup = None;
    assert_ne!(card.key(), first, "the check itself");
}

/// The JSON the page reads: `playing.path.config` with the outcome, the chip, the summary, the
/// limit, the sink and the lines, and no `config` key at all when there is no check.
#[test]
fn the_models_json_carries_the_config_check_or_none() {
    use crate::web::model::PlayingModel;
    let card = setup_card(Some(passing_setup()));
    let json = serde_json::to_value(PlayingModel::from(&card)).expect("json");
    let setup = &json["path"]["config"];
    assert!(
        json["path"]["setup"].is_null(),
        "the key is `config`, as on every surface"
    );
    assert_eq!(setup["outcome"], "untouched");
    assert_eq!(setup["limit"], svolder_ipc::host::SETUP_LIMIT);
    assert!(setup["class"].is_null(), "the old key is gone");
    assert_eq!(json["path"]["verdict"], "untouched");
    assert_eq!(json["path"]["words"], serde_json::json!(["untouched"]));
    assert_eq!(setup["chip"], "config ✓");
    assert_eq!(
        setup["summary"],
        "PipeWire config lets it through untouched"
    );
    assert_eq!(setup["sink"], "Schiit Gungnir 2");
    assert_eq!(setup["lines"].as_array().map(Vec::len), Some(7));
    assert_eq!(
        setup["lines"][3],
        serde_json::json!({
            "key": "format",
            "label": "24-bit output",
            "outcome": "pass",
            "words": "S32LE in use"
        })
    );

    let none = host(Some(track_view()), None);
    let card = card_on(&none, Some("2"), OPEN).expect("a card");
    let json = serde_json::to_value(PlayingModel::from(&card)).expect("json");
    assert!(json["path"].get("config").is_none(), "{json}");
}

/// The themes' CSS and JS hold literal copies of the judge's keys (a chip's class, a dot, the
/// verdicts that say the sound goes elsewhere). Each key a theme styles or compares must still be
/// the judge's, so a renamed key fails here rather than leaving a chip unstyled.
#[test]
fn the_themes_spell_the_judges_keys() {
    use svolder_ipc::host::{SetupSummary, Tone, Verdict};
    let ember = include_str!("../../../../web/themes/ember/style.css");
    let signal = include_str!("../../../../web/themes/signal/style.css");
    let paper = include_str!("../../../../web/themes/paper/style.css");
    let wire = include_str!("../../../../web/core/popups/devices/wire.js");
    let popup = include_str!("../../../../web/core/popups/setup/draw.js");
    let signal_js = include_str!("../../../../web/themes/signal/draw/player.js");
    let paper_js = include_str!("../../../../web/themes/paper/notes.js");
    // Every chip outcome has a rule on the chip's own selector: `.np-setup` in Ember,
    // `.sg-config` in Signal, `.p-setup` in Paper (plain for `untouched`), written as
    // `<chip>.<key>` or inside a `<chip>:is(.a, .b)` list. A key's name elsewhere in the sheet
    // does not count, so deleting a chip rule fails here.
    let styled = |css: &str, chip: &str, key: &str| {
        css.contains(&format!("{chip}.{key}"))
            || css.match_indices(&format!("{chip}:is(")).any(|(at, _)| {
                let list = &css[at..];
                let list = &list[..list.find(')').unwrap_or(list.len())];
                list.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.'))
                    .any(|part| part == format!(".{key}"))
            })
    };
    for key in SetupSummary::KEYS {
        assert!(
            styled(signal, ".sg-config", key),
            "signal styles no `.sg-config.{key}`"
        );
        if key != "untouched" {
            assert!(
                styled(paper, ".p-setup", key),
                "paper styles no `.p-setup.{key}`"
            );
        }
    }
    // Ember colours the label by the row's tone (`good`, `warn`) and styles the config chip it
    // draws only while the check fails.
    for tone in [Tone::Good, Tone::Warn] {
        let key = serde_json::to_value(tone).expect("tone");
        let key = key.as_str().expect("a word");
        assert!(
            ember.contains(&format!(".np-label.{key}")),
            "ember `.np-label.{key}`"
        );
    }
    assert!(ember.contains(".np-setup {"), "ember `.np-setup`");
    // The devices popup treats these verdicts as a sound that goes elsewhere.
    for key in ["not_this_dac", "other_input"] {
        assert!(Verdict::KEYS.contains(&key));
        assert!(wire.contains(&format!("'{key}'")), "wire.js `{key}`");
    }
    // The config popup's dots and the themes' comparisons.
    for key in ["untouched", "changes"] {
        assert!(SetupSummary::KEYS.contains(&key));
        assert!(
            popup.contains(&format!("{key}: '")),
            "setup/draw.js dot for `{key}`"
        );
    }
    assert!(signal_js.contains("=== 'untouched'") && paper_js.contains("=== 'changed'"));
}

/// The row the page draws: an untouched path and one whose DAC changes the sound, in the JSON
/// the page reads (`playing.path.row` and `playing.path.end_to_end`), and no front end has
/// anything left to judge.
#[test]
fn the_models_json_carries_the_row_and_end_to_end() {
    use crate::web::model::PlayingModel;
    let json_of = |card: &Playing| serde_json::to_value(PlayingModel::from(card)).expect("json");

    let card = setup_card(Some(passing_setup()));
    let json = json_of(&card);
    assert_eq!(json["path"]["end_to_end"], "untouched");
    assert_eq!(json["path"]["verdict"], "untouched");
    assert_eq!(
        json["path"]["row"],
        serde_json::json!({
            "label": {"tone": "good", "words": "Path: untouched end to end"},
            "hops": [
                {"label": "player", "words": []},
                {"label": "computer", "words": []},
                {"label": "DAC USB 1", "words": []},
            ],
            "values": [{"text": "48 kHz", "tone": "dim"}],
            "words": null,
        })
    );

    // The device's own volume below 0 dB: the verdict stays untouched (to the DAC's input),
    // end to end it is changed, and the label and the values say so.
    let view = host(Some(track_view()), Some(route("Schiit Gungnir 2")));
    let p = profile_named("gungnir2-v2");
    let name = info("Gungnir 2");
    let readings = [
        read(&p, "input", "1"),
        read(&p, "sample_rate", "2"),
        read(&p, "usb1_connected", "true"),
        read(&p, "usb2_connected", "false"),
        read(&p, "volume", "117"),
    ];
    let s = Snapshot {
        host: &view,
        access: OPEN,
        ..shot(&name, &p, &readings)
    };
    let card = playing(&s, Family::Dac).expect("a card");
    let json = json_of(&card);
    assert_eq!(json["path"]["verdict"], "untouched");
    assert_eq!(json["path"]["end_to_end"], "changed");
    let row = &json["path"]["row"];
    assert_eq!(
        row["label"],
        serde_json::json!({"tone": "warn", "words": "Path: the DAC changes the sound"})
    );
    let values = row["values"].as_array().expect("values");
    assert!(
        values.contains(&serde_json::json!({"text": "volume -3.3 dB", "tone": "warn"})),
        "{values:?}"
    );
}

/// A sound that goes elsewhere, and a route not read yet, are rows with no hops and their words.
#[test]
fn a_row_with_no_hops_carries_the_short_words() {
    use crate::web::model::PlayingModel;
    let view = host(Some(track_view()), Some(route("DELL S2721QS")));
    let card = card_on(&view, Some("2"), OPEN).expect("a card");
    let json = serde_json::to_value(PlayingModel::from(&card)).expect("json");
    assert_eq!(json["path"]["end_to_end"], "not_this_dac");
    assert_eq!(json["path"]["row"]["hops"], serde_json::json!([]));
    assert_eq!(json["path"]["row"]["label"]["tone"], "plain");
    assert_eq!(json["path"]["row"]["words"], "→ DELL S2721QS");

    let none = host(Some(track_view()), None);
    let card = card_on(&none, Some("2"), OPEN).expect("a card");
    let json = serde_json::to_value(PlayingModel::from(&card)).expect("json");
    assert_eq!(json["path"]["end_to_end"], "unknown");
    assert_eq!(json["path"]["row"]["words"], "route not read");
}

/// What the row says is something a viewer sees, so it is in the key.
#[test]
fn the_key_holds_the_row() {
    let view = host(Some(track_view()), Some(route("Schiit Gungnir 2")));
    let mut card = card_on(&view, Some("2"), OPEN).expect("a card");
    let first = card.key();
    card.path.as_mut().expect("a path").row.values.clear();
    assert_ne!(card.key(), first);
    let mut card = card_on(&view, Some("2"), OPEN).expect("a card");
    card.path.as_mut().expect("a path").end_to_end = "changed";
    assert_ne!(card.key(), first);
}
