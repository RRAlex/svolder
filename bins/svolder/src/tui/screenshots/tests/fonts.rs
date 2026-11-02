// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The fonts and the painted frame. A test that needs a font skips, saying which, when the
//! machine has none.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};

use crate::tui::screenshots::raster::fonts::*;
use crate::tui::screenshots::raster::*;
use crate::tui::screenshots::tests::palette;

fn fonts() -> Option<Fonts> {
    match load_system() {
        Ok(fonts) => Some(fonts),
        Err(missing) => {
            eprintln!("skipped: {}", missing.message());
            None
        }
    }
}

#[test]
fn a_missing_font_names_its_package() {
    let missing = load(
        &["/nonexistent/UbuntuSansMono.ttf"],
        DEJAVU_SANS,
        DEJAVU_SANS_BOLD,
    )
    .err()
    .expect("no such font");
    assert_eq!(missing.package, "fonts-ubuntu");
    assert!(
        missing
            .message()
            .contains("/nonexistent/UbuntuSansMono.ttf")
    );
}

#[test]
fn the_cell_is_sixteen_pixels_wide_and_a_whole_number_tall() {
    let Some(fonts) = fonts() else { return };
    let cell = fonts.cell();
    assert_eq!(cell.w, 16);
    assert!((28..=44).contains(&cell.h), "{}", cell.h);
}

#[test]
fn bold_is_heavier_than_regular() {
    let Some(fonts) = fonts() else { return };
    let ink = |bold| fonts.glyph('M', bold).expect("M").iter().sum::<f32>();
    assert!(ink(true) > ink(false) * 1.1);
}

#[test]
fn every_symbol_the_tui_draws_has_a_glyph() {
    let Some(fonts) = fonts() else { return };
    for c in "‹›▶◀▮●○·…—–×°±↵⇧␣←↑→↓•⚠▲▼⚡♨¹²³⁴⁵⁶⁷⁸⁹".chars()
    {
        assert!(fonts.glyph(c, false).is_some(), "U+{:04X} {c}", c as u32);
    }
}

#[test]
fn a_character_no_font_has_is_an_error_naming_it() {
    let Some(fonts) = fonts() else { return };
    let mut raster = Raster::new(fonts, palette());
    let buffer = Buffer::with_lines(["\u{E000}"]);
    let why = raster.paint(&buffer, "dac").expect_err("no glyph");
    assert!(why.contains("U+E000") && why.contains("dac"), "{why}");
}

#[test]
fn two_rasters_paint_one_buffer_to_identical_pixels() {
    let (Some(a), Some(b)) = (fonts(), fonts()) else {
        return;
    };
    let buffer = Buffer::with_lines(["svolder ─┐ ▓▒░ ⣿ ‹ 3 · Optical ›"]);
    let mut first = Raster::new(a, palette());
    let one = first.paint(&buffer, "t").expect("drawn");
    let two = Raster::new(b, palette())
        .paint(&buffer, "t")
        .expect("drawn");
    assert_eq!(one.rgb, two.rgb);
    assert_eq!(one.rgb, first.paint(&buffer, "t").expect("cached").rgb);
}

#[test]
fn a_full_frame_is_1920_wide_and_forty_cells_tall_inside_its_margin() {
    let Some(fonts) = fonts() else { return };
    let mut raster = Raster::new(fonts, palette());
    let h = raster.cell().h;
    let image = raster
        .paint(&Buffer::empty(Rect::new(0, 0, 120, 40)), "t")
        .expect("drawn");
    assert_eq!((image.width, image.height), (1920 + 48, 40 * h + 48));
    assert_eq!(&image.rgb[..3], &[7, 6, 5], "a blank cell is the ground");
}

/// Whether every pixel outside the grid (the 24 px margin) of `image` is `ground`.
fn margin_is(image: &Image, ground: Rgb) -> bool {
    let (w, h) = (image.width, image.height);
    (0..h).all(|y| {
        (0..w).all(|x| {
            let inside = (MARGIN..w - MARGIN).contains(&x) && (MARGIN..h - MARGIN).contains(&y);
            let at = ((y * w + x) * 3) as usize;
            inside || image.rgb[at..at + 3] == [ground.0, ground.1, ground.2]
        })
    })
}

#[test]
fn the_margin_is_the_palettes_ground_even_where_the_grid_is_not() {
    let Some(fonts) = fonts() else { return };
    let mut raster = Raster::new(fonts, palette());
    let bright = Style::default()
        .fg(Color::Rgb(255, 255, 255))
        .bg(Color::Rgb(200, 10, 10));
    let mut buffer = Buffer::empty(Rect::new(0, 0, 4, 2));
    buffer.set_style(buffer.area, bright);
    buffer[(0, 0)].set_symbol("M");
    let image = raster.paint(&buffer, "t").expect("drawn");
    assert!(margin_is(&image, palette().ground));
}

#[test]
fn the_first_cell_starts_at_the_margin() {
    let Some(fonts) = fonts() else { return };
    let mut raster = Raster::new(fonts, palette());
    let mut buffer = Buffer::empty(Rect::new(0, 0, 2, 1));
    buffer.set_style(buffer.area, Style::default().bg(Color::Rgb(200, 10, 10)));
    let image = raster.paint(&buffer, "t").expect("drawn");
    let at = |x: u32, y: u32| {
        let i = ((y * image.width + x) * 3) as usize;
        image.rgb[i..i + 3].to_vec()
    };
    assert_eq!(at(MARGIN, MARGIN), [200, 10, 10]);
    assert_eq!(at(MARGIN - 1, MARGIN), [7, 6, 5]);
    assert_eq!(at(MARGIN, MARGIN - 1), [7, 6, 5]);
    assert_eq!(
        at(image.width - MARGIN - 1, image.height - MARGIN - 1),
        [200, 10, 10]
    );
    assert_eq!(at(image.width - MARGIN, image.height - MARGIN), [7, 6, 5]);
}

