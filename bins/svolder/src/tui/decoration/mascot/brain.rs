// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The squirrel's brain: what it is doing and where, from the views it is shown and the codes it
//! is typed.
//!
//! Every method takes the instant it is told about, so the brain reads no clock. A called
//! squirrel sits at home on the device screen and plays what happens; a hidden one comes out for
//! a visit of about six seconds and goes. Off its home screen a called squirrel visits for the
//! same events a hidden one does.

use std::time::{Duration, Instant};

use svolder_host::players::Status;

use super::codes::Codes;
use super::fx::{Entrance, Fx};
use super::looks::{self, Costume, Paint, Season};
use super::tricks::{Beat, Dance, State, Trick};
use super::{Code, Link, Place, Showing, View, dance_of};

/// How long a visit lasts, from the moment it began.
const VISIT: Duration = Duration::from_secs(6);
/// How often the music's beat falls until the level drives it.
const BEAT: Duration = Duration::from_millis(500);
/// The time constant of the slow average the level is compared with.
const AVERAGE: Duration = Duration::from_secs(2);
/// A level counts as a beat above this multiple of its slow average.
const SURGE: f64 = 1.3;
/// A level at or below this is never a beat.
const FLOOR: f64 = 0.05;
/// The least time between two beats.
const GAP: Duration = Duration::from_millis(200);
/// How long the level's own beat holds after the last level before the steady beat takes over.
const BEAT_HOLD: Duration = Duration::from_secs(1);
/// How long a session lasts for each hour the squirrel marks.
const HOUR: Duration = Duration::from_secs(3600);
/// How long a standing squirrel stays between one zap out and the next.
const ZAP_EVERY: Duration = Duration::from_secs(30 * 60);
/// How long the squirrel is away in a zap, between the end of its exit and the start of its
/// entrance.
const ZAP_GAP: Duration = Duration::from_millis(1500);
/// A gap between two observations longer than this means the session was suspended: a zap that
/// fell due in it is not made up.
const STALL: Duration = Duration::from_secs(120);
/// The most things a held box makes wait: more than this are never typed or happen in a row.
const WAITING: usize = 8;

/// The beats found in the level: it follows an exponential average of the level (time constant
/// [`AVERAGE`], over the instants it is fed) and a beat is the level above [`SURGE`] times that
/// average and above [`FLOOR`], at most one per [`GAP`].
#[derive(Debug, Clone, Default)]
struct Pulse {
    /// The slow average, from the first level on.
    average: Option<f64>,
    /// When the average was last moved.
    at: Option<Instant>,
    /// When the last beat fell.
    last: Option<Instant>,
    /// How many beats have fallen.
    count: u64,
}

impl Pulse {
    /// Whether the last level is recent enough, as of `now`, for the level to drive the beat.
    fn live(&self, now: Instant) -> bool {
        self.average.is_some()
            && self
                .at
                .is_some_and(|at| now.saturating_duration_since(at) <= BEAT_HOLD)
    }

    /// Take in `level` seen at `now`. Without a level the tracker holds for [`BEAT_HOLD`] after
    /// the last one, then starts over.
    fn feed(&mut self, level: Option<f64>, now: Instant) {
        if !self.live(now) {
            *self = Pulse::default();
        }
        let Some(level) = level.filter(|level| level.is_finite()) else {
            return;
        };
        if let (Some(average), Some(at)) = (self.average, self.at) {
            if level > SURGE * average
                && level > FLOOR
                && self
                    .last
                    .is_none_or(|last| now.saturating_duration_since(last) >= GAP)
            {
                self.last = Some(now);
                self.count += 1;
            }
            let dt = now.saturating_duration_since(at).as_secs_f64();
            let weight = 1.0 - (-dt / AVERAGE.as_secs_f64()).exp();
            self.average = Some(average + (level - average) * weight);
        } else {
            self.average = Some(level);
        }
        self.at = Some(now);
    }
}

/// What a visit plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum What {
    Trick(Trick),
    State(State),
}

