// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The squirrel's tricks, states and dances as poses over time.
//!
//! A [`Pose`] is a list of sheet layers with offsets, a whole-sprite shift and a mirror flag.
//! A [`Trick`] is a one-shot: its pose follows the time since it started and stays on its last
//! frame after the end. A [`State`] loops for as long as it holds. Nothing here draws.

use std::time::Duration;

use super::looks::Dress;
use super::{BOX_H, BOX_W, ORIGIN, sheet};

/// One sheet layer in a pose, shifted from its own offset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Part {
    pub layer: &'static str,
    pub dx: i32,
    pub dy: i32,
}

/// What the squirrel looks like for one frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Pose {
    /// The layers, back to front.
    pub parts: Vec<Part>,
    /// Pixels the whole sprite sits right of home.
    pub dx: i32,
    /// Pixels the whole sprite sits below home.
    pub dy: i32,
    /// Whether the box is mirrored before the shift.
    pub flip: bool,
    /// Whether the pose may draw across the pane's inner width (the arrival and departure).
    pub wide: bool,
}

impl Pose {
    /// The letters of the pose in the box, one per pixel, clipped to it.
    #[cfg(test)]
    pub(crate) fn pixels(&self) -> [[Option<char>; BOX_W]; BOX_H] {
        let mut out = [[None; BOX_W]; BOX_H];
        for (y, row) in self.pixels_wide(BOX_W).into_iter().enumerate() {
            out[y].copy_from_slice(&row);
        }
        out
    }

    /// The letters of the pose in a box `width` pixels wide. The squirrel's own box is the
    /// rightmost [`BOX_W`] columns; the shift may carry the sprite left of it, and only `width`
    /// clips it.
    pub(crate) fn pixels_wide(&self, width: usize) -> Vec<Vec<Option<char>>> {
        let mut home = [[None; BOX_W]; BOX_H];
        for part in &self.parts {
            let layer = sheet::layer(part.layer);
            for (y, row) in layer.rows.iter().enumerate() {
                for (x, letter) in row.chars().enumerate() {
                    if letter == '.' {
                        continue;
                    }
                    let gx = layer.dx + part.dx + ORIGIN.0 + x as i32;
                    let gy = layer.dy + part.dy + ORIGIN.1 + y as i32;
                    if (0..BOX_W as i32).contains(&gx) && (0..BOX_H as i32).contains(&gy) {
                        home[gy as usize][gx as usize] = Some(letter);
                    }
                }
            }
        }
        if self.flip {
            home.iter_mut().for_each(|row| row.reverse());
        }
        let left = width as i32 - BOX_W as i32 + self.dx;
        let mut out = vec![vec![None; width]; BOX_H];
        for (y, row) in home.iter().enumerate() {
            for (x, letter) in row.iter().enumerate() {
                let (ox, oy) = (left + x as i32, y as i32 + self.dy);
                if letter.is_some()
                    && (0..width as i32).contains(&ox)
                    && (0..BOX_H as i32).contains(&oy)
                {
                    out[oy as usize][ox as usize] = *letter;
                }
            }
        }
        out
    }
}

/// A one-shot that plays from the moment its event fires.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Trick {
    Climb,
    Slide,
    BigSwing,
    Wake,
    Messenger,
    Palette,
    Hop,
    Stretch,
    /// The longship rows in from the left across the pane, then a hop.
    Arrive,
    /// A hop, then the longship rows out to the left across the pane.
    Depart,
    /// One pass of the longship out of the box to the left.
    RowOut,
    /// One pass of the longship into the box from the right, then a hop.
    RowIn,
}

/// A state that holds until something replaces it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum State {
    Rest,
    Music(Dance),
    Muted,
    Curled,
    Lantern,
    Rowing,
    Pointing,
}

/// How the squirrel moves to music.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Dance {
    /// Follows the level.
    Bob,
    Headbang,
    Bounce,
    Sway,
    Nod,
}

