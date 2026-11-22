// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The README's pictures, drawn by the TUI's own renderer on the fake devices: the stills
//! ([`shots`]) in the world they are drawn in ([`world`]), cells turned into pixels
//! ([`raster`]), and each picture's text twin with the guard that refuses a frame carrying a
//! detail of this machine ([`text`]). `make screenshots` runs the `#[ignore]` test `generate`
//! and encodes what it writes under `target/screenshots/`.
//!
//! Compiled for tests only: nothing here reaches the binary.

mod mascot;

mod raster;

mod shots;

mod text;

mod world;

mod tests;

use std::path::{Path, PathBuf};
use std::time::Instant;

use svolder_autoeq::Client;
use svolder_profiles::tone::apo::{self, Correction};

use crate::tui::screenshots::raster::fonts;
use crate::tui::screenshots::raster::{Palette, Raster, RawStream, write_ppm};
use crate::tui::screenshots::shots::{Still, animation, still, still_sized};
use crate::tui::screenshots::text::{AnimationTwin, HostDetails, guard, still_twin};
use crate::tui::screenshots::world::{PRESET, World, measurement, stand_in};

/// Where the pictures, their twins and the cached correction are written: `target/screenshots`
/// of the workspace, wherever cargo puts its build output.
fn out_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/screenshots")
}

