// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use ratatui::style::{Color, Modifier};
use svolder_ipc::{ColorDepth, TuiConfig};

use super::*;
use crate::tui::test_support::true_colour;

/// The terminal the cases draw in: the smallest a screen is drawn in.
const SIZE: (u16, u16) = (80, 24);

/// A frame the size of the smallest terminal, every cell of it saying `symbol`: the screen
/// a passage plays over.
fn screen(symbol: &str) -> Buffer {
    let mut buf = Buffer::empty(Rect::new(0, 0, SIZE.0, SIZE.1));
    for y in 0..SIZE.1 {
        for x in 0..SIZE.0 {
            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.set_symbol(symbol);
            }
        }
    }
    buf
}

/// Whether `symbol` is one of the noise's shade glyphs.
fn shaded(symbol: &str) -> bool {
    SHADES.contains(&symbol)
}

/// The cells of `buf` the noise covers.
fn covered(buf: &Buffer) -> Vec<(u16, u16)> {
    let area = buf.area;
    (area.y..area.bottom())
        .flat_map(|y| (area.x..area.right()).map(move |x| (x, y)))
        .filter(|at| buf.cell(*at).is_some_and(|cell| shaded(cell.symbol())))
        .collect()
}

/// One passage of the noise over a fresh screen, painted at each of `ticks` in turn, and
/// the frame each paint left.
fn played(theme: &Theme, ticks: &[Duration]) -> Vec<Buffer> {
    let mut passage = Passage::noise();
    ticks
        .iter()
        .map(|tick| {
            let mut buf = screen("x");
            passage.paint(&mut buf, theme, *tick);
            buf
        })
        .collect()
}

/// The noise leaves every cell's background as it was painted, at every moment of it.
#[test]
fn the_noise_leaves_the_backgrounds_as_painted() {
    let theme = true_colour();
    let ground = Color::Rgb(0x0d, 0x0c, 0x0a);
    let mut passage = Passage::noise();
    for tick in [5_000, 25, 25, 100, 100] {
        let mut buf = screen("x");
        for cell in buf.content.iter_mut() {
            cell.bg = ground;
        }
        passage.paint(&mut buf, &theme, Duration::from_millis(tick));
        assert!(
            buf.content.iter().all(|cell| cell.bg == ground),
            "a background changed at tick {tick}"
        );
    }
}

/// The noise covers every cell at its start, whatever time the frame before it says went
/// by, and none once its 250 ms are up; in between the covered share only ever falls, a
/// cell cleared at one frame clear at every later one, and it falls all the way.
#[test]
fn the_noise_covers_every_cell_at_the_start_and_none_at_250_ms() {
    let theme = true_colour();
    let step = Duration::from_millis(25);
    // The first paint is the passage's start, however long the loop waited before it.
    let mut ticks = vec![Duration::from_secs(5)];
    ticks.extend(std::iter::repeat_n(step, 10));
    let frames = played(&theme, &ticks);

    let cells = usize::from(SIZE.0) * usize::from(SIZE.1);
    assert_eq!(
        covered(&frames[0]).len(),
        cells,
        "a cell was clear at the start"
    );
    let last = frames.last().expect("a frame at 250 ms");
    assert!(
        covered(last).is_empty(),
        "cells were still covered at 250 ms"
    );

    for (at, pair) in frames.windows(2).enumerate() {
        let (before, after) = (covered(&pair[0]), covered(&pair[1]));
        assert!(
            after.iter().all(|cell| before.contains(cell)),
            "a cell cleared by frame {at} was covered again on the next"
        );
        assert!(
            after.len() < before.len() || before.is_empty(),
            "the covered share did not fall from frame {at} to the next: {} then {}",
            before.len(),
            after.len()
        );
    }
    // Halfway through, about half the cells are covered: the fall is spread over the
    // whole passage rather than bunched at one end of it.
    let half = covered(&frames[5]).len() as f32 / cells as f32;
    assert!(
        (0.4..0.6).contains(&half),
        "{half} of the cells are covered halfway through"
    );
}

/// Nothing about the noise is random: two passages painted at the same moments draw the
/// very same frames, glyph and colour alike.
#[test]
fn the_noise_is_the_same_every_time_it_plays() {
    let theme = true_colour();
    let ticks = [
        Duration::ZERO,
        Duration::from_millis(60),
        Duration::from_millis(60),
        Duration::from_millis(60),
    ];
    assert_eq!(played(&theme, &ticks), played(&theme, &ticks));
}