#[test]
fn a_still_is_painted_in_the_ground_of_the_scenes_theme() {
    use crate::tui::screenshots::scenes::{Still, still};
    use crate::tui::screenshots::scenes_palette;
    use crate::tui::screenshots::world::stand_in;
    let Some(fonts) = fonts() else { return };
    let ground = scenes_palette(&stand_in());
    let mut raster = Raster::new(fonts, ground);
    for s in Still::ALL {
        let shot = still(&stand_in(), s);
        let image = raster.paint(&shot.buffer, s.name()).expect("drawn");
        assert!(margin_is(&image, ground.ground), "{}", s.name());
        assert_eq!(
            ground.ground,
            (7, 6, 5),
            "{}: the one amber ground",
            s.name()
        );
    }
}

#[test]
fn the_tail_of_a_wide_character_is_its_own_background() {
    let Some(fonts) = fonts() else { return };
    let mut raster = Raster::new(fonts, palette());
    let h = raster.cell().h as usize;
    let bg = Color::Rgb(9, 8, 7);
    let bright = Style::default()
        .fg(Color::Rgb(255, 255, 255))
        .bg(bg)
        .add_modifier(ratatui::style::Modifier::BOLD);
    let mut buffer = Buffer::empty(Rect::new(0, 0, 2, 1));
    buffer[(0, 0)].set_symbol("M").set_style(bright);
    buffer[(1, 0)].set_symbol("").set_style(bright);
    let image = raster.paint(&buffer, "t").expect("drawn");
    let pixel = |x: usize, y: usize| {
        let at = ((y + MARGIN as usize) * (32 + 48) + x + MARGIN as usize) * 3;
        image.rgb[at..at + 3].to_vec()
    };
    let cell = |first: usize| (0..h).flat_map(move |y| (first..first + 16).map(move |x| (x, y)));
    assert!(
        cell(0).any(|(x, y)| pixel(x, y) != [9, 8, 7]),
        "the M is drawn in cell 0"
    );
    assert!(
        cell(16).all(|(x, y)| pixel(x, y) == [9, 8, 7]),
        "the tail holds nothing but the background"
    );
}

#[test]
fn a_cell_wrong_twice_reports_its_symbol_first() {
    let Some(fonts) = fonts() else { return };
    let mut raster = Raster::new(fonts, palette());
    let mut buffer = Buffer::empty(Rect::new(0, 0, 1, 1));
    buffer[(0, 0)].set_symbol("ab").modifier = ratatui::style::Modifier::UNDERLINED;
    let why = raster.paint(&buffer, "t").expect_err("refused");
    assert!(why.contains("several characters"), "{why}");
}

#[test]
fn a_wide_character_is_an_error_naming_it() {
    let Some(fonts) = fonts() else { return };
    let mut raster = Raster::new(fonts, palette());
    let mut buffer = Buffer::empty(Rect::new(0, 0, 2, 1));
    buffer[(0, 0)].set_symbol("世");
    let why = raster.paint(&buffer, "t").expect_err("refused");
    assert!(why.contains("U+4E16") && why.contains("wide"), "{why}");
}

#[test]
fn a_cell_of_several_characters_is_an_error_naming_its_place_and_cell() {
    let Some(fonts) = fonts() else { return };
    let mut raster = Raster::new(fonts, palette());
    let mut buffer = Buffer::empty(Rect::new(0, 0, 3, 2));
    buffer[(2, 1)].set_symbol("ab");
    let why = raster.paint(&buffer, "dac").expect_err("refused");
    assert!(why.starts_with("dac, cell (2, 1): "), "{why}");
    assert!(why.contains("several characters"), "{why}");
}

#[test]
fn the_ppm_and_the_raw_stream_carry_their_sizes() {
    let dir = std::env::temp_dir().join(format!("svolder-shots-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    let image = Image {
        width: 2,
        height: 1,
        rgb: vec![1, 2, 3, 4, 5, 6],
    };
    write_ppm(&dir.join("a.ppm"), &image).expect("written");
    assert_eq!(
        std::fs::read(dir.join("a.ppm")).expect("read"),
        b"P6\n2 1\n255\n\x01\x02\x03\x04\x05\x06"
    );
    let mut stream = RawStream::create(&dir.join("s.rgb"), 2, 1).expect("made");
    stream.push(&image).expect("pushed");
    stream.push(&image).expect("pushed");
    let other = Image {
        width: 1,
        height: 1,
        rgb: vec![0, 0, 0],
    };
    assert!(stream.push(&other).is_err(), "a frame of another size");
    assert_eq!(stream.finish().expect("finished"), 2);
    assert_eq!(std::fs::read(dir.join("s.rgb")).expect("read").len(), 12);
    assert_eq!(
        std::fs::read_to_string(dir.join("s.rgb.size")).expect("read"),
        "2 1 2\n"
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// The script checks exactly the font paths the raster loads, no more and no fewer.
#[test]
fn the_script_checks_the_fonts_the_raster_loads() {
    let script = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/screenshots.sh"),
    )
    .expect("scripts/screenshots.sh reads");
    let in_script: std::collections::BTreeSet<&str> = script
        .split(|c: char| c.is_whitespace() || c == '"')
        .filter(|word| word.starts_with("/usr/share/fonts/"))
        .collect();
    let loaded: std::collections::BTreeSet<&str> = UBUNTU_SANS_MONO
        .iter()
        .chain(DEJAVU_SANS)
        .chain(DEJAVU_SANS_BOLD)
        .copied()
        .collect();
    assert_eq!(in_script, loaded);
}
