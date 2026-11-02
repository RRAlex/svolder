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

/// The visit in progress: what it plays, when it began and when it ends (never, for a fault that
/// stands).
#[derive(Debug, Clone, Copy)]
struct Visit {
    what: What,
    /// Whether it plays over the sticky state a called squirrel at home holds (a click's hop, a
    /// code's arrival or hop).
    over: bool,
    began: Instant,
    ends: Option<Instant>,
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
        self.at = now;
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
        if self
            .visit
            .is_some_and(|v| v.what == What::State(State::Pointing))
            && (!view.fault || view.advice_shown)
        {
            self.visit = None;
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
        } else if self.pointing_owed && self.visit.is_none() {
            self.pointing_owed = false;
            self.visit_of(What::State(State::Pointing), now, true, false);
        }
        let hour_marked = events.contains(&Event::Hour);
        for event in events {
            self.play(event, view, now);
        }
        // The hour speaks only if its stretch is the one shown, begun at this observation.
        hour_marked
            .then(|| self.pick())
            .flatten()
            .filter(|pick| pick.what == What::Trick(Trick::Stretch) && pick.began == now)
            .map(|_| format!("{hour} h in. Ratatoskr stretches."))
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
        let at_home = self.called && view.screen_is_home;
        match Self::act(event, at_home) {
            Some(what) => self.visit_of(what, now, false, false),
            None => false,
        }
    }

    /// Start a visit of `what`; whether it started. A fault's pointing stands against anything
    /// but `force`.
    fn visit_of(&mut self, what: What, now: Instant, force: bool, over: bool) -> bool {
        let pointing = What::State(State::Pointing);
        let pointed = self.visit.is_some_and(|v| v.what == pointing);
        if !force && what != pointing && pointed {
            return false;
        }
        self.pointing_owed |= pointed && what != pointing;
        let ends = match what {
            What::State(State::Pointing) => None,
            What::Trick(trick @ (Trick::RowOut | Trick::Depart)) => Some(now + trick.duration()),
            _ => Some(now + VISIT),
        };
        self.visit = Some(Visit {
            what,
            over,
            began: now,
            ends,
        });
        true
    }

    /// A code was typed at `now`.
    pub(crate) fn code(&mut self, code: Code, now: Instant) {
        self.at = now;
        match code {
            Code::Konami if self.called => {
                self.called = false;
                self.visit_of(What::Trick(Trick::Depart), now, true, false);
            }
            Code::Konami => {
                self.called = true;
                self.visit_of(What::Trick(Trick::Arrive), now, true, true);
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
        self.pick().is_some_and(|pick| match pick.what {
            What::Trick(trick) => self.at.saturating_duration_since(pick.began) < trick.duration(),
            What::State(_) => false,
        })
    }

    /// What is shown, by spec priority, as of the last observation.
    fn pick(&self) -> Option<Pick> {
        let home = self.last.as_ref().is_none_or(|view| view.screen_is_home);
        let visit = self
            .visit
            .filter(|v| v.ends.is_none_or(|ends| self.at < ends))
            .map(|v| (v.what, v.began, v.over));
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
        let costume = if self.god {
            Costume::God
        } else {
            Costume::of_palette(&self.palette)
        };
        let season = self.today.and_then(|(month, day)| Season::on(month, day));
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
            elapsed: session,
        })
    }
}