/// At true colour the noise is drawn in the palette's own spectrum — more than one of its
/// colours across the field, each of them a colour the spectrum runs through — and at 16
/// colours and in monochrome, where there is no spread of colours to draw in, in the dim
/// style.
#[test]
fn the_noise_takes_the_palettes_spectrum_or_the_dim_style_where_there_is_none() {
    let theme = true_colour();
    let buf = &played(&theme, &[Duration::ZERO])[0];
    let spectrum: Vec<Color> = (0..=20)
        .filter_map(|at| theme.along_spectrum(at as f32 / 10.0 - 1.0))
        .collect();
    let mut seen: Vec<Color> = covered(buf)
        .into_iter()
        .map(|at| buf.cell(at).expect("a covered cell").fg)
        .collect();
    seen.sort_by_key(|colour| format!("{colour:?}"));
    seen.dedup();
    assert!(
        seen.len() > 10,
        "the field is drawn in {} colours",
        seen.len()
    );
    for colour in &seen {
        let Color::Rgb(r, g, b) = *colour else {
            panic!("a noise cell is not a true colour: {colour:?}");
        };
        let near = spectrum.iter().any(|stop| {
            let Color::Rgb(sr, sg, sb) = *stop else {
                return false;
            };
            let d = |a: u8, b: u8| i32::from(a).abs_diff(i32::from(b));
            d(r, sr) + d(g, sg) + d(b, sb) < 90
        });
        assert!(near, "{colour:?} is nowhere along the palette's spectrum");
    }

    for (what, theme) in [
        (
            "16 colours",
            Theme::from_config(&TuiConfig::default(), false, ColorDepth::Ansi16),
        ),
        (
            "monochrome",
            Theme::from_config(&TuiConfig::default(), true, ColorDepth::TrueColor),
        ),
    ] {
        let buf = &played(&theme, &[Duration::ZERO])[0];
        let cells = covered(buf);
        assert!(!cells.is_empty(), "{what}: nothing is covered");
        for at in cells {
            let cell = buf.cell(at).expect("a covered cell");
            assert!(
                cell.modifier.contains(Modifier::DIM) && cell.fg == Color::Reset,
                "{what}: {at:?} is not in the dim style: {:?} {:?}",
                cell.fg,
                cell.modifier
            );
        }
    }
}

/// A terminal resized under a passage ends it: the frame of the new size is drawn as it is,
/// with nothing covered, and the passage says it is over.
#[test]
fn a_resize_ends_the_passage_under_way() {
    let theme = true_colour();
    let mut passage = Passage::noise();
    let mut buf = screen("x");
    assert!(passage.paint(&mut buf, &theme, Duration::ZERO));
    let mut wider = Buffer::empty(Rect::new(0, 0, SIZE.0 + 10, SIZE.1));
    assert!(
        !passage.paint(&mut wider, &theme, Duration::from_millis(10)),
        "the passage went on over a terminal of another size"
    );
    assert!(covered(&wider).is_empty(), "the resized frame was covered");
    assert!(passage.over());
}

/// An old screen `width` by `height` whose every cell names the row it is on, so a cell
/// the melt has moved says where it came from.
fn old_screen(width: u16, height: u16) -> Buffer {
    let mut buf = Buffer::empty(Rect::new(0, 0, width, height));
    for y in 0..height {
        let row = char::from(b'a' + u8::try_from(y).expect("a short screen"));
        for x in 0..width {
            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.set_symbol(&row.to_string());
            }
        }
    }
    buf
}

/// A new screen `width` by `height`, every cell of it `#`.
fn new_screen(width: u16, height: u16) -> Buffer {
    let mut buf = Buffer::empty(Rect::new(0, 0, width, height));
    for cell in &mut buf.content {
        cell.set_symbol("#");
    }
    buf
}

/// Whether any cell of `buf` still shows the old screen.
fn any_old(buf: &Buffer) -> bool {
    buf.content.iter().any(|cell| cell.symbol() != "#")
}

