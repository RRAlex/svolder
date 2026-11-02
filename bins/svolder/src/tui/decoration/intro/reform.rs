// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Particles exit, painted over the loop's first frames:
//! - The large mark breaks back into sparks along the same 30° front.
//! - Each letter flies, as one swarm, into its own letter of the header's logo.
//! - Each logo cell stays hidden until its first spark lands, glows while its sparks land, and
//!   cools to its own colour.
//!
//! The exit is planned on its first frame, from the logo that frame drew. Each frame is a
//! function of the time since then.

use std::time::Duration;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier};

use crate::tui::palette::rgb_of;
use crate::tui::render::Logo;
use crate::tui::theme::Rgb;

use super::pace::Pacer;
use super::rng::Rng;
use super::sparks::{
    Colours, Dots, FLASH, FLASH_HEAT, MarkCell, Path, centre, colour, mark_cells, mix, norm, span,
};

/// How long the panes under the exit wait before their arrival trace starts, in milliseconds.
pub(in crate::tui) const PANES_HELD_MS: u32 = 350;
/// How long the front takes to break the whole mark up, in seconds.
const DEPART_SPREAD: f32 = 0.3;
/// How long a flight into the logo lasts.
const FLIGHT: f32 = 0.55;
/// How far a flight's length strays, either way.
const FLIGHT_JITTER: f32 = 0.08;
/// How much heat a spark loses on its way.
const COOL: f32 = 0.5;
/// What the exit's seed is salted with, so its draws are not the build's over again.
const SALT: u64 = 0x0E81_7000_0000_0001;

/// The exit, handed from the intro to the animation layer.
#[derive(Debug)]
pub(in crate::tui) struct Reform {
    /// The frame size the intro finished at; a frame of any other size ends the exit.
    area: Rect,
    seed: u64,
    /// The intro's own colours, which the large mark and the sparks keep.
    colours: Colours,
    /// The pace its frames are drawn at, carried over from the intro.
    pacer: Pacer,
    /// Seconds since its first frame.
    elapsed: f32,
    /// What flies where, once its first frame has shown where the logo is.
    plan: Option<Plan>,
    /// Whether it has ended: played out, or ended early.
    gone: bool,
}

/// What flies where.
#[derive(Debug)]
struct Plan {
    flyers: Vec<Flyer>,
    targets: Vec<Target>,
    /// When the last logo cell has cooled.
    end: f32,
}

/// One cell of the large mark on its way to the logo.
#[derive(Debug)]
struct Flyer {
    at: (u16, u16),
    glyph: char,
    rgb: Rgb,
    #[cfg_attr(not(test), allow(dead_code, reason = "only the tests read it"))]
    letter: usize,
    /// The index of the target it lands on.
    #[cfg_attr(not(test), allow(dead_code, reason = "only the tests read it"))]
    target: usize,
    depart: f32,
    flight: f32,
    path: Path,
}

/// One cell of the header's logo.
#[derive(Debug)]
struct Target {
    at: (u16, u16),
    letter: usize,
    /// When its first spark lands, and its last.
    first: f32,
    last: f32,
}

impl Reform {
    /// The exit for a frame `area`, drawn from `seed` in `colours` at `pacer`'s pace.
    pub(super) fn new(area: Rect, seed: u64, colours: Colours, pacer: Pacer) -> Reform {
        Reform {
            area,
            seed,
            colours,
            pacer,
            elapsed: 0.0,
            plan: None,
            gone: false,
        }
    }

    /// The exit at sixty frames a second, in `theme`'s colours, for a case to drive.
    #[cfg(test)]
    pub(in crate::tui) fn for_test(
        area: Rect,
        seed: u64,
        theme: &crate::tui::theme::Theme,
    ) -> Reform {
        Reform::new(area, seed, Colours::of(theme), Pacer::new(false))
    }

    /// How long one of its frames waits for the next.
    pub(in crate::tui) fn period(&self) -> Duration {
        self.pacer.period()
    }

    /// Take what the frame just drawn cost.
    pub(in crate::tui) fn record(&mut self, cost: Duration) {
        self.pacer.record(cost);
    }

    /// Move its clock on by `by`.
    pub(in crate::tui) fn advance(&mut self, by: Duration) {
        self.elapsed += by.as_secs_f32();
    }

    /// Whether the panes under it still wait for their arrival.
    pub(in crate::tui) fn holding_panes(&self) -> bool {
        !self.gone && self.elapsed_ms() < PANES_HELD_MS
    }

    /// Whether it has more to paint.
    pub(in crate::tui) fn playing(&self) -> bool {
        !self.gone
            && self
                .plan
                .as_ref()
                .is_none_or(|plan| self.elapsed < plan.end)
    }

    /// Whether its first frame has planned it.
    pub(in crate::tui) fn planned(&self) -> bool {
        self.plan.is_some()
    }

    /// Milliseconds since its first frame.
    pub(in crate::tui) fn elapsed_ms(&self) -> u32 {
        (self.elapsed * 1000.0) as u32
    }

    /// Milliseconds until it has played out; 0 before it is planned and once it has.
    pub(in crate::tui) fn remaining_ms(&self) -> u32 {
        self.plan.as_ref().map_or(0, |plan| {
            ((plan.end - self.elapsed).max(0.0) * 1000.0).ceil() as u32
        })
    }