/// The HD 800 S correction: the cached `ParametricEQ.txt` in `dir` when there is one, else the
/// one `client` downloads, written there for the next run. `client` is made only then.
pub(super) fn correction_from(
    dir: &Path,
    client: impl FnOnce() -> Client,
) -> Result<Correction, String> {
    let path = dir.join(format!("{PRESET} ParametricEQ.txt"));
    let (text, cached) = match std::fs::read_to_string(&path) {
        Ok(text) => (text, true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let client = client();
            let text = client.download(&measurement()).map_err(|e| {
                format!(
                    "the {PRESET} correction could not be downloaded from {} ({e}); put its \
                     ParametricEQ.txt at {} and run again",
                    client.base(),
                    path.display()
                )
            })?;
            (text, false)
        }
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    let parsed = apo::parse(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    if !cached {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        std::fs::write(&path, &text).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(parsed.correction)
}

/// The palette every picture is painted in: the stills' theme.
///
/// The theme does not depend on the correction; the world is only built for it.
pub(in crate::tui::screenshots) fn shots_palette(correction: &Correction) -> Palette {
    Palette::of(&World::new(correction.clone()).look.theme)
}

/// Draws the five stills and the animation into `out`, with their text twins.
fn generate_into(out: &Path) -> Result<(), String> {
    let started = Instant::now();
    let io = |path: &Path| {
        let path = path.to_path_buf();
        move |e: std::io::Error| format!("{}: {e}", path.display())
    };
    std::fs::create_dir_all(out).map_err(io(out))?;
    let autoeq = out.join("autoeq");
    // Not `SVOLDER_AUTOEQ_URL`: every test binary points that at a dead address, so this
    // generator reads a variable of its own and otherwise uses AutoEq's repository.
    let correction = correction_from(&autoeq, || {
        Client::from_var("SVOLDER_SCREENSHOTS_AUTOEQ_URL", autoeq.join("cache"))
    })?;
    let fonts = fonts::load_system().map_err(|m| m.message())?;
    let host = HostDetails::of_this_machine();
    let mut raster = Raster::new(fonts, shots_palette(&correction));

    for s in Still::ALL {
        let shot = still(&correction, s);
        guard(&shot.buffer, &host, s.name())?;
        let ppm = out.join(format!("{}.ppm", s.name()));
        write_ppm(&ppm, &raster.paint(&shot.buffer, s.name())?).map_err(io(&ppm))?;
        let txt = out.join(format!("{}.txt", s.name()));
        std::fs::write(&txt, still_twin(&shot.buffer)).map_err(io(&txt))?;
        println!("{}: clip step {:?}", s.name(), shot.clip_step);
    }

    let animation = animation(&correction);
    let first = animation
        .frames
        .first()
        .ok_or("the animation drew no frame")?;
    let (width, height) = raster.picture_size(first);
    let rgb = out.join("svolder.rgb");
    let mut stream = RawStream::create(&rgb, width, height).map_err(io(&rgb))?;
    let mut twin = AnimationTwin::default();
    for (n, frame) in animation.frames.iter().enumerate() {
        let place = format!("svolder, frame {n}");
        guard(frame, &host, &place)?;
        twin.add(frame);
        stream
            .push(&raster.paint(frame, &place)?)
            .map_err(io(&rgb))?;
    }
    let frames = stream.finish().map_err(io(&rgb))?;
    let txt = out.join("svolder.txt");
    std::fs::write(&txt, twin.text()).map_err(io(&txt))?;
    println!(
        "svolder: {frames} frames of {width}x{height}; loop_start {}, volume_frame {}, key_frame {}, \
         gain_frame {}, fade_start {}",
        animation.loop_start,
        animation.volume_frame,
        animation.key_frame,
        animation.gain_frame,
        animation.fade_start
    );
    println!(
        "screenshots: {} stills, image {width}x{height}, in {:.1?}: {}",
        Still::ALL.len(),
        started.elapsed(),
        out.display()
    );
    Ok(())
}

/// Draws the README's pictures. It downloads one AutoEq file the first time.
#[test]
#[ignore = "make screenshots runs it: it draws the README's pictures and downloads one AutoEq file once"]
fn generate() {
    if let Err(why) = generate_into(&out_dir()) {
        panic!("screenshots: {why}");
    }
}

/// The device screen at 80, 120 and 200 columns, and at 120 and 200 with the divider dragged, as PPM
/// files in the divider folder of the screenshots output, each name starting with `after-`
/// (renamed by hand for a before and an after of a layout change). Offline: the stand-in correction is worn.
#[test]
#[ignore = "draws the divider's pictures: cargo test -p svolder --bin svolder -- --ignored divider_shots"]
fn divider_shots() {
    let tag = "after";
    let out = out_dir().join("divider");
    std::fs::create_dir_all(&out).expect("the output folder");
    let correction = stand_in();
    let mut raster = Raster::new(
        fonts::load_system()
            .map_err(|m| m.message())
            .expect("fonts"),
        shots_palette(&correction),
    );
    for (name, size, split) in [
        ("80", (80, 24), None),
        ("120", (120, 30), None),
        ("200", (200, 40), None),
        ("120-dragged", (120, 30), Some(0.7)),
        ("200-dragged", (200, 40), Some(0.7)),
    ] {
        let shot = still_sized(&correction, Still::Dac, size, |app| {
            if let Some(split) = split {
                app.tui.device_split = split;
            }
        });
        let ppm = out.join(format!("{tag}-{name}.ppm"));
        let image = raster.paint(&shot.buffer, name).expect("painted");
        write_ppm(&ppm, &image).expect("written");
    }
}

/// The device screen with the VU rows at 100 by 45 and at 160 by 50 (the Gungnir 2 and the capture's
/// seeded signal), as PPM files and text twins in the vu folder of the screenshots output.
/// Offline: the stand-in correction is worn.
#[test]
#[ignore = "draws the VU stills: cargo test -p svolder --bin svolder -- --ignored vu_shots"]
fn vu_shots() {
    let out = out_dir().join("vu");
    std::fs::create_dir_all(&out).expect("the output folder");
    let correction = stand_in();
    let mut raster = Raster::new(
        fonts::load_system()
            .map_err(|m| m.message())
            .expect("fonts"),
        shots_palette(&correction),
    );
    for (name, size, cover) in [
        ("100x45", (100, 45), false),
        ("160x50", (160, 50), false),
        ("100x45-cover", (100, 45), true),
        ("160x50-cover", (160, 50), true),
    ] {
        let shot = still_sized(&correction, Still::Dac, size, |app| {
            if cover {
                with_half_block_cover(app, size);
            }
        });
        let ppm = out.join(format!("{name}.ppm"));
        let image = raster.paint(&shot.buffer, name).expect("painted");
        write_ppm(&ppm, &image).expect("written");
        std::fs::write(out.join(format!("{name}.txt")), still_twin(&shot.buffer))
            .expect("the text twin");
    }
}

/// The computer's Loudness in the TUI: the device screen's Tone pane and the tone screen, folded and
/// unfolded, at 80 by 24 and 120 by 40, as PPM files and text twins in the loudness folder of the
/// screenshots output. The view is the worked example (strength 50 %, reference 100 %, volume 35 %).
/// Offline: the stand-in correction is worn.
#[test]
#[ignore = "draws the Loudness stills: cargo test -p svolder --bin svolder -- --ignored loudness_shots"]
fn loudness_shots() {
    use crate::tui::app::{Ld, Screen};
    let out = out_dir().join("loudness");
    std::fs::create_dir_all(&out).expect("the output folder");
    let correction = stand_in();
    let mut raster = Raster::new(
        fonts::load_system()
            .map_err(|m| m.message())
            .expect("fonts"),
        shots_palette(&correction),
    );
    for (name, size, screen, unfolded) in [
        ("pane-80x24", (80, 24), Screen::Device, false),
        ("pane-120x40", (120, 40), Screen::Device, false),
        ("tone-80x24", (80, 24), Screen::Tone, false),
        ("tone-80x24-open", (80, 24), Screen::Tone, true),
        ("tone-120x40", (120, 40), Screen::Tone, false),
        ("tone-120x40-open", (120, 40), Screen::Tone, true),
    ] {
        let shot = still_sized(&correction, Still::Dac, size, |app| {
            app.host.computer.eq = Some(crate::tui::test_support::computer_eq_with_loudness(
                true,
                (50.0, 100.0),
                Some(35.0),
                13.7,
            ));
            app.flip_side();
            app.show(screen);
            if unfolded {
                app.focus_ld(Ld::Strength);
            }
        });
        let ppm = out.join(format!("{name}.ppm"));
        let image = raster.paint(&shot.buffer, name).expect("painted");
        write_ppm(&ppm, &image).expect("written");
        std::fs::write(out.join(format!("{name}.txt")), still_twin(&shot.buffer))
            .expect("the text twin");
    }
}

/// Give `app` a cover to show beside its Now playing row in a frame `size` large, drawn as
/// half-blocks from a gradient.
fn with_half_block_cover(app: &mut crate::tui::app::App, size: (u16, u16)) {
    use image::{DynamicImage, RgbImage};
    use ratatui::layout::Rect;
    app.host.cover.hash = Some("0123456789abcdef".into());
    app.host.cover.font = Some((8, 16));
    let frame = Rect::new(0, 0, size.0, size.1);
    let boxed = crate::tui::screens::device::art_box(frame, app).expect("a box for the cover");
    let image = DynamicImage::ImageRgb8(RgbImage::from_fn(96, 96, |x, y| {
        image::Rgb([(x * 2) as u8, (y * 2) as u8, 128])
    }));
    let protocol =
        crate::tui::art::engine::build(&ratatui_image::picker::Picker::halfblocks(), image, boxed)
            .expect("a picture");
    app.host.cover.picture = Some(crate::tui::art::Picture {
        protocol,
        hash: "0123456789abcdef".into(),
        sent: Default::default(),
    });
}
