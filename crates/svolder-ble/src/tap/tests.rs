// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The frame tap over a session on the fake transport.

use super::*;
use crate::advert::Advertisement;
use crate::fake::FakeTransport;
use crate::session::Session;
use svolder_profiles::Registry;
use svolder_proto::{Passphrase, UnitId, decode_command, decode_notification};

/// A fake Gungnir 2 and the advertisement it sends.
fn fake() -> (Arc<FakeTransport>, Advertisement, svolder_profiles::Profile) {
    let profile = Registry::builtin()
        .unwrap()
        .get("gungnir2-v2")
        .unwrap()
        .clone();
    let unit = "0102030405060708".parse::<UnitId>().unwrap();
    let transport = Arc::new(FakeTransport::from_profile(
        &profile,
        unit,
        3,
        Passphrase::NONE,
    ));
    let advert = transport.advertisement();
    (transport, advert, profile)
}

/// A session over `transport`, wrapped in the tap when there is one.
fn session(
    transport: &Arc<FakeTransport>,
    advert: Advertisement,
    profile: svolder_profiles::Profile,
    tap: Option<FrameTap>,
) -> Session {
    let over: Arc<dyn Transport> = match tap {
        Some(tap) => Arc::new(TappedTransport::new(transport.clone(), tap)),
        None => transport.clone(),
    };
    Session::new(over, advert, profile, Passphrase::NONE)
}

#[tokio::test(start_paused = true)]
async fn a_tapped_session_records_what_it_sends_and_hears() {
    let (transport, advert, profile) = fake();
    let tap = FrameTap::new();
    let mut s = session(&transport, advert, profile, Some(tap.clone()));
    s.connect().await.unwrap();
    s.refresh().await.unwrap();
    let frames = tap.frames();
    assert_eq!(
        frames.first().map(|f| f.direction),
        Some(FrameDirection::Sent)
    );
    assert!(
        frames
            .iter()
            .any(|f| f.direction == FrameDirection::Sent && decode_command(&f.bytes).is_ok())
    );
    assert!(
        frames
            .iter()
            .any(|f| f.direction == FrameDirection::Received
                && decode_notification(&f.bytes).is_ok())
    );
    assert!(frames.windows(2).all(|w| w[0].at <= w[1].at));
}

#[tokio::test(start_paused = true)]
async fn the_tap_follows_a_reconnect() {
    let (transport, advert, profile) = fake();
    let tap = FrameTap::new();
    let mut s = session(&transport, advert, profile, Some(tap.clone()));
    s.connect().await.unwrap();
    s.refresh().await.unwrap();
    let before = tap.frames().len();
    transport.sever();
    s.connect().await.unwrap();
    s.refresh().await.unwrap();
    let frames = tap.frames();
    assert!(frames.len() > before);
    assert!(
        frames[before..]
            .iter()
            .any(|f| f.direction == FrameDirection::Sent)
    );
}

#[tokio::test(start_paused = true)]
async fn an_untapped_transport_is_unchanged() {
    let (transport, advert, profile) = fake();
    let mut bare = session(&transport, advert.clone(), profile.clone(), None);
    bare.connect().await.unwrap();
    bare.refresh().await.unwrap();
    let bare_state: Vec<_> = bare.state().messages().cloned().collect();
    bare.close().await;

    let (transport, advert, profile) = fake();
    let mut tapped = session(&transport, advert, profile, Some(FrameTap::new()));
    tapped.connect().await.unwrap();
    tapped.refresh().await.unwrap();
    let tapped_state: Vec<_> = tapped.state().messages().cloned().collect();
    assert!(!bare_state.is_empty());
    assert_eq!(bare_state, tapped_state);
}

/// Frames noted from several threads at once are in the vector in the order of their times.
#[test]
fn frames_noted_from_many_threads_are_ordered_by_time() {
    let tap = FrameTap::new();
    let threads: Vec<_> = (0..8)
        .map(|_| {
            let tap = tap.clone();
            std::thread::spawn(move || {
                for _ in 0..2_000 {
                    tap.record(FrameDirection::Received, &[1]);
                }
            })
        })
        .collect();
    for thread in threads {
        thread.join().expect("a thread ends");
    }
    let frames = tap.frames();
    assert_eq!(frames.len(), 16_000);
    assert!(frames.windows(2).all(|w| w[0].at <= w[1].at));
}