/// The animation a squirrel enters or leaves by: which, and the seed of its noise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Entry {
    kind: Entrance,
    seed: u32,
}

impl Entry {
    fn len(self) -> Duration {
        self.kind.duration()
    }
}

/// The visit in progress: what it plays, when it began and when it ends (never, for a fault that
/// stands). It begins with the squirrel's entrance, when it came from nowhere, and ends with its
/// exit, when it is a visitor.
#[derive(Debug, Clone, Copy)]
struct Visit {
    what: What,
    /// Whether it plays over the sticky state a called squirrel at home holds (a click's hop, a
    /// code's arrival or hop).
    over: bool,
    began: Instant,
    ends: Option<Instant>,
    entry: Option<Entry>,
    exit: Option<Entry>,
}

impl Visit {
    /// How long the entrance takes, before the visit's own play begins.
    fn lead(&self) -> Duration {
        self.entry.map_or(Duration::ZERO, Entry::len)
    }

    /// When the visit's own play begins.
    fn plays_from(&self) -> Instant {
        self.began + self.lead()
    }
}

/// What starts a visit.
struct Start {
    what: What,
    over: bool,
    /// Whether the squirrel leaves when the visit is over.
    visitor: bool,
    /// Whether the squirrel is on show already, so needs no entrance.
    shown: bool,
    /// How long the visit's own play lasts, when not the usual.
    body: Option<Duration>,
}

/// A standing squirrel's zap: out by an exit, away a moment, back in by an entrance.
#[derive(Debug, Clone, Copy)]
struct Zap {
    began: Instant,
    out: Entry,
    back: Entry,
    /// How long each takes; nothing when effects are off.
    out_len: Duration,
    back_len: Duration,
}

impl Zap {
    fn end(&self) -> Instant {
        self.began + self.out_len + ZAP_GAP + self.back_len
    }
}

/// What a held box made wait.
#[derive(Debug, Clone, Copy)]
enum Waiting {
    Event(Event),
    Code(Code),
}

/// What happened between two views.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Event {
    VolumeUp,
    VolumeDown,
    BigSwing,
    Unmuted,
    Input,
    Eq,
    LinkLost,
    Offline,
    LinkBack,
    Fault,
    PowerOff,
    PowerOn,
    Hour,
    Advice,
}

/// What the squirrel is showing: what it plays, since when and where.
struct Pick {
    what: What,
    began: Instant,
    place: Place,
    fx: Option<Fx>,
}

/// The squirrel's state, kept across a device switch (see [`Mascot::reopened`]).
#[derive(Debug, Clone)]
pub(crate) struct Mascot {
    called: bool,
    god: bool,
    /// When the session began, which the hours and the costume's colours count from.
    started: Instant,
    /// How many full hours have been marked.
    hours: u64,
    /// The last view, and so the baseline the next one is compared with.
    last: Option<View>,
    /// Where the link stood, which a reopen keeps.
    link: Option<Link>,
    /// Whether a forced visit took the place of a pointing the standing fault still asks for.
    /// A reopen keeps it; the first observation with no standing fault drops it.
    pointing_owed: bool,
    /// The sticky state a called squirrel at home holds, and since when.
    standing: Option<(State, Instant)>,
    visit: Option<Visit>,
    /// The beats the level has made.
    pulse: Pulse,
    /// The instant of the last observation.
    at: Instant,
    /// The month and day, from the local clock, which the season comes from.
    pub today: Option<(u8, u8)>,
    /// Whether the terminal can show the squirrel's colours.
    pub capable: bool,
    /// The name of the TUI's palette, which picks the costume.
    pub palette: String,
    /// The keys typed toward a code in the help overlay or the menu.
    pub codes: Codes,
    /// Whether entrances and exits are drawn; off, the squirrel appears and goes whole.
    pub effects: bool,
    /// Whether the hour has called the squirrel out once.
    hour_called: bool,
    /// How many entrances have been picked, which the pick's noise turns on.
    rolls: u64,
    /// The entrance last picked, which the next never repeats.
    last_entrance: Option<Entrance>,
    /// Whether a box had the keys at the last observation.
    held: bool,
    /// What happened or was typed while a box had the keys.
    waiting: Vec<Waiting>,
    /// The entrance every pick is made to be, for the pictures.
    #[cfg(test)]
    pub forced: Option<Entrance>,
    /// The zap in progress.
    zap: Option<Box<Zap>>,
    /// When the next zap falls due.
    zap_at: Option<Instant>,
}