/// Where the music's beat stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Beat {
    /// The time since the last beat.
    pub since: Duration,
    /// How many beats there have been.
    pub count: u64,
}

/// One frame of a timeline.
struct Frame {
    ms: u64,
    pose: Pose,
}

/// The frame showing `elapsed` into `frames`, which loop when `looping` and otherwise hold the
/// last.
fn frame_at(frames: &[Frame], elapsed: Duration, looping: bool) -> &Pose {
    let total: u64 = frames.iter().map(|f| f.ms).sum();
    let mut at = elapsed.as_millis() as u64;
    if looping {
        at %= total;
    }
    for frame in frames {
        if at < frame.ms {
            return &frame.pose;
        }
        at -= frame.ms;
    }
    &frames[frames.len() - 1].pose
}

fn part(layer: &'static str) -> Part {
    Part {
        layer,
        dx: 0,
        dy: 0,
    }
}

fn part_at(layer: &'static str, dx: i32, dy: i32) -> Part {
    Part { layer, dx, dy }
}

fn parts(layers: &[&'static str]) -> Vec<Part> {
    layers.iter().map(|l| part(l)).collect()
}

/// `parts` moved `dy` pixels down.
fn lift(parts: Vec<Part>, dy: i32) -> Vec<Part> {
    parts
        .into_iter()
        .map(|p| Part { dy: p.dy + dy, ..p })
        .collect()
}

/// The head, ears, `extra` layers and `hat`, `dy` pixels down: a head that moves takes its hat.
fn head(dy: i32, extra: &[&'static str], hat: Option<&'static str>) -> Vec<Part> {
    let mut layers = vec!["head", "ears"];
    layers.extend(extra);
    layers.extend(hat);
    lift(parts(&layers), dy)
}

/// The whole squirrel sitting: body, head, `extra` layers, the dress's hat and `carry`.
fn sit(dress: &Dress, extra: &[&'static str], carry: &[&'static str]) -> Vec<Part> {
    let mut out = parts(&["body"]);
    out.extend(head(0, extra, dress.hat));
    out.extend(parts(carry));
    out
}

/// The worn layers that sit on the face and so move with the head.
const HEAD_WORN: [&str; 4] = ["shades", "shades2", "eyepatch", "eyepatch2"];

/// The dress's worn layers: those on the face take the head's offset in `layers`, the rest stay
/// with the body.
fn worn(layers: &[Part], dress: &Dress) -> Vec<Part> {
    let (dx, dy) = layers
        .iter()
        .find(|p| p.layer == "head")
        .map_or((0, 0), |p| (p.dx, p.dy));
    dress
        .wear
        .iter()
        .map(|&layer| {
            if HEAD_WORN.contains(&layer) {
                part_at(layer, dx, dy)
            } else {
                part(layer)
            }
        })
        .collect()
}

/// A frame of `ms` showing `layers` with the dress's worn layers on top.
fn dressed(ms: u64, mut layers: Vec<Part>, dress: &Dress) -> Frame {
    layers.extend(worn(&layers, dress));
    Frame {
        ms,
        pose: still(layers),
    }
}

/// A frame of `ms` showing `layers` and nothing worn.
fn bare(ms: u64, layers: Vec<Part>) -> Frame {
    Frame {
        ms,
        pose: still(layers),
    }
}

fn still(parts: Vec<Part>) -> Pose {
    Pose {
        parts,
        dx: 0,
        dy: 0,
        flip: false,
        wide: false,
    }
}

impl Frame {
    fn shifted(mut self, dx: i32, dy: i32) -> Frame {
        self.pose.dx = dx;
        self.pose.dy = dy;
        self
    }

    fn flipped(mut self) -> Frame {
        self.pose.flip = true;
        self
    }

    fn wide(mut self) -> Frame {
        self.pose.wide = true;
        self
    }
}

/// The longship at `x`, rowing: its two frames alternate with `step`.
fn ship(step: usize, x: i32) -> Frame {
    let layers = if step % 2 == 1 {
        vec![
            part_at("ship_head", 0, 1),
            part_at("ship", 0, 1),
            part_at("oar2", 0, 1),
            part("wake2"),
        ]
    } else {
        parts(&["ship_head", "ship", "oar1", "wake1"])
    };
    bare(SHIP_STEP_MS, layers).shifted(x, 0)
}

/// How long one pass step of the longship across the box or the pane holds: two pixels every
/// 99 ms, an unhurried row of about twenty pixels a second.
const SHIP_STEP_MS: u64 = 99;

/// The longship from `from` to `to`, 2 pixels a step.
fn ship_pass(from: i32, to: i32) -> Vec<Frame> {
    let sign = if to >= from { 1 } else { -1 };
    let steps = (to - from).abs() / 2;
    (0..=steps)
        .map(|i| ship(i as usize, from + sign * 2 * i))
        .collect()
}

/// The hop: two jumps with happy eyes.
fn hop(dress: &Dress) -> Vec<Frame> {
    let happy = || sit(dress, &["eyes_happy"], &dress.carry);
    vec![
        dressed(140, happy(), dress).shifted(0, -2),
        dressed(120, happy(), dress),
        dressed(140, happy(), dress).shifted(0, -2),
        dressed(300, happy(), dress),
    ]
}

impl Trick {
    /// The frames of the trick, `dress` worn.
    fn frames(self, dress: &Dress) -> Vec<Frame> {
        let none: &[&'static str] = &[];
        match self {
            Trick::Climb => {
                let reach = || sit(dress, &["eyes_up", "arms_up"], none);
                let mut top = parts(&["body"]);
                top.extend(head(-1, &["eyes_up"], dress.hat));
                top.push(part_at("arms_up", 0, -1));
                vec![
                    dressed(200, reach(), dress),
                    dressed(500, top, dress),
                    dressed(200, reach(), dress),
                ]
            }
            Trick::Slide => {
                let mut duck = parts(&["body"]);
                duck.extend(head(1, &["eyes_down"], dress.hat));
                duck.extend(parts(&dress.carry));
                vec![dressed(700, duck, dress)]
            }
            Trick::BigSwing => {
                let blown = |dy: i32| {
                    let mut layers = parts(&["body", "head", "ears", "mouth_open"]);
                    layers.extend(dress.hat.map(|hat| part_at(hat, 1, dy)));
                    layers.extend(parts(&dress.carry));
                    layers
                };
                vec![
                    dressed(500, blown(-1), dress),
                    dressed(300, blown(-2), dress),
                ]
            }
            Trick::Wake => {
                let start = |dy: i32, extra: &[&'static str]| {
                    let mut layers = parts(&["body"]);
                    layers.extend(head(dy, extra, dress.hat));
                    layers.push(part("exclaim"));
                    layers.extend(parts(&dress.carry));
                    layers
                };
                vec![
                    dressed(180, start(-1, &["mouth_open"]), dress),
                    dressed(180, start(-2, &["mouth_open"]), dress),
                    dressed(300, start(-1, &[]), dress),
                ]
            }
            Trick::Messenger => {
                let mut frames = Vec::new();
                for x in (0..=28).step_by(2) {
                    let body = if x % 4 == 0 { "body_run1" } else { "body_run2" };
                    let mut layers = parts(&[body, "head", "ears", "eyes_right"]);
                    layers.extend(dress.hat.map(part));
                    frames.push(dressed(33, layers, dress).shifted(x, 0).flipped());
                }
                frames.push(bare(400, Vec::new()));
                for x in (0..=28).rev().step_by(2) {
                    let body = if x % 4 == 0 { "body_run1" } else { "body_run2" };
                    let mut layers = parts(&[body, "head", "ears", "eyes_left"]);
                    layers.extend(dress.hat.map(part));
                    layers.push(part("scroll1"));
                    frames.push(dressed(33, layers, dress).shifted(x, 0));
                }
                for _ in 0..3 {
                    for scroll in ["scroll1", "scroll2", "scroll3", "scroll4"] {
                        frames.push(dressed(180, sit(dress, none, &[scroll]), dress));
                    }
                }
                frames
            }
            Trick::Palette => {
                let mut frames = Vec::new();
                for _ in 0..4 {
                    for palette in ["palette1", "palette2", "palette3", "palette4"] {
                        frames.push(dressed(180, sit(dress, none, &[palette]), dress));
                    }
                }
                frames
            }
            Trick::Hop => hop(dress),
            Trick::Stretch => {
                let mut yawn = parts(&["body"]);
                yawn.extend(head(-1, &["eyes_closed"], dress.hat));
                yawn.push(part_at("arms_up", 0, -1));
                let mut wide = parts(&["body"]);
                wide.extend(head(-2, &["eyes_closed", "mouth_open"], dress.hat));
                wide.push(part_at("arms_up", 0, -2));
                let mut settle = parts(&["body"]);
                settle.extend(head(-1, &["eyes_lid"], dress.hat));
                vec![
                    dressed(250, yawn, dress),
                    dressed(1100, wide, dress),
                    dressed(300, settle, dress),
                ]
            }
            Trick::Arrive => {
                // The prow is on the sheet's left, so a ship sailing right is mirrored.
                let mut frames: Vec<Frame> = ship_pass(-28, 0)
                    .into_iter()
                    .map(|f| f.flipped().wide())
                    .collect();
                frames.extend(hop(dress).into_iter().map(Frame::wide));
                frames
            }
            Trick::Depart => {
                let mut frames: Vec<Frame> = hop(dress).into_iter().map(Frame::wide).collect();
                frames.extend(ship_pass(0, -28).into_iter().map(Frame::wide));
                frames
            }
            Trick::RowOut => ship_pass(0, -28),
            Trick::RowIn => {
                let mut frames = ship_pass(28, 0);
                frames.extend(hop(dress));
                frames
            }
        }
    }

    /// How long the trick plays.
    pub(crate) fn duration(self) -> Duration {
        let dress = super::looks::dress(super::looks::Costume::Amber, None, Duration::ZERO);
        let ms = self.frames(&dress).iter().map(|f| f.ms).sum();
        Duration::from_millis(ms)
    }

    /// The pose `elapsed` into the trick; past the end it stays on the last frame.
    pub(crate) fn pose(self, elapsed: Duration, dress: &Dress) -> Pose {
        frame_at(&self.frames(dress), elapsed, false).clone()
    }
}

/// Rest: 2600 still, a 250 blink, 1800 still, a tail flick, a pause, a flick, 1200 still.
fn rest(dress: &Dress) -> Vec<Frame> {
    let look = |body: &'static str, closed: bool| {
        let mut layers = vec![body, "head", "ears"];
        if closed {
            layers.push("eyes_closed");
        } else {
            layers.extend(dress.eyes);
        }
        layers.extend(dress.hat);
        let mut parts = parts(&layers);
        parts.extend(self::parts(&dress.carry));
        parts
    };
    vec![
        dressed(2600, look("body", false), dress),
        dressed(250, look("body", true), dress),
        dressed(1800, look("body", false), dress),
        dressed(250, look("body_flick", false), dress),
        dressed(250, look("body", false), dress),
        dressed(250, look("body_flick", false), dress),
        dressed(1200, look("body", false), dress),
    ]
}

/// A sleeper's z's drifting up, `ms` a frame, over `base`.
fn sleep(
    dress: Option<&Dress>,
    base: &[&'static str],
    extra: &[&'static str],
    ms: u64,
) -> Vec<Frame> {
    let mut frames = Vec::new();
    for (dx, dy) in [(0, 0), (-1, -1), (-2, -2)] {
        let mut layers = match dress {
            Some(d) => sit(d, extra, &d.carry),
            None => Vec::new(),
        };
        layers.extend(parts(base));
        layers.push(part_at("zz_small", dx, dy));
        frames.push(match dress {
            Some(d) => dressed(ms, layers, d),
            None => bare(ms, layers),
        });
    }
    let mut layers = match dress {
        Some(d) => sit(d, extra, &d.carry),
        None => Vec::new(),
    };
    layers.extend(parts(base));
    frames.push(match dress {
        Some(d) => dressed(ms, layers, d),
        None => bare(ms, layers),
    });
    frames
}

/// The music's pose: headphones for the hat, the carry kept, the horn's waves on the up-beat.
/// On the hit (the first part of a beat) the dance moves; after it the squirrel is up.
fn music(dance: Dance, beat: Beat, dress: &Dress) -> Pose {
    let since = beat.since.as_millis();
    // (hit window in ms, head dx, head dy, carry dy, whole-sprite dy). A window is at least a
    // quarter second, so a frame every 250 ms always meets a hit.
    let (window, hx, hy, cy, wy) = match dance {
        Dance::Bob => (300, 0, 1, 1, 0),
        Dance::Headbang => (250, 0, 2, 1, 0),
        Dance::Bounce => (250, 0, 0, 0, -1),
        Dance::Sway => (400, if beat.count % 2 == 1 { -1 } else { 1 }, 0, 0, 0),
        Dance::Nod => (300, 0, 1, 1, 0),
    };
    let hit = since < window;
    let mut layers = parts(&["body"]);
    let mut moved = head(
        if hit { hy } else { 0 },
        dress.eyes.as_slice(),
        Some("phones"),
    );
    if hit {
        moved.iter_mut().for_each(|p| p.dx += hx);
    }
    layers.extend(moved);
    layers.extend(lift(parts(&dress.carry), if hit { cy } else { 0 }));
    if !hit && dress.carry.contains(&"horn") {
        layers.push(part("waves"));
    }
    layers.extend(worn(&layers, dress));
    let mut pose = still(layers);
    if hit {
        pose.dy = wy;
    }
    pose
}

impl State {
    /// The pose `elapsed` into the state, which loops. `beat` is read only by the music.
    pub(crate) fn pose(self, elapsed: Duration, beat: Beat, dress: &Dress) -> Pose {
        let none: &[&'static str] = &[];
        match self {
            State::Rest => frame_at(&rest(dress), elapsed, true).clone(),
            State::Music(dance) => music(dance, beat, dress),
            State::Muted => frame_at(
                &sleep(Some(dress), &[], &["eyes_closed", "paws_ears"], 600),
                elapsed,
                true,
            )
            .clone(),
            State::Curled => frame_at(&sleep(None, &["curl"], none, 700), elapsed, true).clone(),
            State::Lantern => {
                let watch = |eyes: &[&'static str], lantern: &'static str| {
                    dressed(400, sit(dress, eyes, &[lantern]), dress)
                };
                let frames = vec![
                    watch(&["eyes_left"], "lantern1"),
                    watch(&["eyes_left"], "lantern2"),
                    watch(&["eyes_left"], "lantern1"),
                    watch(&[], "lantern2"),
                    watch(&["eyes_right"], "lantern1"),
                    watch(&[], "lantern2"),
                ];
                frame_at(&frames, elapsed, true).clone()
            }
            State::Rowing => {
                const LOOP: u64 = 55;
                let step = elapsed.as_millis() as u64 / 250 % LOOP;
                ship(step as usize, 27 - step as i32).pose
            }
            State::Pointing => {
                let look = |extra: &[&'static str]| dressed(0, sit(dress, extra, none), dress);
                let mut shout = look(&["eyes_downleft", "mouth_open", "point", "exclaim"]);
                shout.ms = 500;
                let mut calm = look(&["eyes_downleft", "mouth_open", "point"]);
                calm.ms = 300;
                frame_at(&[shout, calm], elapsed, true).clone()
            }
        }
    }
}
