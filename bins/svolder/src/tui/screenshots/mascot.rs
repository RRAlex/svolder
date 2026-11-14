// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Stills of the squirrel's entrances: for each one a frame partway in, the landed frame and a
//! frame partway out, cropped round the squirrel's box on the Gungnir 2's device screen. Not
//! part of `make screenshots`: `make mascot-shots` runs the `#[ignore]` test below.

use std::path::Path;
use std::time::Duration;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::tui::app::Screen;
use crate::tui::decoration::mascot::Mascot;
use crate::tui::decoration::mascot::draw::home_box;
use crate::tui::decoration::mascot::fx::Entrance;
use crate::tui::run::drawn_turn;
use crate::tui::screenshots::raster::{Raster, fonts, write_ppm};
use crate::tui::screenshots::scenes::SIZE;
use crate::tui::screenshots::world::{STILL_VOLUME_DB, World, gungnir, runtime, stand_in};
use crate::tui::screenshots::{out_dir, scenes_palette};

/// How far into each entrance its middle picture is taken, where the animation shows most.
fn middle(kind: Entrance) -> f64 {
    match kind {
        Entrance::Transporter => 0.55,
        Entrance::Bifrost => 0.45,
        Entrance::Sparkle => 0.5,
        Entrance::Scanlines => 0.4,
        Entrance::Lightning => 0.42,
        Entrance::Drop => 0.62,
        Entrance::Longship => 0.5,
    }
}

/// How far into each exit its picture is taken.
fn middle_out(kind: Entrance) -> f64 {
    match kind {
        Entrance::Lightning => 0.55,
        Entrance::Longship => 0.5,
        other => middle(other),
    }
}

/// The name of an entrance in a file.
fn name(kind: Entrance) -> &'static str {
    match kind {
        Entrance::Transporter => "transporter",
        Entrance::Bifrost => "bifrost",
        Entrance::Sparkle => "sparkle",
        Entrance::Scanlines => "scanlines",
        Entrance::Lightning => "lightning",
        Entrance::Drop => "drop",
        Entrance::Longship => "longship",
    }
}

/// The cells of `buffer` inside `area`, as a buffer of their own.
fn cropped(buffer: &Buffer, area: Rect) -> Buffer {
    let mut out = Buffer::empty(area);
    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            out[(x, y)] = buffer[(x, y)].clone();
        }
    }
    out
}

/// Writes `<kind>-in.ppm`, `<kind>-landed.ppm` and `<kind>-out.ppm` for every entrance into
/// `out`, under the Rest pose of the amber costume.
fn shots_into(out: &Path) -> Result<(), String> {
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let fonts = fonts::load_system().map_err(|m| m.message())?;
    let mut raster = Raster::new(fonts, scenes_palette(&stand_in()));
    for kind in Entrance::ALL {
        let frames = runtime().block_on(async {
            let world = World::new(stand_in());
            let t0 = world.base;
            let mut app = gungnir(&world, t0, STILL_VOLUME_DB).await;
            app.show(Screen::Device);
            app.mascot = Mascot::new(t0);
            app.mascot.capable = true;
            app.mascot.effects = true;
            app.mascot.palette = "amber".into();
            app.mascot.forced = Some(kind);
            let World { mut look, .. } = world;
            let mut terminal = Terminal::new(TestBackend::new(SIZE.0, SIZE.1)).expect("a terminal");
            let mut take = |app: &mut crate::tui::app::App, at: Duration| {
                let now = t0 + at;
                let view = app.mascot_view();
                app.mascot.observe(&view, now);
                let (buffer, _) = drawn_turn(
                    &mut terminal,
                    app,
                    &mut look,
                    now,
                    Duration::from_millis(33),
                );
                buffer
            };
            let hour = Duration::from_secs(3600);
            let mut app_frames = Vec::new();
            // The screen settles for two seconds, as a still does.
            for n in 0..61 {
                take(&mut app, Duration::from_millis(33) * n);
            }
            // The first hour calls the squirrel out by the entrance.
            let into = hour + kind.duration().mul_f64(middle(kind));
            take(&mut app, hour);
            app_frames.push(take(&mut app, into));
            let settled = hour + kind.duration() + Duration::from_secs(8);
            app_frames.push(take(&mut app, settled));
            // Half an hour on, the zap takes it out by the matching exit.
            let mut at = settled;
            while at < hour + kind.duration() + Duration::from_secs(1795) {
                at += Duration::from_secs(10);
                take(&mut app, at);
            }
            let zap_from = at;
            let mut leaving = None;
            for step in 0..200 {
                let now = zap_from + Duration::from_millis(50 * step);
                let buffer = take(&mut app, now);
                if leaving.is_none()
                    && app
                        .mascot
                        .showing()
                        .and_then(|s| s.fx)
                        .is_some_and(|fx| fx.out && fx.progress >= middle_out(kind))
                {
                    leaving = Some(buffer);
                }
            }
            app_frames.extend(leaving);
            (app_frames, app, look)
        });
        let (buffers, app, look) = frames;
        let theme = look.theme;
        let pane =
            crate::tui::screens::device::home_of(Rect::new(0, 0, SIZE.0, SIZE.1), &app, &theme)
                .ok_or("the device screen has no home pane")?
                .rect;
        let home = home_box(pane);
        let area = Rect {
            x: home.x.saturating_sub(3),
            y: home.y.saturating_sub(2),
            width: home.width + 6,
            height: home.height + 3,
        }
        .intersection(Rect::new(0, 0, SIZE.0, SIZE.1));
        for (buffer, tag) in buffers.iter().zip(["in", "landed", "out"]) {
            let place = format!("{}-{tag}", name(kind));
            let image = raster.paint(&cropped(buffer, area), &place)?;
            let path = out.join(format!("{place}.ppm"));
            write_ppm(&path, &image).map_err(|e| format!("{}: {e}", path.display()))?;
        }
        if buffers.len() != 3 {
            return Err(format!("{}: only {} pictures", name(kind), buffers.len()));
        }
    }
    Ok(())
}

/// Draws the entrance stills into `target/screenshots/mascot`.
#[test]
#[ignore = "make mascot-shots runs it"]
fn mascot_entrances() {
    if let Err(why) = shots_into(&out_dir().join("mascot")) {
        panic!("mascot shots: {why}");
    }
}