impl Mascot {
    /// A hidden squirrel whose session began at `now`.
    pub(crate) fn new(now: Instant) -> Mascot {
        Mascot {
            called: false,
            god: false,
            started: now,
            hours: 0,
            last: None,
            link: None,
            standing: None,
            pointing_owed: false,
            visit: None,
            pulse: Pulse::default(),
            at: now,
            today: None,
            capable: false,
            palette: String::new(),
            codes: Codes::default(),
            effects: false,
            hour_called: false,
            rolls: 0,
            last_entrance: None,
            held: false,
            waiting: Vec::new(),
            #[cfg(test)]
            forced: None,
            zap: None,
            zap_at: None,
        }
    }

    /// The squirrel a device switch hands on: presence, god mode, the session start, the trick in
    /// progress and the link, but not the readings baseline, so the new device's first values
    /// fire nothing.
    pub(crate) fn reopened(mut self) -> Mascot {
        self.last = None;
        self.standing = None;
        self
    }

    /// Whether the squirrel has been called, for the tests.
    #[cfg(test)]
    pub(crate) fn called(&self) -> bool {
        self.called
    }

    /// Whether god mode is on, for the tests.
    #[cfg(test)]
    pub(crate) fn god(&self) -> bool {
        self.god
    }

    /// Take in `view`, seen at `now`. Returns the toast an hour turning asks for, when the
    /// squirrel is shown stretching.
    pub(crate) fn observe(&mut self, view: &View, now: Instant) -> Option<String> {
        let passed = now.saturating_duration_since(self.at);
        let stalled = passed > STALL;
        // An animation under way waits while a box has the keys: its clock stands still.
        if view.keys_held && (self.zap.is_some() || self.visit_fx().is_some()) {
            if let Some(zap) = self.zap.as_deref_mut() {
                zap.began += passed;
            }
            if let Some(visit) = self.visit.as_mut() {
                visit.began += passed;
                visit.ends = visit.ends.map(|ends| ends + passed);
            }
        }
        self.at = now;
        self.held = view.keys_held;
        self.pulse.feed(view.level, now);
        let hour = now.saturating_duration_since(self.started).as_secs() / HOUR.as_secs();
        let mut events = Vec::new();
        if hour > self.hours {
            self.hours = hour;
            events.push(Event::Hour);
        }
        if let Some(last) = &self.last {
            events.extend(Self::changes(last, view));
        }
        if let Some(link) = self.link
            && link != view.link
            && let Some(event) = match view.link {
                Link::Connected => Some(Event::LinkBack),
                Link::Reconnecting if link == Link::Connected => Some(Event::LinkLost),
                Link::Reconnecting => None,
                Link::Offline => Some(Event::Offline),
            }
        {
            events.push(event);
        }
        self.link = Some(view.link);
        self.last = Some(view.clone());
        self.standing = match (self.standing, Self::standing_of(view)) {
            (Some((held, since)), Some(next)) if held == next => Some((held, since)),
            (_, Some(next)) => Some((next, now)),
            (_, None) => None,
        };
        // A pointing that nothing stands behind any more is over.
        if let Some(visit) = self
            .visit
            .filter(|v| v.what == What::State(State::Pointing) && v.ends.is_none())
            && (!view.fault || view.advice_shown)
        {
            // A visitor that was pointing leaves by its exit; a pointing nobody entered for
            // is over at once.
            self.visit = match visit.exit {
                Some(exit) if !self.at_home() => Some(Visit {
                    ends: Some(now + exit.len()),
                    ..visit
                }),
                _ => None,
            };
        }
        if self
            .visit
            .is_some_and(|v| v.ends.is_some_and(|ends| now >= ends))
        {
            self.visit = None;
        }
        // A fault that still stands points again once what interrupted the pointing is over.
        let faulted = view.fault && !view.advice_shown;
        if !faulted {
            self.pointing_owed = false;
        } else if self.pointing_owed && self.visit.is_none() && !self.held {
            self.pointing_owed = false;
            self.visit_of(What::State(State::Pointing), now, true, false);
        }
        self.waiting.extend(events.into_iter().map(Waiting::Event));
        let mut hour_stretch = false;
        // A box that has the keys keeps everything waiting; the first turn without one plays it.
        if self.held {
            let over = self.waiting.len().saturating_sub(WAITING);
            self.waiting.drain(..over);
        } else {
            for waited in std::mem::take(&mut self.waiting) {
                match waited {
                    Waiting::Event(event) => {
                        self.play(event, view, now);
                        hour_stretch |= event == Event::Hour
                            && self
                                .visit
                                .is_some_and(|v| v.what == What::Trick(Trick::Stretch));
                    }
                    Waiting::Code(code) => self.code_now(code, now),
                }
            }
        }
        if stalled && self.zap_at.is_some_and(|at| now >= at) {
            self.zap_at = Some(now + ZAP_EVERY);
        }
        self.zaps(now);
        // The hour speaks only if its stretch is the one shown, begun at this observation.
        hour_stretch
            .then(|| self.pick())
            .flatten()
            .filter(|pick| pick.what == What::Trick(Trick::Stretch))
            .map(|_| format!("{hour} h in. Ratatoskr stretches."))
    }

