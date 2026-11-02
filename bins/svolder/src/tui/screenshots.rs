// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The README's pictures, drawn by the TUI's own renderer on the fake devices: the scenes
//! ([`scenes`]) in the world they are drawn in ([`world`]), cells turned into pixels
//! ([`raster`]), and each picture's text twin with the guard that refuses a frame carrying a
//! detail of this machine ([`text`]). `make screenshots` runs the `#[ignore]` test `generate`
//! and encodes what it writes under `target/screenshots/`.
//!
//! Compiled for tests only: nothing here reaches the binary.

mod raster;

mod scenes;

mod text;

mod world;

mod tests;

use std::path::{Path, PathBuf};
use std::time::Instant;

use svolder_autoeq::Client;
use svolder_profiles::tone::apo::{self, Correction};

use crate::tui::screenshots::raster::fonts;
use crate::tui::screenshots::raster::{Palette, Raster, RawStream, write_ppm};
use crate::tui::screenshots::scenes::{Still, animation, still};
use crate::tui::screenshots::text::{AnimationTwin, HostDetails, guard, still_twin};
use crate::tui::screenshots::world::{PRESET, World, measurement};

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

/// The palette every picture is painted in: the scenes' theme.
///
/// The theme does not depend on the correction; the world is only built for it.
pub(in crate::tui::screenshots) fn scenes_palette(correction: &Correction) -> Palette {
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
    let mut raster = Raster::new(fonts, scenes_palette(&correction));

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
