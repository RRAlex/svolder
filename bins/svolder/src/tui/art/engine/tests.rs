// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The cover's way from the daemon to a picture, against a stub daemon that serves covers, with
//! half-blocks (and sixel, for the clear) as the terminal.

use std::io::Cursor;
use std::time::Duration;

use image::{DynamicImage, ImageFormat, RgbImage};
use ratatui::layout::Rect;
use ratatui_image::picker::ProtocolType;
use svolder_host::cover::Cover;

use super::*;
use crate::test_support::stub_daemon::stub_daemon;
use crate::tui::app::Screen;
use crate::tui::test_support::app_showing;

/// The frame the picture is sized for.
const SMALL: Rect = Rect::new(0, 0, 80, 24);
const BIG: Rect = Rect::new(0, 0, 126, 40);

/// A picture of `width` by `height` pixels, encoded as `format`.
fn encoded(width: u32, height: u32, format: ImageFormat) -> Vec<u8> {
    let image = DynamicImage::ImageRgb8(RgbImage::from_fn(width, height, |x, y| {
        image::Rgb([(x % 251) as u8, (y % 241) as u8, ((x + y) % 239) as u8])
    }));
    let mut bytes = Cursor::new(Vec::new());
    image.write_to(&mut bytes, format).expect("encode");
    bytes.into_inner()
}

fn png() -> Cover {
    Cover {
        bytes: encoded(96, 96, ImageFormat::Png),
        mime: "image/png",
    }
}

/// An app on the device screen hearing a player on the daemon's bus, whose track has the cover
/// `hash` (or none).
fn playing(hash: Option<&str>) -> App {
    let mut app = app_showing(Screen::Device);
    app.host.linked = true;
    app.host.found_bus(
        Some(svolder_host::NowPlaying {
            title: "So What".into(),
            artist: "Miles Davis".into(),
            ..Default::default()
        }),
        app.now,
    );
    app.host.cover.hash = hash.map(str::to_string);
    app
}

/// Wait for the step under way to finish, and take what it brought into `app`.
async fn finish_a_step(art: &mut Art, app: &mut App) {
    let done = tokio::time::timeout(Duration::from_secs(20), art.next())
        .await
        .expect("a step finishes");
    art.fold(done, app);
}

/// Steer and fold until nothing is under way any more.
async fn settle(art: &mut Art, app: &mut App, backend: Option<&Backend>, area: Rect) {
    for _ in 0..8 {
        art.steer(app, backend, area);
        if !art.busy() {
            return;
        }
        finish_a_step(art, app).await;
    }
    panic!("art is still busy after eight steps");
}

fn halfblocks() -> Art {
    Art::with(Some(Picker::halfblocks()))
}

#[tokio::test]
async fn a_cover_goes_from_the_daemon_to_a_picture_in_steps() {
    let daemon = stub_daemon().await;
    daemon.serves_cover("abc", png());
    let backend = Backend::stub_remote(&daemon.socket).await;
    let (mut art, mut app) = (halfblocks(), playing(Some("abc")));

    art.steer(&mut app, Some(&backend), SMALL);
    assert!(
        app.host.cover.picture.is_none(),
        "nothing yet: the bytes are on their way"
    );
    assert_eq!(
        app.host.cover.font,
        Some((10, 20)),
        "the layout is told the cell size"
    );
    settle(&mut art, &mut app, Some(&backend), SMALL).await;

    let picture = app.host.cover.picture.as_ref().expect("a picture");
    assert_eq!(picture.hash, "abc");
    let most = art_box(SMALL, &app).expect("a box");
    assert!(picture.size().width <= most.width && picture.size().height <= most.height);
    assert!(picture.size().width > 0);
    assert_eq!(
        daemon.host_cover_asks(),
        [0],
        "a small cover comes whole in one ask"
    );
}