    /// Start the zap that has fallen due, and close the one that is over.
    fn zaps(&mut self, now: Instant) {
        if let Some(zap) = self.zap.as_deref().copied()
            && now >= zap.end()
        {
            self.zap = None;
            self.zap_at = Some(zap.end() + ZAP_EVERY);
        }
        if self.zap.is_none()
            && self.called
            && !self.held
            && self.zap_at.is_some_and(|at| now >= at)
        {
            let out = self.roll(now, false);
            let back = self.roll(now, true);
            let len = |entry: Entry| {
                if self.effects {
                    entry.len()
                } else {
                    Duration::ZERO
                }
            };
            self.zap = Some(Box::new(Zap {
                began: now,
                out,
                back,
                out_len: len(out),
                back_len: len(back),
            }));
            self.zap_at = None;
        }
    }

    /// Pick an entrance or the exit that matches the one the visit is given, at `now`. An
    /// entrance leans toward the costume's own and never repeats the one before it.
    fn roll(&mut self, now: Instant, entrance: bool) -> Entry {
        self.rolls += 1;
        let elapsed = now.saturating_duration_since(self.started).as_millis() as u64;
        let mut h = self.rolls.wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ elapsed.rotate_left(21);
        h = (h ^ (h >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        h = (h ^ (h >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        h ^= h >> 31;
        let all = Entrance::ALL;
        let mut index = (h % all.len() as u64) as usize;
        if let Some(own) = Entrance::favourite(self.costume(), self.season())
            && (h >> 8).is_multiple_of(3)
        {
            index = all.iter().position(|kind| *kind == own).unwrap_or(index);
        }
        if entrance {
            if Some(all[index]) == self.last_entrance {
                index = (index + 1 + ((h >> 16) % (all.len() as u64 - 1)) as usize) % all.len();
            }
            self.last_entrance = Some(all[index]);
        }
        #[cfg(test)]
        if let Some(kind) = self.forced {
            index = all.iter().position(|each| *each == kind).unwrap_or(index);
        }
        Entry {
            kind: all[index],
            seed: (h >> 32) as u32,
        }
    }

    /// The costume the squirrel wears now.
    fn costume(&self) -> Costume {
        if self.god {
            Costume::God
        } else {
            Costume::of_palette(&self.palette)
        }
    }

    /// The season today falls in.
    fn season(&self) -> Option<Season> {
        self.today.and_then(|(month, day)| Season::on(month, day))
    }

    /// Whether the squirrel is called and on the screen it lives on.
    fn at_home(&self) -> bool {
        self.called && self.last.as_ref().is_none_or(|view| view.screen_is_home)
    }

    /// The events between `last` and `view`, apart from the link's.
    fn changes(last: &View, view: &View) -> Vec<Event> {
        let mut out = Vec::new();
        if let (Some((was, ..)), Some((now, min, max))) = (last.volume, view.volume)
            && now != was
        {
            let range = (max - min).max(0);
            let step = (now - was).abs();
            let big = range > 0 && step >= 2 && step * 10 >= range;
            out.push(match (big, now > was) {
                (true, _) => Event::BigSwing,
                (false, true) => Event::VolumeUp,
                (false, false) => Event::VolumeDown,
            });
        }
        if let (Some(was), Some(now)) = (last.muted, view.muted)
            && was
            && !now
        {
            out.push(Event::Unmuted);
        }
        if let (Some(was), Some(now)) = (last.power, view.power)
            && was != now
        {
            out.push(if now { Event::PowerOn } else { Event::PowerOff });
        }
        if let (Some(was), Some(now)) = (last.input, view.input)
            && was != now
        {
            out.push(Event::Input);
        }
        if let (Some(was), Some(now)) = (&last.eq, &view.eq)
            && (now
                .raws
                .iter()
                .any(|(id, raw)| was.raws.get(id).is_some_and(|w| w != raw))
                || (now.preset != was.preset && now.preset.is_some()))
        {
            out.push(Event::Eq);
        }
        if view.fault && !last.fault && !view.advice_shown {
            out.push(Event::Fault);
        }
        if view.presses > last.presses && view.presses.is_multiple_of(10) && !view.fault {
            out.push(Event::Advice);
        }
        out
    }

    /// The sticky state `view` holds a called squirrel in, by priority.
    fn standing_of(view: &View) -> Option<State> {
        if view.fault && !view.advice_shown {
            Some(State::Pointing)
        } else if view.power == Some(false) {
            Some(State::Curled)
        } else if view.link == Link::Offline {
            Some(State::Lantern)
        } else if view.link == Link::Reconnecting {
            Some(State::Rowing)
        } else if view.muted == Some(true) {
            Some(State::Muted)
        } else {
            None
        }
    }

    /// What `event` plays, for a squirrel that is `at_home` (called, on the screen it lives on)
    /// or not. A sticky state a squirrel at home holds is not a visit: the view stands behind
    /// it. Away from home only the events a hidden squirrel visits for play.
    fn act(event: Event, at_home: bool) -> Option<What> {
        let held = |state: State| (!at_home).then_some(What::State(state));
        let at_home_only = |trick: Trick| at_home.then_some(What::Trick(trick));
        match event {
            Event::VolumeUp => at_home_only(Trick::Climb),
            Event::VolumeDown => at_home_only(Trick::Slide),
            Event::BigSwing => at_home_only(Trick::BigSwing),
            Event::Unmuted => at_home_only(Trick::Wake),
            Event::Input => Some(What::Trick(Trick::Messenger)),
            Event::Eq => at_home_only(Trick::Palette),
            Event::LinkLost | Event::Offline => (!at_home).then_some(What::Trick(Trick::RowOut)),
            Event::LinkBack => Some(What::Trick(Trick::RowIn)),
            Event::Fault => held(State::Pointing),
            Event::PowerOff => held(State::Curled),
            Event::PowerOn | Event::Hour => Some(What::Trick(Trick::Stretch)),
            Event::Advice => Some(What::Trick(Trick::Hop)),
        }
    }

    /// Play `event`; whether a visit started.
    fn play(&mut self, event: Event, view: &View, now: Instant) -> bool {
        if event == Event::Hour && !self.hour_called {
            return self.hour_calls(now);
        }
        let at_home = self.called && view.screen_is_home;
        match Self::act(event, at_home) {
            Some(what) => self.visit_of(what, now, false, false),
            None => false,
        }
    }

    /// The first hour: the squirrel comes out, stretches and stays, and from then on zaps out
    /// and back in every half hour.
    fn hour_calls(&mut self, now: Instant) -> bool {
        self.hour_called = true;
        let shown = self.pick().is_some();
        let was_called = std::mem::replace(&mut self.called, true);
        let started = self.start(
            Start {
                what: What::Trick(Trick::Stretch),
                over: !was_called,
                visitor: !self.at_home(),
                shown,
                body: None,
            },
            now,
            false,
        );
        self.arm_zaps(now);
        started
    }

    /// Arm the zaps: the first falls due half an hour after what is entering has landed.
    fn arm_zaps(&mut self, now: Instant) {
        let landed = self.visit.map_or(now, |visit| visit.plays_from());
        self.zap_at = Some(landed.max(now) + ZAP_EVERY);
    }

    /// Start a visit of `what`; whether it started. A fault's pointing stands against anything
    /// but `force`.
    fn visit_of(&mut self, what: What, now: Instant, force: bool, over: bool) -> bool {
        let start = Start {
            what,
            over,
            visitor: !self.at_home(),
            shown: self.pick().is_some(),
            body: None,
        };
        self.start(start, now, force)
    }

    /// Start the visit `start` says; whether it started.
    fn start(&mut self, start: Start, now: Instant, force: bool) -> bool {
        let Start {
            mut what,
            over,
            visitor,
            shown,
            mut body,
        } = start;
        // A squirrel that is away in a zap is on show as far as an entrance goes: the zap brings
        // it back.
        let shown = shown || self.zap.is_some();
        let pointing = What::State(State::Pointing);
        let pointed = self
            .visit
            .is_some_and(|v| v.what == pointing && v.ends.is_none());
        if !force && what != pointing && pointed {
            return false;
        }
        self.pointing_owed |= pointed && what != pointing;
        // A squirrel still entering is not on show yet: the new visit carries on its entrance.
        let carried = self
            .visit
            .filter(|visit| visit.entry.is_some() && now < visit.plays_from());
        let entry = match carried {
            Some(visit) => visit.entry,
            None => (self.effects && !shown).then(|| self.roll(now, true)),
        };
        let began = carried.map_or(now, |visit| visit.began);
        let exit = (self.effects && visitor).then(|| match entry {
            Some(entry) => Entry {
                seed: entry.seed.rotate_left(7),
                ..entry
            },
            None => self.roll(now, false),
        });
        // The longship's passes give way to an entrance or an exit that plays in their place.
        match what {
            What::Trick(Trick::RowOut) if exit.is_some() => what = What::State(State::Lantern),
            What::Trick(Trick::Depart) if exit.is_some() => {
                what = What::Trick(Trick::Hop);
                body = Some(Trick::Hop.duration());
            }
            What::Trick(Trick::RowIn | Trick::Arrive) if entry.is_some() => {
                what = What::Trick(Trick::Hop);
            }
            _ => {}
        }
        let plays_from = began + entry.map_or(Duration::ZERO, Entry::len);
        let ends = match what {
            What::State(State::Pointing) => None,
            What::Trick(trick @ (Trick::RowOut | Trick::Depart)) => {
                Some(plays_from + trick.duration())
            }
            _ => Some(plays_from + body.unwrap_or(VISIT) + exit.map_or(Duration::ZERO, Entry::len)),
        };
        self.visit = Some(Visit {
            what,
            over,
            began,
            ends,
            entry,
            exit,
        });
        true
    }

    /// A code was typed at `now`.
    pub(crate) fn code(&mut self, code: Code, now: Instant) {
        self.at = now;
        if self.held {
            self.waiting.push(Waiting::Code(code));
        } else {
            self.code_now(code, now);
        }
    }

    /// Carry out `code` at `now`: no box has the keys.
    fn code_now(&mut self, code: Code, now: Instant) {
        self.at = now;
        match code {
            Code::Konami if self.called => {
                let shown = self.pick().is_some();
                self.called = false;
                self.zap = None;
                self.zap_at = None;
                self.start(
                    Start {
                        what: What::Trick(Trick::Depart),
                        over: false,
                        visitor: true,
                        shown,
                        body: None,
                    },
                    now,
                    true,
                );
            }
            Code::Konami => {
                let shown = self.pick().is_some();
                self.called = true;
                self.start(
                    Start {
                        what: What::Trick(Trick::Arrive),
                        over: true,
                        visitor: false,
                        shown,
                        body: None,
                    },
                    now,
                    true,
                );
                if self.hour_called {
                    self.arm_zaps(now);
                }
            }
            Code::Iddqd => {
                self.god = !self.god;
                self.visit_of(What::Trick(Trick::Hop), now, true, true);
            }
        }
    }

    /// The squirrel was clicked at `now`: it hops if it is showing.
    pub(crate) fn click(&mut self, now: Instant) {
        if self.pick().is_some() {
            self.at = now;
            self.visit_of(What::Trick(Trick::Hop), now, false, true);
        }
    }

    /// Whether a one-shot is playing as of the last observation, which wants frames.
    pub(crate) fn animating(&self) -> bool {
        self.zap.is_some()
            || self.pick().is_some_and(|pick| {
                pick.fx.is_some()
                    || match pick.what {
                        What::Trick(trick) => {
                            self.at.saturating_duration_since(pick.began) < trick.duration()
                        }
                        What::State(_) => false,
                    }
            })
    }

    /// What is shown, as of the last observation: a zap takes the squirrel away for a moment, and
    /// an entrance or an exit shows the animation over what it plays.
    fn pick(&self) -> Option<Pick> {
        if let Some(zap) = self.zap.as_deref().copied() {
            let since = self.at.saturating_duration_since(zap.began);
            let back_from = zap.out_len + ZAP_GAP;
            let fx = |entry: Entry, out: bool, from: Duration, len: Duration| Fx {
                kind: entry.kind,
                seed: entry.seed,
                out,
                progress: (since.saturating_sub(from).as_secs_f64() / len.as_secs_f64()).min(1.0),
            };
            if since < zap.out_len {
                let fx = fx(zap.out, true, Duration::ZERO, zap.out_len);
                return self.shown().map(|pick| Pick {
                    fx: Some(fx),
                    ..pick
                });
            } else if since < back_from {
                return None;
            } else if since < back_from + zap.back_len {
                let fx = fx(zap.back, false, back_from, zap.back_len);
                return self.shown().map(|pick| Pick {
                    fx: Some(fx),
                    ..pick
                });
            }
        }
        let fx = self.visit_fx();
        self.shown().map(|pick| Pick { fx, ..pick })
    }

    /// The entrance or exit the visit in progress is in, if it is in one.
    fn visit_fx(&self) -> Option<Fx> {
        let visit = self.visit?;
        let ends = visit.ends;
        if ends.is_some_and(|ends| self.at >= ends) {
            return None;
        }
        let fx = |entry: Entry, out: bool, since: Duration| Fx {
            kind: entry.kind,
            seed: entry.seed,
            out,
            progress: (since.as_secs_f64() / entry.len().as_secs_f64()).min(1.0),
        };
        if let Some(entry) = visit.entry {
            let since = self.at.saturating_duration_since(visit.began);
            if since < entry.len() {
                return Some(fx(entry, false, since));
            }
        }
        if let (Some(exit), Some(ends), false) = (visit.exit, ends, self.at_home()) {
            let from = ends - exit.len();
            if self.at >= from {
                return Some(fx(exit, true, self.at - from));
            }
        }
        None
    }

    /// What is shown, by spec priority, as of the last observation.
    fn shown(&self) -> Option<Pick> {
        let home = self.last.as_ref().is_none_or(|view| view.screen_is_home);
        let visit = self
            .visit
            .filter(|v| v.ends.is_none_or(|ends| self.at < ends))
            .map(|v| (v.what, v.plays_from(), v.over));
        if self.called && home {
            let pointing = self
                .standing
                .is_some_and(|(state, _)| state == State::Pointing);
            let hopping = visit.filter(|(what, began, over)| {
                let playing =
                    |trick: &Trick| self.at.saturating_duration_since(*began) < trick.duration();
                *over && !pointing && matches!(what, What::Trick(trick) if playing(trick))
            });
            if let Some((state, since)) = self.standing.filter(|_| hopping.is_none()) {
                return Some(Pick {
                    what: What::State(state),
                    began: since,
                    place: Place::Home,
                    fx: None,
                });
            }
            let (what, began) = match visit {
                Some((what @ What::Trick(trick), began, _))
                    if self.at.saturating_duration_since(began) < trick.duration() =>
                {
                    (what, began)
                }
                _ => (self.idle(), self.started),
            };
            return Some(Pick {
                what,
                began,
                place: Place::Home,
                fx: None,
            });
        }
        let (what, began, _) = visit?;
        let what = match what {
            What::Trick(trick)
                if self.at.saturating_duration_since(began) >= trick.duration()
                    && !matches!(trick, Trick::RowOut | Trick::Depart) =>
            {
                What::State(State::Rest)
            }
            what => what,
        };
        Some(Pick {
            what,
            began,
            place: if home { Place::Home } else { Place::Float },
            fx: None,
        })
    }

    /// What a called squirrel at home does with nothing else to play: the music's dance or rest.
    fn idle(&self) -> What {
        match &self.last {
            Some(View {
                playing: Some((Status::Playing, genre)),
                ..
            }) => What::State(State::Music(dance_of(genre))),
            _ => What::State(State::Rest),
        }
    }

    /// Where the music's beat stands: the level's own beat while it gives a level, else a steady
    /// 120 bpm from the session's start. Before the level's first beat, `since` is the time
    /// since the session began, which keeps growing through silence.
    pub(crate) fn beat(&self) -> Beat {
        let elapsed = self.at.saturating_duration_since(self.started);
        let (average, last) = match self.pulse.live(self.at) {
            true => (self.pulse.average, self.pulse.last),
            false => (None, None),
        };
        match (average, last) {
            (Some(_), Some(last)) => Beat {
                since: self.at.saturating_duration_since(last),
                count: self.pulse.count,
            },
            (Some(_), None) => Beat {
                since: elapsed,
                count: 0,
            },
            _ => Beat {
                since: Duration::from_millis((elapsed.as_millis() % BEAT.as_millis()) as u64),
                count: (elapsed.as_millis() / BEAT.as_millis()) as u64,
            },
        }
    }

    /// The beat `state` dances to: the plain bob has none without a level, so it sits still.
    fn beat_of(&self, state: State) -> Beat {
        if state == State::Music(Dance::Bob) && !self.pulse.live(self.at) {
            Beat {
                since: Duration::MAX,
                count: 0,
            }
        } else {
            self.beat()
        }
    }

    /// What the squirrel plays and where, for the tests.
    #[cfg(test)]
    pub(crate) fn what(&self) -> Option<(What, Place)> {
        self.pick().map(|pick| (pick.what, pick.place))
    }

    /// What the visit under way started, while its `VISIT` lasts, for the tests.
    #[cfg(test)]
    pub(crate) fn last_visit(&self) -> Option<What> {
        self.visit.map(|visit| visit.what)
    }

    /// The squirrel as it is drawn as of the last observation, if it is shown.
    pub(crate) fn showing(&self) -> Option<Showing> {
        let pick = self.pick()?;
        let session = self.at.saturating_duration_since(self.started);
        let costume = self.costume();
        let season = self.season();
        let dress = looks::dress(costume, season, session);
        let since = self.at.saturating_duration_since(pick.began);
        let pose = match pick.what {
            What::Trick(trick) => trick.pose(since, &dress),
            What::State(state) => state.pose(since, self.beat_of(state), &dress),
        };
        Some(Showing {
            pose,
            place: pick.place,
            paint: Paint {
                costume,
                accent: None,
                season,
            },
            fx: pick.fx,
            elapsed: session,
        })
    }
}