/// The melt's first frame is the old screen exactly, whatever time the frame before it
/// says went by; its last is the new screen exactly; and in between the old screen's
/// columns have slid down whole — a cell shows the old row it came from, some rows above
/// where that row was — with the new screen above them.
#[test]
fn the_melt_starts_as_the_old_screen_and_ends_as_the_new() {
    let theme = true_colour();
    let old = old_screen(SIZE.0, SIZE.1);
    let mut passage = Passage::melt(old.clone());

    let mut first = new_screen(SIZE.0, SIZE.1);
    assert!(passage.paint(&mut first, &theme, Duration::from_secs(5)));
    assert_eq!(first, old, "the melt's first frame is not the old screen");

    let mut middle = new_screen(SIZE.0, SIZE.1);
    assert!(passage.paint(&mut middle, &theme, Duration::from_millis(300)));
    for x in 0..SIZE.0 {
        let column: Vec<&str> = (0..SIZE.1)
            .map(|y| middle.cell((x, y)).expect("a cell").symbol())
            .collect();
        let offset = column.iter().take_while(|symbol| **symbol == "#").count();
        let rest: Vec<String> = column[offset..].iter().map(|s| s.to_string()).collect();
        let slid: Vec<String> = (0..SIZE.1 - u16::try_from(offset).expect("a row"))
            .map(|y| old.cell((x, y)).expect("a cell").symbol().to_string())
            .collect();
        assert_eq!(
            rest, slid,
            "column {x} is not the old column slid down {offset}"
        );
    }
    assert!(any_old(&middle), "the old screen was gone at 300 ms");
    assert_ne!(middle, first, "nothing had fallen at 300 ms");

    let mut last = new_screen(SIZE.0, SIZE.1);
    assert!(
        !passage.paint(&mut last, &theme, Duration::from_millis(300)),
        "the melt outlasted its 600 ms"
    );
    assert_eq!(
        last,
        new_screen(SIZE.0, SIZE.1),
        "the melt's end is not the new screen"
    );
}

/// Every column starts where it was and ends off the bottom, and how far down it has slid
/// only ever grows: a column never climbs back.
#[test]
fn a_columns_offset_only_ever_grows() {
    for width in [SIZE.0, 126] {
        let height = SIZE.1;
        for (x, delay) in delays(width).into_iter().enumerate() {
            let offsets: Vec<u16> = (0..=MELT_MS / 10)
                .map(|at| fallen(delay, Duration::from_millis(u64::from(at * 10)), height))
                .collect();
            assert_eq!(
                offsets[0], 0,
                "column {x} of {width} had fallen at the start"
            );
            assert_eq!(
                offsets.last().copied(),
                Some(height),
                "column {x} of {width} was not gone at the end"
            );
            assert!(
                offsets.windows(2).all(|pair| pair[0] <= pair[1]),
                "column {x} of {width} climbed back: {offsets:?}"
            );
        }
    }
}

/// The melt takes as long at 126 columns as at 80: the first column starts falling at
/// once and the last 150 ms in at either width, so each is still showing some of the old
/// screen 30 ms short of its 600 ms, and over at 600 ms.
#[test]
fn the_melt_takes_as_long_at_126_columns_as_at_80() {
    let theme = true_colour();
    for width in [SIZE.0, 126] {
        let starts = delays(width);
        assert_eq!(
            starts.iter().min(),
            Some(&0),
            "{width}: no column falls at once"
        );
        assert_eq!(
            starts.iter().max(),
            Some(&STAGGER_MS),
            "{width}: the last column does not start 150 ms in"
        );
        assert!(
            starts.windows(2).any(|pair| pair[0] != pair[1]),
            "{width}: every column falls together"
        );

        let mut passage = Passage::melt(old_screen(width, SIZE.1));
        passage.paint(&mut new_screen(width, SIZE.1), &theme, Duration::ZERO);
        let mut late = new_screen(width, SIZE.1);
        assert!(passage.paint(&mut late, &theme, Duration::from_millis(570)));
        assert!(
            any_old(&late),
            "{width}: the old screen was gone 30 ms early"
        );
        let mut done = new_screen(width, SIZE.1);
        assert!(
            !passage.paint(&mut done, &theme, Duration::from_millis(30)),
            "{width}: the melt outlasted 600 ms"
        );
        assert!(
            !any_old(&done),
            "{width}: the old screen outlasted the melt"
        );
    }
}

/// Nothing about the melt is random: two melts of the same old screen painted at the same
/// moments draw the very same frames.
#[test]
fn the_melt_is_the_same_every_time_it_plays() {
    let theme = true_colour();
    let play = || {
        let mut passage = Passage::melt(old_screen(SIZE.0, SIZE.1));
        [0, 150, 150, 150]
            .into_iter()
            .map(|ms| {
                let mut buf = new_screen(SIZE.0, SIZE.1);
                passage.paint(&mut buf, &theme, Duration::from_millis(ms));
                buf
            })
            .collect::<Vec<Buffer>>()
    };
    assert_eq!(play(), play());
}

/// A melt whose old screen is another size than the frame it would play over — the
/// terminal resized since that screen was shown — plays nothing and is over at once.
#[test]
fn a_melt_of_a_screen_another_size_plays_nothing() {
    let theme = true_colour();
    let mut passage = Passage::melt(old_screen(SIZE.0, SIZE.1));
    let mut wider = new_screen(SIZE.0 + 20, SIZE.1);
    assert!(!passage.paint(&mut wider, &theme, Duration::ZERO));
    assert!(
        !any_old(&wider),
        "the old screen was painted over another size"
    );
    assert!(passage.over());
}