#[tokio::test]
async fn nothing_is_awaited_by_the_caller_the_steps_run_on_their_own() {
    // `steer` returns at once with the fetch spawned: a daemon that never answers holds nothing.
    let daemon = stub_daemon().await;
    daemon.serves_cover("abc", png());
    let backend = Backend::stub_remote(&daemon.socket).await;
    let (mut art, mut app) = (halfblocks(), playing(Some("abc")));
    let began = std::time::Instant::now();
    art.steer(&mut app, Some(&backend), SMALL);
    assert!(art.busy());
    assert!(began.elapsed() < Duration::from_millis(500));
}

#[tokio::test]
async fn a_track_change_drops_the_old_picture_at_once_and_a_late_answer_for_it() {
    let daemon = stub_daemon().await;
    daemon.serves_cover("abc", png());
    let backend = Backend::stub_remote(&daemon.socket).await;
    let (mut art, mut app) = (halfblocks(), playing(Some("abc")));
    settle(&mut art, &mut app, Some(&backend), SMALL).await;
    assert!(app.host.cover.picture.is_some());

    app.host.cover.hash = Some("def".into());
    daemon.serves_cover("def", png());
    art.steer(&mut app, Some(&backend), SMALL);
    assert!(
        app.host.cover.picture.is_none(),
        "the old track's picture is gone at once"
    );
    assert!(art.busy(), "the new cover is on its way");

    // What the old track's steps bring now is not kept.
    art.fold(
        Done::Fetched {
            hash: "abc".into(),
            cover: Some(png()),
        },
        &mut app,
    );
    art.fold(
        Done::Built {
            hash: "abc".into(),
            size: art_box(SMALL, &app).expect("a box"),
            protocol: build(
                &Picker::halfblocks(),
                DynamicImage::ImageRgb8(RgbImage::new(8, 8)),
                art_box(SMALL, &app).expect("a box"),
            ),
        },
        &mut app,
    );
    assert!(
        app.host.cover.picture.is_none(),
        "a stale picture was shown"
    );
    settle(&mut art, &mut app, Some(&backend), SMALL).await;
    assert_eq!(
        app.host.cover.picture.as_ref().map(|p| p.hash.as_str()),
        Some("def")
    );
}

#[tokio::test]
async fn a_track_change_aborts_the_fetch_of_the_old_cover() {
    let daemon = stub_daemon().await;
    daemon.serves_cover("abc", png());
    let backend = Backend::stub_remote(&daemon.socket).await;
    let (mut art, mut app) = (halfblocks(), playing(Some("abc")));
    art.steer(&mut app, Some(&backend), SMALL);
    assert!(art.fetching.as_ref().is_some_and(|(hash, _)| hash == "abc"));
    app.host.cover.hash = Some("def".into());
    art.steer(&mut app, Some(&backend), SMALL);
    assert!(
        art.fetching.as_ref().is_some_and(|(hash, _)| hash == "def"),
        "one fetch at a time, for the track playing"
    );
}

#[tokio::test]
async fn a_cover_with_no_bytes_is_asked_for_once_and_the_row_stays_as_it_was() {
    let daemon = stub_daemon().await;
    let backend = Backend::stub_remote(&daemon.socket).await;
    let (mut art, mut app) = (halfblocks(), playing(Some("abc")));
    settle(&mut art, &mut app, Some(&backend), SMALL).await;
    assert!(app.host.cover.picture.is_none());
    for _ in 0..3 {
        art.steer(&mut app, Some(&backend), SMALL);
    }
    assert!(!art.busy());
    assert_eq!(daemon.host_cover_asks(), [0], "asked once, then remembered");
}

#[tokio::test]
async fn a_daemon_without_host_cover_is_not_asked_at_all() {
    let daemon = stub_daemon().await;
    daemon.serves_cover("abc", png());
    let backend = Backend::stub_remote(&daemon.socket).await;
    let (mut art, mut app) = (halfblocks(), playing(Some("abc")));
    art.daemon_serves(false);
    art.steer(&mut app, Some(&backend), SMALL);
    assert!(!art.busy());
    assert_eq!(daemon.host_cover_asks(), Vec::<u64>::new());
}

