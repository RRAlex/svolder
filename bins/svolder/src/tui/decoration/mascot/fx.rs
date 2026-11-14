// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The squirrel's entrances and exits: seven ways of arriving and the matching ways of leaving,
//! each a function from how far along it is to the pixels of the landed pose.
//!
//! An animation works on the grid of pixel letters a pose gives (see
//! [`Pose::pixels_wide`](super::tricks::Pose::pixels_wide)), hides some of the squirrel's pixels,
//! moves the grid or draws a few more pixels of its own in letters the sheet already colours.
//! Nothing here reads a clock or draws a cell; the brain says how far along the animation is.
//! At full progress an entrance is the landed pose itself and an exit is empty.

use std::time::Duration;

use super::looks::{Costume, Season};
use super::tricks;
use super::{BOX_H, BOX_W};

/// One way of coming in, which has a matching way of going out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Entrance {
    /// The squirrel assembles from the feet up in glittering blue.
    Transporter,
    /// A rainbow beam falls and the squirrel opens out from it.
    Bifrost,
    /// Pixels flicker in out of white noise.
    Sparkle,
    /// An old television draws it, every other line first.
    Scanlines,
    /// A bolt strikes, the squirrel flashes white and flickers solid.
    Lightning,
    /// It falls from above and bounces; it leaves by jumping up and out.
    Drop,
    /// A longship brings it in from the right and sails off with it to the left.
    Longship,
}

impl Entrance {
    /// Every entrance, in the order the pick counts them.
    pub(crate) const ALL: [Entrance; 7] = [
        Entrance::Transporter,
        Entrance::Bifrost,
        Entrance::Sparkle,
        Entrance::Scanlines,
        Entrance::Lightning,
        Entrance::Drop,
        Entrance::Longship,
    ];

    /// How long the entrance, and the matching exit, takes.
    pub(crate) fn duration(self) -> Duration {
        Duration::from_millis(match self {
            Entrance::Transporter => 1400,
            Entrance::Bifrost => 1600,
            Entrance::Sparkle => 1200,
            Entrance::Scanlines => 1200,
            Entrance::Lightning => 1100,
            Entrance::Drop => 1100,
            Entrance::Longship => 1900,
        })
    }

    /// The entrance a costume or a day of the year leans toward, if it has one.
    pub(crate) fn favourite(costume: Costume, season: Option<Season>) -> Option<Entrance> {
        let by_season = season.and_then(|season| match season {
            Season::Pirate | Season::Pride => Some(Entrance::Longship),
            Season::Halloween => Some(Entrance::Lightning),
            Season::Alien | Season::Towel => Some(Entrance::Transporter),
            Season::Midsummer | Season::Judgment => Some(Entrance::Bifrost),
            Season::NewYear | Season::Independence | Season::Yule => Some(Entrance::Sparkle),
            Season::CatInfinity | Season::Pi => Some(Entrance::Scanlines),
            Season::Bastille | Season::Canada | Season::Groundhog => Some(Entrance::Drop),
            _ => None,
        });
        by_season.or(match costume {
            Costume::Neon => Some(Entrance::Scanlines),
            Costume::Ice => Some(Entrance::Sparkle),
            Costume::Red => Some(Entrance::Lightning),
            Costume::God => Some(Entrance::Bifrost),
            Costume::Amber => None,
        })
    }
}

/// An animation under way: which, with what noise, going in or out, and how far along.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Fx {
    pub kind: Entrance,
    /// Moves the noise, so two entrances of one kind do not look alike.
    pub seed: u32,
    /// Whether the squirrel is leaving.
    pub out: bool,
    /// `0.0..=1.0` of the animation's own length.
    pub progress: f64,
}

type Grid = Vec<Vec<Option<char>>>;

