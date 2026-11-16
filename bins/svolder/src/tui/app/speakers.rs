// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The computer's space in the TUI: one choice of Off, Crossfeed or Virtual speakers, and the
//! virtual speakers' set.
//!
//! The controller owns both (the daemon's, or this TUI's own when no daemon runs); this side
//! reads the view ([`SpeakersView`], beside the crossfeed's) and sends [`HostEqOp::Speakers`] or
//! [`HostEqOp::Crossfeed`]. The two are exclusive on the controller's side, so choosing one sends
//! only its own op: the other goes off in the same update. The choice is shown only where the
//! view carries speakers (a daemon from before them shows crossfeed alone, as it always did).
//!
//! The rows are the crossfeed's own stops ([`Xf`]): [`Xf::Space`] is the choice, and [`Xf::Set`]
//! the set while the choice is Virtual speakers. The set list's box is
//! [`boxes::speakers`](crate::tui::boxes::speakers).

use svolder_host::eq::SpeakersView;
use svolder_ipc::HostEqOp;

use super::{App, Intent, Xf};

/// What the space is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Space {
    /// Neither plays.
    Off,
    /// The crossfeed plays.
    Crossfeed,
    /// The virtual speakers play.
    Speakers,
}

impl Space {
    /// Every choice, in the order the arrows step through them.
    pub const ALL: [Space; 3] = [Space::Off, Space::Crossfeed, Space::Speakers];

    /// The choice's name.
    pub fn word(self) -> &'static str {
        match self {
            Space::Off => "Off",
            Space::Crossfeed => "Crossfeed",
            Space::Speakers => "Virtual speakers",
        }
    }
}

impl App {
    /// The virtual speakers as the controller last said it: `None` where the controller serves
    /// none.
    pub(crate) fn speakers(&self) -> Option<&SpeakersView> {
        self.computer_eq().and_then(|eq| eq.speakers.as_ref())
    }

    /// Whether the space choice is shown: the computer side has both crossfeed and speakers.
    pub(crate) fn space_shown(&self) -> bool {
        self.crossfeed_shown() && self.speakers().is_some()
    }

    /// What plays now.
    pub(crate) fn space_now(&self) -> Space {
        match (self.speakers(), self.crossfeed()) {
            (Some(speakers), _) if speakers.on => Space::Speakers,
            (_, Some(crossfeed)) if crossfeed.on => Space::Crossfeed,
            _ => Space::Off,
        }
    }

    /// The row the keys land on first in the section: the choice where it is shown.
    pub(crate) fn xf_first(&self) -> Xf {
        match self.space_shown() {
            true => Xf::Space,
            false => Xf::Preset,
        }
    }

    /// After the view changed, a row the section no longer has (the crossfeed's rows while the
    /// speakers play, the set while they do not) gives the keys back to the choice.
    pub(crate) fn fix_space_focus(&mut self) {
        let super::ToneRow::Crossfeed(row) = self.tone.row else {
            return;
        };
        if !self.xf_stops().contains(&row) {
            self.tone.xf_line = None;
            self.tone.row = super::ToneRow::Crossfeed(self.xf_first());
        }
    }

    /// The one change of the space to `to`, with the view's refusal in words when it cannot be
    /// changed.
    pub(crate) fn space_go(&mut self, to: Space) -> Vec<Intent> {
        let now = self.space_now();
        if to == now {
            return Vec::new();
        }
        if let Some(why) = self.speakers().and_then(|view| view.unavailable.clone()) {
            self.warn(why);
            return Vec::new();
        }
        match to {
            Space::Off => match now {
                Space::Speakers => vec![Intent::eq(speakers_op(Some(false), None))],
                _ => self.xf_toggle(Some(false)),
            },
            Space::Crossfeed => self.xf_toggle(Some(true)),
            Space::Speakers => self.speakers_on(),
        }
    }

    /// Turn the virtual speakers on with the chosen set; with none chosen, with the first fetched
    /// one; with none fetched, the set list opens so one can be fetched.
    fn speakers_on(&mut self) -> Vec<Intent> {
        let Some(view) = self.speakers() else {
            return Vec::new();
        };
        if view.set.is_some() {
            return vec![Intent::eq(speakers_op(Some(true), None))];
        }
        if let Some(first) = view.sets.first().map(|set| set.id.clone()) {
            return vec![Intent::eq(speakers_op(Some(true), Some(first)))];
        }
        self.notify("no virtual speaker set is fetched yet: pick one");
        self.open_sets()
    }

