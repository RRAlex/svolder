// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The squirrel's side of the app: the view the brain is shown each drawn turn and the
//! switches that say whether it is seen at all.

use std::time::Instant;

use ratatui::layout::Rect;
use svolder_ipc::ColorDepth;
use svolder_profiles::roles;

use super::{App, Screen};
use crate::tui::boxes::toast::ToastKind;
use crate::tui::decoration::mascot::{EqSig, Link, View, draw};
use crate::tui::theme::Theme;

impl App {
    /// What the brain looks at: the device's volume, mute, power, input and tone control, the
    /// link, the faults and the player, as the app holds them now.
    pub(crate) fn mascot_view(&self) -> View {
        let toggle = |key: &str| {
            let control = self.profile.control_by_key(key)?;
            Some(self.raw_of(control.id)? != 0)
        };
        let volume = roles::volume(&self.profile).and_then(|control| {
            let (min, max) = control.range?;
            Some((self.raw_of(control.id)?, min, max))
        });
        let muted = match self.profile.control_by_key("mute") {
            Some(control) => self.raw_of(control.id).map(|raw| raw != 0),
            None if self.host_shown() => self.host.sink.as_ref().map(|sink| sink.muted),
            None => None,
        };
        let input = self
            .profile
            .control_by_key("input")
            .and_then(|control| self.raw_of(control.id));
        // The tone control is unknown until one of its readings has arrived.
        let eq = self
            .profile
            .eq
            .as_ref()
            .map(|eq| EqSig {
                preset: self.presets.loaded_preset.clone(),
                raws: self
                    .readings
                    .values
                    .iter()
                    .filter(|(id, _)| id.first() == eq.id)
                    .filter_map(|(id, reading)| Some((*id, reading.raw_i64()?)))
                    .collect(),
            })
            .filter(|sig| !sig.raws.is_empty());
        let link = if self.session.holding.is_some() {
            Link::Offline
        } else if self.session.connected {
            Link::Connected
        } else if self.session.feed_open {
            Link::Reconnecting
        } else {
            Link::Offline
        };
        View {
            volume,
            muted,
            power: toggle("power"),
            input,
            eq,
            link,
            fault: !self.faults().is_empty(),
            advice_shown: self.notices.show_advice,
            level: self.level(),
            playing: self
                .host
                .playing
                .as_ref()
                .map(|playing| (playing.status, playing.genre.clone())),
            presses: u32::try_from(self.notices.advice_presses).unwrap_or(u32::MAX),
            screen_is_home: self.screen == Screen::Device && self.home_has_room(),
        }
    }

    /// Whether the squirrel's home box has room on the device screen at the terminal's size.
    fn home_has_room(&self) -> bool {
        let area = Rect::new(0, 0, self.size.0, self.size.1);
        draw::home_has_room(
            area,
            self,
            &Theme::from_config(&self.tui, false, ColorDepth::Ansi16),
        )
    }

    /// Show the brain the app as of `now`, on the day `month_day`, and say the hour's words in
    /// an Info toast when the squirrel may be seen and no Failure toast is showing.
    pub(crate) fn observe_mascot(&mut self, now: Instant, month_day: (u8, u8)) {
        self.mascot.today = Some(month_day);
        self.mascot.palette.clone_from(&self.tui.palette);
        let view = self.mascot_view();
        if let Some(text) = self.mascot.observe(&view, now)
            && self.mascot_on()
            && self
                .notices
                .toast
                .as_ref()
                .is_none_or(|toast| toast.kind != ToastKind::Failure)
        {
            self.notify(text);
        }
    }

    /// Whether the squirrel may be seen: the terminal shows its colours and `[tui.effects]`
    /// allows it. When this is false nothing is drawn and the codes are inert.
    pub(crate) fn mascot_on(&self) -> bool {
        self.mascot.capable && self.tui.effects.enabled && self.tui.effects.mascot
    }

    /// Whether a one-shot of the squirrel is playing, which wants frames.
    pub(crate) fn mascot_animating(&self) -> bool {
        self.mascot_on() && self.mascot.animating()
    }
}