    /// Paint the exit over `buf`, the loop's frame as the screen drew it, its header's logo at
    /// `logo`, the frames in `held` drawn as bare `pane` grey while the panes are held.
    ///
    /// A frame of another size, or one with no logo to fly to, ends the exit and is left as
    /// drawn, and so is every frame once it has played out.
    pub(in crate::tui) fn paint(
        &mut self,
        buf: &mut Buffer,
        logo: Option<Logo>,
        held: &[Rect],
        pane: Color,
    ) {
        if self.gone {
            return;
        }
        if buf.area != self.area {
            self.gone = true;
            return;
        }
        if self.plan.is_some() && logo.is_none() {
            self.gone = true;
            return;
        }
        if self.plan.is_none() {
            self.plan =
                logo.and_then(|logo| Plan::new(self.area, self.seed, &self.colours, buf, logo));
        }
        let t = self.elapsed;
        let Some(plan) = self.plan.as_ref().filter(|plan| t < plan.end) else {
            self.gone = true;
            return;
        };
        if self.elapsed_ms() < PANES_HELD_MS {
            for rect in held {
                let rect = rect.intersection(buf.area);
                for y in rect.top()..rect.bottom() {
                    for x in rect.left()..rect.right() {
                        if let Some(cell) = buf.cell_mut((x, y)) {
                            cell.set_fg(pane);
                        }
                    }
                }
            }
        }
        for target in &plan.targets {
            let Some(cell) = buf.cell_mut(target.at) else {
                continue;
            };
            if t < target.first {
                cell.set_symbol(" ");
                continue;
            }
            let heat = match t < target.last {
                true => FLASH_HEAT,
                false => FLASH_HEAT * (1.0 - (t - target.last) / FLASH).max(0.0),
            };
            cell.set_fg(colour(mix(rgb_of(cell.fg), self.colours.hot, heat)));
        }
        let mut dots = Dots::new(self.area);
        for flyer in &plan.flyers {
            let tau = t - flyer.depart;
            if (0.0..flyer.flight).contains(&tau) {
                dots.fly(&flyer.path, tau, flyer.flight, COOL);
            }
        }
        dots.paint(buf, &self.colours);
        for flyer in plan.flyers.iter().filter(|flyer| t <= flyer.depart) {
            if let Some(cell) = buf.cell_mut(flyer.at) {
                cell.set_char(flyer.glyph);
                cell.set_fg(colour(flyer.rgb));
                cell.modifier = Modifier::empty();
            }
        }
    }
}

impl Plan {
    /// The exit over a frame `area` from `seed`, flying into `logo` as `buf` shows it; `None`
    /// where the frame has no room for the large mark or the logo has no cell.
    fn new(area: Rect, seed: u64, colours: &Colours, buf: &Buffer, logo: Logo) -> Option<Plan> {
        let mark = super::large_mark(area)?;
        let mut targets = targets(buf, logo);
        if targets.is_empty() {
            return None;
        }
        let cells = mark_cells();
        let (low, high) = span(cells.iter().map(|cell| cell.front));
        let letters = cells.iter().map(|cell| cell.letter + 1).max().unwrap_or(0);
        let mut rng = Rng::new(seed ^ SALT);
        let mut flyers = Vec::with_capacity(cells.len());
        for letter in 0..letters {
            let mut mine: Vec<&MarkCell> = cells.iter().filter(|c| c.letter == letter).collect();
            mine.sort_by_key(|cell| (cell.col, cell.row));
            let mut theirs: Vec<usize> = (0..targets.len())
                .filter(|&i| targets[i].letter == letter)
                .collect();
            if theirs.is_empty() {
                theirs = (0..targets.len()).collect();
            }
            theirs.sort_by_key(|&i| targets[i].at);
            for (i, cell) in mine.iter().enumerate() {
                let target = theirs[i * theirs.len() / mine.len()];
                let depart = norm(cell.front, low, high) * DEPART_SPREAD;
                let flight = FLIGHT + rng.signed() * FLIGHT_JITTER;
                let (x, y) = targets[target].at;
                let to = (f32::from(x) + 0.5, f32::from(y) + 0.5);
                let path = Path::curve(centre(mark, cell), to, &mut rng);
                let landing = depart + flight;
                targets[target].first = targets[target].first.min(landing);
                targets[target].last = targets[target].last.max(landing);
                flyers.push(Flyer {
                    at: (mark.x + cell.col, mark.y + cell.row),
                    glyph: cell.glyph,
                    rgb: colours.rows[usize::from(cell.row)],
                    letter,
                    target,
                    depart,
                    flight,
                    path,
                });
            }
        }
        let end = targets
            .iter()
            .map(|target| target.last)
            .filter(|last| last.is_finite())
            .fold(0.0, f32::max)
            + FLASH;
        Some(Plan {
            flyers,
            targets,
            end,
        })
    }
}

/// The logo's cells as `buf` shows them, each with its letter. The mark's letters are split by
/// the columns blank in all its rows. The word's letters are its cells, one each.
fn targets(buf: &Buffer, logo: Logo) -> Vec<Target> {
    let target = |at, letter| Target {
        at,
        letter,
        first: f32::INFINITY,
        last: f32::NEG_INFINITY,
    };
    match logo {
        Logo::Word(rect) => (rect.left()..rect.right())
            .map(|x| target((x, rect.y), usize::from(x - rect.x)))
            .collect(),
        Logo::Mark(rect) => {
            let mut out = Vec::new();
            let mut letter = 0;
            let mut inside = false;
            for x in rect.left()..rect.right() {
                let lit: Vec<u16> = (rect.top()..rect.bottom())
                    .filter(|&y| {
                        buf.cell((x, y))
                            .is_some_and(|c| !c.symbol().trim().is_empty())
                    })
                    .collect();
                if lit.is_empty() {
                    if inside {
                        letter += 1;
                        inside = false;
                    }
                    continue;
                }
                inside = true;
                out.extend(lit.into_iter().map(|y| target((x, y), letter)));
            }
            out
        }
    }
}

#[cfg(test)]
mod tests;