    /// The choice `dir` steps from the one that plays: nothing at either end.
    pub(crate) fn space_step(&mut self, dir: i64) -> Vec<Intent> {
        let at = Space::ALL
            .iter()
            .position(|space| *space == self.space_now())
            .unwrap_or(0);
        let next = match dir > 0 {
            true => at + 1,
            false => at.wrapping_sub(1),
        };
        match Space::ALL.get(next) {
            Some(space) => self.space_go(*space),
            None => Vec::new(),
        }
    }

    /// `␣` on the choice or the set: what plays goes off; from off, the virtual speakers where a
    /// set is chosen, else the crossfeed.
    pub(crate) fn space_toggle(&mut self) -> Vec<Intent> {
        let to = match self.space_now() {
            Space::Off if self.speakers().is_some_and(|view| view.set.is_some()) => Space::Speakers,
            Space::Off => Space::Crossfeed,
            _ => Space::Off,
        };
        self.space_go(to)
    }

    /// The fetched set `dir` steps from the chosen one: nothing at either end. The set is
    /// chosen, which rebuilds the chain once; whether it plays is left as it was.
    pub(crate) fn set_step(&mut self, dir: i64) -> Vec<Intent> {
        let Some(view) = self.speakers() else {
            return Vec::new();
        };
        if let Some(why) = view.unavailable.clone() {
            self.warn(why);
            return Vec::new();
        }
        let at = view
            .set
            .as_ref()
            .and_then(|id| view.sets.iter().position(|set| &set.id == id));
        let next = match (at, dir > 0) {
            (Some(at), true) => at + 1,
            (Some(at), false) => at.wrapping_sub(1),
            (None, _) => 0,
        };
        match view.sets.get(next).map(|set| set.id.clone()) {
            Some(id) => vec![Intent::eq(speakers_op(None, Some(id)))],
            None => Vec::new(),
        }
    }

    /// The toast the view's own words earn, once: why the speakers turned themselves off.
    pub(crate) fn note_speakers_view(&mut self) {
        let note = self.speakers().and_then(|view| view.note.clone());
        if note != self.tone.sp_note {
            if let Some(why) = note.clone() {
                self.notify(why);
            }
            self.tone.sp_note = note;
        }
        self.fix_space_focus();
        self.follow_sets();
    }
}

/// The op that changes only these fields of the virtual speakers.
fn speakers_op(on: Option<bool>, set: Option<String>) -> HostEqOp {
    HostEqOp::Speakers {
        on,
        set,
        done: true,
    }
}

/// How much quieter than Off the speakers play: `about 7 dB quieter than Off`.
pub(crate) fn level_text(view: &SpeakersView) -> String {
    match view.loudness_db {
        db if db.abs() < 0.5 => "about as loud as Off".to_string(),
        db if db < 0.0 => format!("about {:.0} dB quieter than Off", -db),
        db => format!("about {db:.0} dB louder than Off"),
    }
}

/// The level in a few words: `7 dB quieter`.
pub(crate) fn level_short(view: &SpeakersView) -> String {
    match view.loudness_db {
        db if db.abs() < 0.5 => "as loud".to_string(),
        db if db < 0.0 => format!("{:.0} dB quieter", -db),
        db => format!("{db:.0} dB louder"),
    }
}

/// The level as a signed number, for a line with little room: `−7 dB`.
pub(crate) fn level_signed(view: &SpeakersView) -> String {
    format!("{:.0} dB", view.loudness_db.round()).replace('-', "\u{2212}")
}

/// How long the responses are: `5.3 ms`, `0.5 s`, `cut to 0.5 s` where they were cut.
pub(crate) fn length_text(view: &SpeakersView) -> String {
    length_words(view.taps_ms, view.cut)
}

/// Responses of `ms` milliseconds in words, `cut` where they were cut to that length: `5.3 ms`,
/// `cut to 0.5 s`.
pub(crate) fn length_words(ms: f64, cut: bool) -> String {
    let length = match ms >= 500.0 {
        true => format!("{:.1} s", ms / 1000.0),
        false => format!("{ms:.1} ms"),
    };
    match cut {
        true => format!("cut to {length}"),
        false => length,
    }
}

/// The choice's name with its technical term and the set: `No room · HRTF · SADIE II, D2`.
pub(crate) fn set_words(view: &SpeakersView) -> String {
    match (&view.room_name, &view.term) {
        (Some(room), Some(term)) => format!("{room} \u{b7} {term} \u{b7} {}", view.label),
        _ => "no set chosen".to_string(),
    }
}

/// The licence line: the SPDX id, then the dataset's own credit words where it asks for them.
pub(crate) fn licence_text(view: &SpeakersView) -> String {
    match view.credit.as_deref().filter(|credit| !credit.is_empty()) {
        Some(credit) => format!("{} \u{b7} {credit}", view.licence),
        None => view.licence.clone(),
    }
}

#[cfg(test)]
mod tests;