#[tokio::test]
async fn a_session_with_no_daemon_has_no_art() {
    let (backend, _gate) = Backend::fake(Duration::from_secs(30)).await;
    let (mut art, mut app) = (halfblocks(), playing(Some("abc")));
    art.steer(&mut app, Some(&backend), SMALL);
    assert!(!art.busy());
    art.steer(&mut app, None, SMALL);
    assert!(!art.busy());
}

#[tokio::test]
async fn cover_art_off_draws_nothing_and_asks_nothing() {
    let daemon = stub_daemon().await;
    daemon.serves_cover("abc", png());
    let backend = Backend::stub_remote(&daemon.socket).await;
    let (mut art, mut app) = (halfblocks(), playing(Some("abc")));
    settle(&mut art, &mut app, Some(&backend), SMALL).await;
    assert!(app.host.cover.picture.is_some());

    app.tui.cover_art = CoverArt::Off;
    art.steer(&mut app, Some(&backend), SMALL);
    assert!(
        app.host.cover.picture.is_none(),
        "turned off, the picture goes"
    );
    assert_eq!(
        app.host.cover.font, None,
        "and the layout keeps nothing for it"
    );
    let asks = daemon.host_cover_asks().len();
    art.steer(&mut app, Some(&backend), SMALL);
    assert!(!art.busy());
    assert_eq!(daemon.host_cover_asks().len(), asks);
}

#[tokio::test]
async fn art_turned_on_with_no_probe_to_go_by_draws_half_blocks() {
    let daemon = stub_daemon().await;
    daemon.serves_cover("abc", png());
    let backend = Backend::stub_remote(&daemon.socket).await;
    let (mut art, mut app) = (Art::default(), playing(Some("abc")));
    settle(&mut art, &mut app, Some(&backend), SMALL).await;
    assert!(
        app.host.cover.picture.is_some(),
        "auto draws, with half-blocks"
    );
}

#[tokio::test]
async fn a_cover_that_will_not_decode_is_dropped_and_not_asked_for_again() {
    let daemon = stub_daemon().await;
    // The daemon sniffs the first bytes only: these start like a PNG and are not one.
    let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
    bytes.extend_from_slice(&[7; 300]);
    daemon.serves_cover(
        "abc",
        Cover {
            bytes,
            mime: "image/png",
        },
    );
    let backend = Backend::stub_remote(&daemon.socket).await;
    let (mut art, mut app) = (halfblocks(), playing(Some("abc")));
    settle(&mut art, &mut app, Some(&backend), SMALL).await;
    assert!(app.host.cover.picture.is_none());
    let asks = daemon.host_cover_asks().len();
    for _ in 0..3 {
        art.steer(&mut app, Some(&backend), SMALL);
    }
    assert!(!art.busy());
    assert_eq!(daemon.host_cover_asks().len(), asks);
}

/// Every cover that failed is remembered as one that did, not only the last: a track whose cover
/// failed before and plays again keeps the usual layout, whatever failed in between.
#[tokio::test]
async fn a_track_played_again_whose_cover_failed_is_still_a_failed_cover() {
    let daemon = stub_daemon().await;
    let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
    bytes.extend_from_slice(&[7; 300]);
    for hash in ["abc", "def"] {
        daemon.serves_cover(
            hash,
            Cover {
                bytes: bytes.clone(),
                mime: "image/png",
            },
        );
    }
    let backend = Backend::stub_remote(&daemon.socket).await;
    let (mut art, mut app) = (halfblocks(), playing(Some("abc")));
    settle(&mut art, &mut app, Some(&backend), SMALL).await;
    assert_eq!(app.host.cover.failed.as_deref(), Some("abc"));
    app.host.cover.hash = Some("def".into());
    settle(&mut art, &mut app, Some(&backend), SMALL).await;
    assert_eq!(app.host.cover.failed.as_deref(), Some("def"));
    app.host.cover.hash = Some("abc".into());
    art.steer(&mut app, Some(&backend), SMALL);
    assert_eq!(app.host.cover.failed.as_deref(), Some("abc"));
}

