// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::host::path::tests::clean;

#[test]
fn a_view_without_what_it_lacks_is_a_short_line() {
    let line = serde_json::to_string(&HostView::default()).unwrap();
    assert_eq!(line, r#"{"available":false,"now_ms":0}"#);
    let back: HostView = serde_json::from_str("{}").unwrap();
    assert_eq!(back, HostView::default());
}

#[test]
fn a_view_round_trips() {
    let view = HostView {
        available: true,
        playing: Some(PlayingView {
            title: "Song".into(),
            artist: "Artist".into(),
            album: "Album".into(),
            player: "Spotify".into(),
            status: "playing".into(),
            length_us: Some(200_000_000),
            progress: Some(Progress {
                position_us: 5_000_000,
                rate_milli: 1000,
                read_at_ms: 99,
            }),
            cover: Some("0123456789abcdef".into()),
        }),
        route: Some(clean()),
        now_ms: 1_700_000_000_000,
        volume: None,
        eq: None,
    };
    let back: HostView = serde_json::from_str(&serde_json::to_string(&view).unwrap()).unwrap();
    assert_eq!(back, view);
}

/// A frame survives the trip through its wire form, and a wire frame whose bands are not
/// 240 a channel is no frame.
#[test]
fn a_frame_view_makes_the_frame_it_came_from() {
    let mut frame = svolder_host::Frame {
        capture_ms: 1_700_000_000_123,
        left: [0; 240],
        right: [0; 240],
        levels: [1, 2, 3, 4],
        beat: true,
        bpm: Some(118),
        phase: 200,
    };
    frame.left[0] = 255;
    frame.right[239] = 17;
    let view = FrameView::from(&frame);
    assert_eq!(
        (view.t, view.left.len(), view.right.len()),
        (frame.capture_ms, 240, 240)
    );
    assert_eq!(view.to_frame(), Some(frame));
    let short = FrameView {
        left: vec![0; 239],
        ..view
    };
    assert_eq!(short.to_frame(), None);
}

/// A daemon from before the cover hash sends a track with no `cover`, and a front end from
/// before it drops the field it does not know: both read as no art.
#[test]
fn a_track_with_no_cover_field_is_a_track_with_no_art() {
    let old: PlayingView = serde_json::from_str(r#"{"title":"Song","status":"playing"}"#).unwrap();
    assert_eq!(old.cover, None);
    let line = serde_json::to_string(&old).unwrap();
    assert!(!line.contains("cover"), "{line}");
    let new = PlayingView {
        cover: Some("0123456789abcdef".into()),
        ..PlayingView::default()
    };
    let wire = serde_json::to_string(&new).unwrap();
    assert!(wire.contains(r#""cover":"0123456789abcdef""#), "{wire}");
}