/// A number in `0.0..1.0` that depends only on the arguments.
fn noise(seed: u32, x: usize, y: usize, salt: u32) -> f64 {
    let mut h = u64::from(seed)
        ^ ((x as u64) << 20)
        ^ ((y as u64) << 40)
        ^ u64::from(salt).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    h = (h ^ (h >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    h = (h ^ (h >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    h ^= h >> 31;
    (h >> 11) as f64 / (1u64 << 53) as f64
}

/// The grid with `keep` deciding for each lit pixel what it becomes: itself, another letter or
/// nothing.
fn map_pixels(grid: Grid, mut keep: impl FnMut(usize, usize, char) -> Option<char>) -> Grid {
    grid.into_iter()
        .enumerate()
        .map(|(y, row)| {
            row.into_iter()
                .enumerate()
                .map(|(x, cell)| cell.and_then(|letter| keep(x, y, letter)))
                .collect()
        })
        .collect()
}

/// `grid` moved `dy` rows down, what leaves the grid dropped.
fn shifted_down(grid: &Grid, dy: i32) -> Grid {
    let width = grid.first().map_or(0, Vec::len);
    let mut out = vec![vec![None; width]; grid.len()];
    for (y, row) in grid.iter().enumerate() {
        let to = y as i32 + dy;
        if (0..grid.len() as i32).contains(&to) {
            out[to as usize].clone_from(row);
        }
    }
    out
}

/// A grid of `width` by [`BOX_H`] with nothing lit.
fn blank(width: usize) -> Grid {
    vec![vec![None; width]; BOX_H]
}

/// Light the cell at (`x`, `y`) with `letter` if it is in the grid.
fn plot(grid: &mut Grid, x: i64, y: i64, letter: char) {
    if let (Ok(x), Ok(y)) = (usize::try_from(x), usize::try_from(y))
        && let Some(cell) = grid.get_mut(y).and_then(|row| row.get_mut(x))
    {
        *cell = Some(letter);
    }
}

/// `pose`, the landed squirrel's grid, as the animation `fx` shows it.
pub(crate) fn apply(fx: &Fx, pose: Grid) -> Grid {
    let width = pose.first().map_or(BOX_W, Vec::len);
    // The box is the grid's rightmost BOX_W columns.
    let left = width.saturating_sub(BOX_W);
    match (fx.kind, fx.out) {
        (Entrance::Drop, false) => dropped(pose, fx.progress),
        (Entrance::Drop, true) => {
            let t = fx.progress.clamp(0.0, 1.0);
            shifted_down(&pose, -((t * t * (BOX_H as f64 + 1.0)).round() as i32))
        }
        (Entrance::Longship, out) => ship(pose, fx, out),
        (kind, out) => {
            let p = if out { 1.0 - fx.progress } else { fx.progress }.clamp(0.0, 1.0);
            if p >= 1.0 {
                return pose;
            }
            let tick = (p * 36.0) as usize;
            let seed = fx.seed;
            match kind {
                Entrance::Transporter => transporter(pose, p, seed, tick),
                Entrance::Bifrost => bifrost(pose, p, left, tick),
                Entrance::Sparkle => sparkle(pose, p, seed, left, tick),
                Entrance::Scanlines => scanlines(pose, p),
                Entrance::Lightning => lightning(pose, p, seed, left, tick),
                Entrance::Drop | Entrance::Longship => pose,
            }
        }
    }
}

/// Assembled from the feet up: a pixel glitters pale then blue, then settles.
fn transporter(pose: Grid, p: f64, seed: u32, tick: usize) -> Grid {
    map_pixels(pose, |x, y, letter| {
        let low = 1.0 - y as f64 / BOX_H as f64;
        let due = 0.8 * (0.65 * low + 0.35 * noise(seed, x, y, 1));
        if p >= due + 0.15 {
            Some(letter)
        } else if p >= due {
            Some(if (tick + x + y).is_multiple_of(2) {
                'I'
            } else {
                'B'
            })
        } else {
            None
        }
    })
}

/// A rainbow beam down the middle of the box, the squirrel opening out from it.
fn bifrost(pose: Grid, p: f64, left: usize, tick: usize) -> Grid {
    let centre = left as i64 + (BOX_W / 2) as i64;
    let reach = (p - 0.25).max(0.0) / 0.65 * (BOX_W / 2 + 1) as f64;
    let mut out = map_pixels(pose, |x, _, letter| {
        (p > 0.25 && (x as i64 - centre).abs() as f64 <= reach).then_some(letter)
    });
    if p < 0.9 {
        let half = if p < 0.3 {
            1.0 + p / 0.3
        } else {
            2.0 * (0.9 - p) / 0.6
        };
        let fall = if p < 0.25 {
            ((p / 0.25) * BOX_H as f64).ceil() as i64
        } else {
            BOX_H as i64
        };
        for y in 0..fall.min(BOX_H as i64) {
            let letter = ['1', '2', '3', '4', '5'][(y as usize + tick / 2) % 5];
            for dx in -(half as i64)..=(half as i64) {
                plot(&mut out, centre + dx, y, letter);
            }
        }
    }
    out
}

/// Pixels flicker in at random moments; stray stars thin out around them.
fn sparkle(pose: Grid, p: f64, seed: u32, left: usize, tick: usize) -> Grid {
    let lit: Vec<Vec<bool>> = pose
        .iter()
        .map(|row| row.iter().map(Option::is_some).collect())
        .collect();
    let mut out = map_pixels(pose, |x, y, letter| {
        let due = 0.85 * noise(seed, x, y, 2);
        if p >= due + 0.1 {
            Some(letter)
        } else if p >= due {
            Some(if (tick + x).is_multiple_of(2) {
                'W'
            } else {
                'V'
            })
        } else {
            None
        }
    });
    // Stars come and go around the squirrel: none at either end of the animation.
    let density = 0.07 * 4.0 * p * (1.0 - p);
    for (y, row) in out.iter_mut().enumerate() {
        for (x, cell) in row.iter_mut().enumerate().skip(left) {
            if cell.is_none() && !lit[y][x] && noise(seed, x, y, 100 + tick as u32) < density {
                *cell = Some('W');
            }
        }
    }
    out
}

/// A television drawing it: the even lines come first, then the odd, a bright line sweeping.
fn scanlines(pose: Grid, p: f64) -> Grid {
    let sweep = ((p / 0.5) * BOX_H as f64) as usize % BOX_H;
    map_pixels(pose, |_, y, letter| {
        let row = y as f64 / BOX_H as f64;
        let due = if y % 2 == 0 {
            0.45 * row
        } else {
            0.5 + 0.45 * row
        };
        if p >= due + 0.05 {
            Some(letter)
        } else if p >= due {
            Some('B')
        } else if p < 0.5 && y == sweep {
            Some('I')
        } else {
            None
        }
    })
}

/// A bolt down the box, a white flash, a flicker, then solid.
fn lightning(pose: Grid, p: f64, seed: u32, left: usize, tick: usize) -> Grid {
    let centre = left as i64 + (BOX_W / 2) as i64;
    let mut out = if p < 0.35 {
        blank(pose.first().map_or(BOX_W, Vec::len))
    } else if p < 0.5 {
        map_pixels(pose, |_, _, _| Some('I'))
    } else if tick.is_multiple_of(2) {
        pose
    } else {
        blank(pose.first().map_or(BOX_W, Vec::len))
    };
    if p < 0.45 {
        let reach = ((p / 0.3) * BOX_H as f64).ceil().min(BOX_H as f64) as i64;
        for y in 0..reach {
            let jitter = (noise(seed, y as usize, 0, 3) * 5.0) as i64 - 2;
            plot(&mut out, centre + jitter, y, 'I');
            plot(&mut out, centre + jitter + 1, y, 'V');
        }
    }
    out
}

/// Falling from above, a high bounce, a low one, and still.
fn dropped(pose: Grid, t: f64) -> Grid {
    let t = t.clamp(0.0, 1.0);
    let arc = |s: f64, height: f64| (4.0 * s * (1.0 - s) * height).round() as i32;
    let dy = if t < 0.55 {
        let s = t / 0.55;
        -(((1.0 - s * s) * (BOX_H as f64 + 1.0)).round() as i32)
    } else if t < 0.8 {
        -arc((t - 0.55) / 0.25, 3.0)
    } else if t < 0.92 {
        -arc((t - 0.8) / 0.12, 1.0)
    } else {
        0
    };
    shifted_down(&pose, dy)
}

/// A longship, which carries its own rower: in, it sails in from the right and sinks as the
/// squirrel takes its place; out, it rises, takes the squirrel's place and sails off to the left.
fn ship(pose: Grid, fx: &Fx, out: bool) -> Grid {
    let width = pose.first().map_or(BOX_W, Vec::len);
    let t = fx.progress.clamp(0.0, 1.0);
    let step = (t * 40.0) as usize;
    let hull = |x: i32, dy: i32| {
        let mut frame = tricks::ship(step, x).pose;
        frame.dy = dy;
        frame.pixels_wide(width)
    };
    let rows = BOX_H as f64;
    if !out {
        if t < 0.7 {
            hull((28.0 * (1.0 - t / 0.7)).round() as i32, 0)
        } else if t < 0.8 {
            hull(0, ((t - 0.7) / 0.1 * rows).round() as i32)
        } else {
            pose
        }
    } else if t < 0.06 {
        pose
    } else if t < 0.18 {
        hull(0, (rows - (t - 0.06) / 0.12 * rows).round() as i32)
    } else if t < 1.0 {
        hull(-(((t - 0.18) / 0.82 * width as f64).round() as i32), 0)
    } else {
        blank(width)
    }
}