#[tokio::test]
async fn a_resize_sizes_the_picture_again_for_the_new_box() {
    let daemon = stub_daemon().await;
    daemon.serves_cover("abc", png());
    let backend = Backend::stub_remote(&daemon.socket).await;
    let (mut art, mut app) = (halfblocks(), playing(Some("abc")));
    settle(&mut art, &mut app, Some(&backend), SMALL).await;
    let small = app.host.cover.picture.as_ref().expect("a picture").size();

    settle(&mut art, &mut app, Some(&backend), BIG).await;
    let big = app.host.cover.picture.as_ref().expect("a picture").size();
    assert!(
        big.height > small.height,
        "{big:?} is not taller than {small:?}"
    );
    // Nothing is fetched or built again for a frame the picture already fits.
    art.steer(&mut app, Some(&backend), BIG);
    assert!(!art.busy());
    assert_eq!(
        daemon.host_cover_asks(),
        [0],
        "the picture was kept, not fetched again"
    );
}

#[tokio::test]
async fn too_narrow_a_frame_takes_the_picture_away_and_keeps_the_cover() {
    let daemon = stub_daemon().await;
    daemon.serves_cover("abc", png());
    let backend = Backend::stub_remote(&daemon.socket).await;
    let (mut art, mut app) = (halfblocks(), playing(Some("abc")));
    settle(&mut art, &mut app, Some(&backend), SMALL).await;
    art.steer(&mut app, Some(&backend), Rect::new(0, 0, 70, 24));
    assert!(app.host.cover.picture.is_none());
    settle(&mut art, &mut app, Some(&backend), SMALL).await;
    assert!(app.host.cover.picture.is_some());
    assert_eq!(daemon.host_cover_asks(), [0], "the decoded cover was kept");
}

#[tokio::test]
async fn no_cover_in_the_view_takes_the_picture_away() {
    let daemon = stub_daemon().await;
    daemon.serves_cover("abc", png());
    let backend = Backend::stub_remote(&daemon.socket).await;
    let (mut art, mut app) = (halfblocks(), playing(Some("abc")));
    settle(&mut art, &mut app, Some(&backend), SMALL).await;
    app.host.cover.hash = None;
    art.steer(&mut app, Some(&backend), SMALL);
    assert!(app.host.cover.picture.is_none());
    assert!(!art.busy());
}

#[tokio::test]
async fn a_sixel_picture_replaced_moved_or_removed_owes_a_repaint_and_other_protocols_do_not() {
    for (protocol, owes) in [
        (ProtocolType::Sixel, true),
        (ProtocolType::Kitty, false),
        (ProtocolType::Halfblocks, false),
    ] {
        let daemon = stub_daemon().await;
        daemon.serves_cover("abc", png());
        let backend = Backend::stub_remote(&daemon.socket).await;
        let mut picker = Picker::halfblocks();
        picker.set_protocol_type(protocol);
        let (mut art, mut app) = (Art::with(Some(picker)), playing(Some("abc")));
        assert!(
            !art.clears_before(&app, SMALL),
            "{protocol:?}: nothing drawn yet"
        );
        settle(&mut art, &mut app, Some(&backend), SMALL).await;
        assert!(
            app.host.cover.picture.is_some(),
            "{protocol:?}: no picture was made"
        );
        assert!(
            !art.clears_before(&app, SMALL),
            "{protocol:?}: the first picture needs none"
        );
        assert!(
            !art.clears_before(&app, SMALL),
            "{protocol:?}: nor does the same one again"
        );

        // The track changes: the frame that draws something else there repaints.
        app.host.cover.hash = Some("def".into());
        daemon.serves_cover("def", png());
        art.steer(&mut app, Some(&backend), SMALL);
        assert_eq!(
            art.clears_before(&app, SMALL),
            owes,
            "{protocol:?}: on a track change"
        );
        assert!(
            !art.clears_before(&app, SMALL),
            "{protocol:?}: once per change"
        );

        // The new picture lands where the old one was: it is a picture to draw, not one to
        // clear, and then the frame grows under it.
        settle(&mut art, &mut app, Some(&backend), SMALL).await;
        assert!(
            !art.clears_before(&app, SMALL),
            "{protocol:?}: a picture appearing"
        );
        settle(&mut art, &mut app, Some(&backend), BIG).await;
        assert_eq!(
            art.clears_before(&app, BIG),
            owes,
            "{protocol:?}: on a resize"
        );

        // The cover goes: the picture goes with it, and the frame that does not draw it repaints.
        app.host.cover.hash = None;
        art.steer(&mut app, Some(&backend), BIG);
        assert_eq!(
            art.clears_before(&app, BIG),
            owes,
            "{protocol:?}: when the cover goes"
        );
    }
}

#[test]
fn the_three_formats_the_daemon_sniffs_decode() {
    for format in [ImageFormat::Png, ImageFormat::Jpeg, ImageFormat::WebP] {
        let image = decode(&encoded(40, 30, format)).unwrap_or_else(|| panic!("{format:?}"));
        assert_eq!((image.width(), image.height()), (40, 30), "{format:?}");
    }
}

#[test]
fn a_large_picture_is_kept_at_a_size_that_costs_little() {
    let image = decode(&encoded(2000, 1000, ImageFormat::Png)).expect("decodes");
    assert_eq!((image.width(), image.height()), (1024, 512));
}

#[test]
fn what_is_not_a_picture_or_asks_too_much_to_decode_is_refused() {
    assert!(decode(b"not a picture at all").is_none());
    assert!(decode(&[]).is_none());
    // Wider than any cover is: a few kilobytes that would decode to a large buffer.
    assert!(decode(&encoded(9000, 2, ImageFormat::Png)).is_none());
}

#[test]
fn a_picture_is_sized_to_the_box_and_keeps_its_proportions() {
    let size = ratatui::layout::Size {
        width: 8,
        height: 4,
    };
    let wide = DynamicImage::ImageRgb8(RgbImage::new(200, 100));
    let protocol = build(&Picker::halfblocks(), wide, size).expect("a picture");
    assert!(protocol.size().width <= 8 && protocol.size().height <= 4);
    assert!(protocol.size().width > protocol.size().height);
}

#[tokio::test]
async fn a_box_over_a_sixel_picture_owes_a_repaint_as_it_opens_and_as_it_closes() {
    for (protocol, owes) in [(ProtocolType::Sixel, true), (ProtocolType::Kitty, false)] {
        let daemon = stub_daemon().await;
        daemon.serves_cover("abc", png());
        let backend = Backend::stub_remote(&daemon.socket).await;
        let mut picker = Picker::halfblocks();
        picker.set_protocol_type(protocol);
        let (mut art, mut app) = (Art::with(Some(picker)), playing(Some("abc")));
        settle(&mut art, &mut app, Some(&backend), SMALL).await;
        assert!(!art.clears_before(&app, SMALL));
        app.overlays.help_open = true;
        assert_eq!(
            art.clears_before(&app, SMALL),
            owes,
            "{protocol:?}: box opens"
        );
        assert!(!art.clears_before(&app, SMALL), "{protocol:?}: box stays");
        app.overlays.help_open = false;
        assert_eq!(
            art.clears_before(&app, SMALL),
            owes,
            "{protocol:?}: box closes"
        );
        assert!(!art.clears_before(&app, SMALL));
    }
}

#[tokio::test]
async fn a_build_for_a_size_that_is_not_the_one_asked_last_is_dropped() {
    let (mut art, mut app) = (halfblocks(), playing(Some("abc")));
    app.host.cover.font = Some((10, 20));
    let small = art_box(SMALL, &app).expect("a box");
    let big = art_box(BIG, &app).expect("a box");
    art.building = Some(("abc".into(), big));
    let old = build(
        &Picker::halfblocks(),
        DynamicImage::ImageRgb8(RgbImage::new(64, 64)),
        small,
    );
    art.fold(
        Done::Built {
            hash: "abc".into(),
            size: small,
            protocol: old,
        },
        &mut app,
    );
    assert!(
        app.host.cover.picture.is_none(),
        "an older size's picture won the race"
    );
    assert_eq!(
        art.building,
        Some(("abc".into(), big)),
        "the newer build is still awaited"
    );
}

#[tokio::test]
async fn with_art_off_no_picker_is_made_and_nothing_is_spawned() {
    // Making one inside tmux runs `tmux set -p allow-passthrough on`: with the art off that is
    // never done, however long the loop runs.
    let (mut art, mut app) = (Art::with(None), playing(Some("abc")));
    app.tui.cover_art = CoverArt::Off;
    assert!(art.picker.is_none());
    for _ in 0..3 {
        art.steer(&mut app, None, SMALL);
    }
    assert!(art.picker.is_none(), "a picker was made with the art off");
    assert!(!art.busy());
}

#[tokio::test]
async fn art_turned_on_makes_its_picker_off_the_loop() {
    let (mut art, mut app) = (Art::with(None), playing(Some("abc")));
    art.steer(&mut app, None, SMALL);
    assert!(art.picker.is_none(), "made on the loop");
    assert!(art.busy(), "the picker is on its way");
    finish_a_step(&mut art, &mut app).await;
    assert!(art.picker.is_some());
}

/// A cover that is not square (a video's wide thumbnail) gets a box of its own shape, and is
/// built once for that box: the frames after it build nothing again, however many of them steer.
#[tokio::test]
async fn a_wide_cover_is_built_once_for_its_box_and_not_again_every_frame() {
    let daemon = stub_daemon().await;
    daemon.serves_cover(
        "wide",
        Cover {
            bytes: encoded(160, 90, ImageFormat::Png),
            mime: "image/png",
        },
    );
    let backend = Backend::stub_remote(&daemon.socket).await;
    let (mut art, mut app) = (halfblocks(), playing(Some("wide")));
    settle(&mut art, &mut app, Some(&backend), BIG).await;
    assert_eq!(
        app.host.cover.extent,
        Some(("wide".into(), 160, 90)),
        "the decoded size travels with the picture"
    );
    let picture = app.host.cover.picture.as_ref().expect("a picture").size();
    let most = art_box(BIG, &app).expect("a box");
    assert_eq!(picture, most, "the picture fills the box of its own shape");
    assert!(most.width > most.height * 2, "a wide box: {most:?}");
    for _ in 0..3 {
        art.steer(&mut app, Some(&backend), BIG);
        assert!(!art.busy(), "the picture is built again for the same box");
    }
}

/// A wide cover in a box too narrow for its shape (the words cap it) is cropped, never left
/// with bars: the built picture is as large as the box.
#[test]
fn a_wide_cover_in_a_capped_box_is_cropped_not_letterboxed() {
    let picker = Picker::halfblocks();
    let wide = DynamicImage::ImageRgb8(RgbImage::new(320, 180));
    let tall = DynamicImage::ImageRgb8(RgbImage::new(180, 320));
    for size in [Size::new(12, 4), Size::new(8, 4), Size::new(6, 12)] {
        for image in [&wide, &tall] {
            let protocol = build(&picker, image.clone(), size).expect("a picture");
            assert_eq!(protocol.size(), size, "{size:?} is filled");
        }
    }
}
