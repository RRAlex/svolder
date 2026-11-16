// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The TUI's state: the readings it has heard, what the user has focused, and the intents
//! keys turn into.
//!
//! Nothing here touches a terminal, a radio or a clock of its own: [`App::apply`] takes the
//! updates a backend reports, [`App::key`] turns a key press into the writes it means, and
//! the read-only views ([`App::faults`], [`App::peak`], [`App::level`], [`App::placed`],
//! [`App::rows`]) are what a renderer draws. Values shown come only from the
//! [`ValueObject`]s a backend hands over; the profile says what a control is, never what its bytes
//! mean.
//!
//! - [`advice`]: the idle advice `a` gives when no fault stands.
//! - [`connect`]: the keys of the Settings box's Connect category.
//! - [`controls`]: the device screen's Controls pane: its rows, and the keys and clicks on them.
//! - [`curve_drag`]: dragging a band's point on the tone-control screen's Response curve.
//! - [`devices`]: the devices screen's state: its rows, the editor over them, a pending removal.
//! - [`dispatch`]: how a key press becomes intents, and the moves from one screen to another.
//! - [`focus`]: where the cursor is, what a click focuses, and the moves `Tab` and the arrows make.
//! - [`frame_flags`]: the one-shot flags the frame loop drains, for the effects they earn.
//! - [`glide`]: level bars that take the mouse, and the volume's glide.
//! - [`intent`]: what a key press or a click asks for.
//! - [`lists`]: the keys of the list screens and of the Settings box.
//! - [`mascot`]: the squirrel's side of the app: what its brain is shown, and whether it is seen.
//! - [`meters`]: the meters' peak markers and the level the animations follow.
//! - [`notices`]: what the user is told: the toast and the advice.
//! - [`overlays`]: what is open over the screen: Settings, the Esc menu, help, the quit question.
//! - [`mod@pointer`]: what the mouse is doing: its drags, its climb, its last press, the tooltip.
//! - [`readings`]: the readings the app has heard, and the faults standing.
//! - [`session`]: who the session is talking to and how it stands.
//! - [`side`]: the device's side or the computer's, and the computer's points.
//! - [`tone`]: the tone control's keys, on the Tone pane and on the tone-control screen.
//! - [`tone_state`]: what the tone control is on its way to: steps asked for and not yet answered.
//!
//! [`ValueObject`]: svolder_ipc::ValueObject

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use ratatui::layout::Rect;
use svolder_ipc::config::RefreshWeeks;
use svolder_ipc::{Config, EffectsConfig, TuiConfig};
use svolder_profiles::Profile;
use svolder_proto::FunctionId;

use crate::backend::DeviceRow;
use crate::recovery::Failure;

use super::boxes::connections::Connections;
use super::boxes::presets::PresetsBox;
use super::boxes::speakers::SpeakersBox;
use super::boxes::toast::{Toast, ToastKind};
use super::decoration::effects::Effects;
use super::decoration::mascot::Mascot;
use super::host::Host;
use super::layout_tier::Tier;
use super::screens::device;
use super::widgets::graph::{Histories, Link};
use super::widgets::spinner;

mod advice;
mod connect;
mod controls;
pub(crate) mod crossfeed;
pub(super) mod curve_drag;
mod devices;
mod dispatch;
pub(super) mod divider;
mod focus;
mod frame_flags;
pub(super) mod glide;
mod intent;
mod lists;
mod mascot;
mod meters;
mod notices;
mod overlays;
mod point_line;
mod pointer;
mod readings;
mod session;
mod side;
pub(crate) mod speakers;
mod tone;
mod tone_state;

pub(crate) use controls::row_group;
pub use crossfeed::{Xf, XfLine};
pub(crate) use devices::DevicesScreen;
pub(crate) use frame_flags::FrameFlags;
pub use intent::{ConnChange, EffectSwitch, Intent, TuiChange};
pub(crate) use meters::{METER_PERIOD, Meters};
pub(crate) use notices::Notices;
pub(crate) use overlays::Overlays;
pub use point_line::PointLine;
pub(crate) use pointer::Pointer;
pub use readings::Reading;
pub(crate) use readings::Readings;
pub(crate) use session::Session;
pub(crate) use side::SWITCH_ROW;
pub use speakers::Space;
/// What the computer's edit line calls its fields.
pub(crate) fn point_line_names() -> [(&'static str, &'static str); 3] {
    point_line::NAMES
}
pub use side::Side;
pub use tone::{LOUDNESS_MAX, ToneRow};
pub use tone_state::Pending;
pub(crate) use tone_state::{TargetCache, TargetView, ToneEdit};

/// How long a write the device has not answered is waited on: long enough for any device to
/// have answered, short enough that a step the device never confirms does not stand for the rest
/// of the session. A band step asked for ([`Readings::pending`]) and a recall's `loading` both give
/// up after it, in the [`App::tick`] that finds it past.
pub(crate) const ANSWER_LIFETIME: Duration = Duration::from_secs(5);
/// The terminal an app assumes until the loop measures the real one, which is the smallest
/// any screen is drawn in.
const DEFAULT_SIZE: (u16, u16) = (80, 24);
/// How long the idle advice toast stays.
const ADVICE_LIFETIME: Duration = Duration::from_secs(6);
/// How often the app asks to be ticked.
const TICK_PERIOD: Duration = Duration::from_millis(250);

/// Which screen the TUI is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    /// The controls-first device screen.
    Device,
    /// The tone-control screen.
    Tone,
    /// Every reading the device has sent, with its age and rate.
    Telemetry,
    /// The configured devices, to switch between.
    Devices,
    /// The name of the Settings box's keys, for the key table and the help overlay. The box is
    /// a modal over a screen ([`Overlays::settings`]); no app ever shows this as its screen.
    Settings,
    /// One step of the setup wizard, which runs before there is a device to show.
    ///
    /// The wizard keeps its own state and reads its own keys; this is here so that the key
    /// table, the wizard's key row and the help overlay name its screen the way they name every other
    /// one. No app ever shows it.
    Setup,
}

/// What an app holds when no device is open on it: the profile it draws through is a placeholder,
/// so the device screen shows a panel in place of controls it cannot have.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Holding {
    /// The daemon holds the device [`Session::device_name`] and has not found it yet; `since` is
    /// when this session began waiting for it.
    Waiting {
        /// When the wait began.
        since: std::time::SystemTime,
    },
    /// No device is open: the Devices screen shows `why` (the failure's words and its next step)
    /// above the list, when there is a failure to show.
    Nothing {
        /// What happened, and the step that follows it.
        why: Option<(String, String)>,
    },
}

/// Which pane has the focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    /// The control rows.
    Controls,
    /// The tone-control summary and its bands.
    Tone,
    /// The status lines.
    Status,
}

/// The row each screen was left on, to come back to when it is shown again.
#[derive(Debug, Clone, Default)]
pub(crate) struct FocusMemory {
    /// The device screen's focused control row.
    pub(crate) device: usize,
    /// The devices screen's focused row.
    pub(crate) devices: usize,
    /// The reading the telemetry screen had focused.
    pub(crate) reading: Option<FunctionId>,
}

/// Where the Settings box is: a category, an index into
/// [`settings::CATEGORIES`](crate::tui::screens::settings::CATEGORIES), and a row of it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SettingsBox {
    /// The category the box shows.
    pub category: usize,
    /// The selected row of that category.
    pub row: usize,
}

/// What drawing a frame leaves for the app to read: whether the last frame drew the switch, per
/// screen (a frame too narrow for it has no switch row for a key to be on; true until a frame says
/// otherwise), where it drew the bar between the panes, and, while a frame is being drawn, the
/// device screen's layout it worked out.
#[derive(Debug, Clone)]
pub struct Drawn {
    /// The Controls pane's switch line.
    pub controls_switch: std::cell::Cell<bool>,
    /// Whether the frame being drawn has drawn the Controls pane's switch so far; it becomes
    /// [`Drawn::controls_switch`] when the pane is done, so the rows read the last frame's answer.
    pub controls_pending: std::cell::Cell<bool>,
    /// The tone screen's switch row.
    pub tone_switch: std::cell::Cell<bool>,
    /// Where the last device frame drew the bar between its two panes, `None` where it drew no
    /// bar to move: another screen, one pane, or a terminal too narrow for two of 40 columns.
    pub split: std::cell::Cell<Option<divider::Split>>,
    /// Whether a frame is being drawn ([`Drawn::hold`], [`Drawn::begin_frame`]), so the layout
    /// below may be kept for it.
    held: std::cell::Cell<bool>,
    /// The device screen's layout as the frame being drawn worked it out: the frame's area, its
    /// tiers and its three bands. Kept while [`Drawn::hold`] holds, so every part of one frame
    /// that asks where the bands are (the panes, their frames, the squirrel's home, the cover)
    /// reads the one answer rather than building both panes again; `None` outside a frame.
    device_rows: std::cell::Cell<Option<(Rect, Tier, [Rect; 3])>>,
    /// The squirrel's home as the frame being drawn found it, over an area in a monochrome theme
    /// or not (what the panes' words say can differ by it): the squirrel's look at its home and
    /// its draw read the one answer. Kept with the layout, `None` outside a frame.
    home: std::cell::RefCell<Option<(Rect, bool, Option<device::HomePane>)>>,
    /// The box the cover had in the frame last drawn: the frame's area, the cover and the cells
    /// it was sized for, and the box (`None`: no room). The picture is sized by it after the
    /// frame rather than by laying the screen out again; kept until the next frame begins.
    art_box: std::cell::RefCell<Option<ArtBox>>,
}

/// The box the cover had in a drawn frame ([`Drawn::art_box`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ArtBox {
    /// The frame's area.
    pub area: Rect,
    /// The cover the box was for.
    pub hash: String,
    /// The terminal's cell, in pixels, the box was sized by.
    pub font: (u16, u16),
    /// The box, or `None` where the frame had no room for it.
    pub size: Option<ratatui::layout::Size>,
}

impl Drawn {
    /// Keep the layout a frame works out until the answer is dropped, for a stretch in which
    /// the app does not change: drawing one frame. A hold inside another leaves it to the outer
    /// one to let go.
    #[must_use = "the layout is kept only while the hold is held"]
    pub fn hold(&self) -> Held<'_> {
        Held {
            drawn: self,
            opened: !self.held.replace(true),
        }
    }

    /// Keep the layout from here until [`Drawn::end_frame`], for the loop's steps before a frame
    /// and the frame itself: those steps change nothing the layout reads (the squirrel's brain,
    /// the terminal's size, the pointer), so the squirrel's look at its home, the picture's
    /// check and the frame all read one layout.
    pub fn begin_frame(&self) {
        self.held.set(true);
        self.art_box.replace(None);
    }

    /// Forget what [`Drawn::begin_frame`] kept: the frame is drawn.
    pub fn end_frame(&self) {
        self.held.set(false);
        self.forget();
    }

    /// Forget the layout and the home a frame kept.
    fn forget(&self) {
        self.device_rows.set(None);
        self.home.replace(None);
    }

    /// The squirrel's home over `area`, `monochrome` or not, as this frame found it, if it has.
    pub(crate) fn home(&self, area: Rect, monochrome: bool) -> Option<Option<device::HomePane>> {
        self.home
            .borrow()
            .as_ref()
            .filter(|(at, mono, _)| *at == area && *mono == monochrome)
            .map(|(_, _, home)| home.clone())
    }

    /// Keep the squirrel's home over `area` for the rest of the frame, if one is held.
    pub(crate) fn keep_home(&self, area: Rect, monochrome: bool, home: Option<device::HomePane>) {
        if self.held.get() {
            self.home.replace(Some((area, monochrome, home)));
        }
    }

    /// Note the box the cover had in the frame being drawn.
    pub(crate) fn keep_art_box(&self, art_box: ArtBox) {
        self.art_box.replace(Some(art_box));
    }

    /// The box the frame last drawn gave the cover `hash` over `area` with cells `font` large, if
    /// it noted one for exactly those.
    pub(crate) fn drawn_art_box(
        &self,
        area: Rect,
        hash: &str,
        font: (u16, u16),
    ) -> Option<Option<ratatui::layout::Size>> {
        self.art_box
            .borrow()
            .as_ref()
            .filter(|kept| kept.area == area && kept.hash == hash && kept.font == font)
            .map(|kept| kept.size)
    }

    /// The device screen's layout over `area` as this frame worked it out, if it has.
    pub(crate) fn device_rows(&self, area: Rect) -> Option<(Tier, [Rect; 3])> {
        self.device_rows
            .get()
            .filter(|(at, _, _)| *at == area)
            .map(|(_, tier, rows)| (tier, rows))
    }

    /// Keep the device screen's layout over `area` for the rest of the frame, if one is held.
    pub(crate) fn keep_device_rows(&self, area: Rect, (tier, rows): (Tier, [Rect; 3])) {
        if self.held.get() {
            self.device_rows.set(Some((area, tier, rows)));
        }
    }
}

/// A frame's hold on what it worked out ([`Drawn::hold`]): dropping it forgets all of it.
pub struct Held<'a> {
    /// What is held.
    drawn: &'a Drawn,
    /// Whether this hold began it, and so ends it.
    opened: bool,
}

impl Drop for Held<'_> {
    fn drop(&mut self) {
        if self.opened {
            self.drawn.held.set(false);
            self.drawn.forget();
        }
    }
}

impl Default for Drawn {
    fn default() -> Self {
        Drawn {
            controls_switch: std::cell::Cell::new(true),
            controls_pending: std::cell::Cell::new(false),
            tone_switch: std::cell::Cell::new(true),
            split: std::cell::Cell::new(None),
            held: std::cell::Cell::new(false),
            device_rows: std::cell::Cell::new(None),
            home: std::cell::RefCell::new(None),
            art_box: std::cell::RefCell::new(None),
        }
    }
}

/// Where the keys are in the focused list: on a row, or on the Controls pane's switch line, which
/// sits over the row it remembers (the one carrying the volume). One value, so a row index that
/// is stale while the switch has the keys cannot be written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlsCursor {
    /// On row `n`.
    Row(usize),
    /// On the switch line over row `over`, the row the cursor returns to when it leaves.
    Target { over: usize },
}

impl ControlsCursor {
    /// The row the cursor is on, or the one the switch line sits over.
    pub fn row(self) -> usize {
        match self {
            Self::Row(row) | Self::Target { over: row } => row,
        }
    }

    /// Whether the cursor is on the switch line.
    pub fn on_target(self) -> bool {
        matches!(self, Self::Target { .. })
    }
}

/// Everything one key press can move, for the cases that press a key and ask what changed.
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Moved {
    /// The screen being shown.
    pub(crate) screen: Screen,
    /// The focused row of the focused list.
    pub(crate) selected: usize,
    /// The focused band.
    pub(crate) band: usize,
    /// The focused row of the Tone pane.
    pub(crate) tone_row: ToneRow,
    /// The pane with the focus.
    pub(crate) pane: Pane,
    /// The side shown.
    pub(crate) side: Side,
    /// Whether a fault's advice is being shown.
    pub(crate) advice: bool,
    /// How many idle advice toasts `a` has posted.
    pub(crate) advised: usize,
    /// Whether mouse capture is on.
    pub(crate) mouse: bool,
    /// Where the Settings box is, while it is open.
    pub(crate) settings: Option<SettingsBox>,
    /// Where the menu's cursor is, while it is open.
    pub(crate) menu: Option<usize>,
    /// Whether the help overlay is open.
    pub(crate) help: bool,
    /// Whether a device is being edited.
    pub(crate) editing: bool,
    /// The device screen's split, as its bits.
    pub(crate) split: u64,
    /// Whether a removal is waiting to be confirmed.
    pub(crate) confirming: bool,
    /// Whether the preset list is open.
    pub(crate) presets: bool,
    /// Whether the virtual speakers' set list is open.
    pub(crate) sets: bool,
    /// Whether a band's edit line is open.
    pub(crate) edit_line: bool,
    /// Whether the quit question is open.
    pub(crate) quitting: bool,
}

/// Everything the TUI knows.
pub struct App {
    /// The profile the device is read through: kinds, ranges, labels and the tone control.
    pub profile: Profile,
    /// Who this session is talking to and how it stands: the device's name, its link and the
    /// daemon behind it.
    pub(crate) session: Session,
    /// What the device has said and what is still on its way to it: the readings, their histories
    /// and the writes the device has not answered.
    pub(crate) readings: Readings,
    /// The meters' peak markers, smoothed level and last feed.
    pub(crate) meters: Meters,
    /// The one-shot flags the frame loop drains: what changed since the last frame, for the effects
    /// it earns.
    pub(crate) frame_flags: FrameFlags,
    /// The screen being shown.
    pub screen: Screen,
    /// The focused pane of the device screen.
    pub pane: Pane,
    /// Whether the volume and the tone control shown are the device's or the computer's.
    pub side: Side,
    /// What the last frame drew, for keys to know which rows exist.
    pub drawn: Drawn,
    /// The focused row of the focused list; on the Controls pane it may instead be the switch
    /// line over a row ([`ControlsCursor`]). On the tone screen the switch is [`ToneRow::Target`].
    pub cursor: ControlsCursor,
    /// The rows the screens not being shown were left on.
    pub(crate) focus_memory: FocusMemory,
    /// The tone control's edit: the focused band and row, the edit line, the mode `f` and the
    /// Loudness half come back to, and the curve's clip and target.
    pub(crate) tone: ToneEdit,
    /// How big the terminal is, as the loop last measured it: what the help overlay and the
    /// `Now playing` row's marquee are worked out against.
    pub size: (u16, u16),
    /// What the user is told: the toast, the advice and what its idle and fault lines count from.
    pub(crate) notices: Notices,
    /// What is open over the screen: the Settings box, the Esc menu, the help overlay and the quit
    /// question, and what the Connect category knows.
    pub(crate) overlays: Overlays,
    /// The `[tui]` table as this session holds it: what the Settings box shows and steps
    /// through. The flags below say what is actually running, which is this table and what the
    /// terminal can show.
    pub tui: TuiConfig,
    /// The configuration file the Settings box names; set by whoever builds the app (the
    /// opener, from the path it also reads and writes).
    pub config_path: PathBuf,
    /// Whether mouse capture is on.
    pub mouse: bool,
    /// Whether the animation layer runs.
    pub effects: bool,
    /// Whether the rows of a list dim with their distance from the selected row.
    pub list_fade: bool,
    /// Whether a changed value glides to its new length, which is the one animation this app
    /// keeps the state of rather than the animation layer.
    pub eased: bool,
    /// Whether the pane borders breathe with the meters' smoothed level.
    pub breathing: bool,
    /// Whether a meter's bar keeps a marker at its recent peak.
    pub peak_hold: bool,
    /// The devices screen's rows, what the configuration file names and the editor over them.
    pub devices: DevicesScreen,
    /// The saved presets, the preset list over the screens, the AutoEq index and the computer EQ.
    pub presets: PresetsBox,
    /// The virtual speakers' set list, the catalogue the daemon last gave and the fetch in
    /// flight.
    pub speakers_box: Box<SpeakersBox>,
    /// What the mouse is doing: the drags it holds, the climb it started, its last press and the
    /// tooltip it asks for.
    pub(crate) pointer: Pointer,
    /// What the TUI knows of the host's own audio, which the loop folds in as the host says it
    /// and a device switch carries over to the app that replaces this one.
    pub host: Host,
    /// The squirrel: whether it is called, what it is doing and what it has seen of the app.
    pub(crate) mascot: Mascot,
    /// The local time the header shows, as `HH:MM`; whoever draws sets it.
    pub clock: String,
    /// The moment of the last tick or update: what ages and rates are measured against, and
    /// how far along a bar's easing the screens draw it.
    pub now: Instant,
    /// When this session began, which the header's and the Devices screen's spinner turn from:
    /// whichever of them draws, it draws the same glyph at the same moment, from the one clock.
    spinner_epoch: Instant,
}

impl App {
    /// The focused row of the focused list; the row a switch line sits over counts.
    pub fn selected(&self) -> usize {
        self.cursor.row()
    }

    /// Put the cursor on row `row`, off the switch line.
    pub fn set_selected(&mut self, row: usize) {
        self.cursor = ControlsCursor::Row(row);
    }

    /// An app on `profile`, watching `device_name`, drawn as `tui` asks.
    ///
    /// Nothing animates on an app until [`App::set_look`] has said what the terminal can show:
    /// a configuration alone cannot answer for that, so the animation flags start off and the
    /// layer turns on whichever of them it is running.
    pub fn new(
        profile: Profile,
        device_name: String,
        tui: &TuiConfig,
        devices: Vec<DeviceRow>,
    ) -> App {
        let now = Instant::now();
        let product = profile.display.clone();
        let mut app = App {
            frame_flags: FrameFlags::default(),
            pointer: Pointer::default(),
            notices: Notices::new(now),
            meters: Meters::default(),
            profile,
            readings: Readings {
                values: BTreeMap::new(),
                histories: Histories::default(),
                link: Link::default(),
                easings: BTreeMap::new(),
                pending: BTreeMap::new(),
                unreported: BTreeSet::new(),
                sent: BTreeMap::new(),
                loading: None,
            },
            screen: Screen::Device,
            pane: Pane::Controls,
            side: Side::Dac,
            drawn: Drawn::default(),
            cursor: ControlsCursor::Row(0),
            focus_memory: FocusMemory::default(),
            size: DEFAULT_SIZE,
            overlays: Overlays {
                settings: None,
                settings_memory: SettingsBox::default(),
                held_accents: svolder_ipc::FamilyAccents::default(),
                conn: Connections::default(),
                menu: None,
                help_open: false,
                help_scroll: 0,
                quitting: false,
            },
            tui: tui.clone(),
            config_path: PathBuf::new(),
            mouse: tui.mouse,
            effects: false,
            list_fade: false,
            eased: false,
            breathing: false,
            peak_hold: false,
            devices: DevicesScreen {
                opened: None,
                rows: devices,
                configured: Vec::new(),
                default_device: None,
                editor: None,
                confirm: None,
            },
            speakers_box: Box::default(),
            presets: PresetsBox {
                saved: Vec::new(),
                preset_list: None,
                device_list: None,
                ask_seq: 0,
                landed: None,
                autoeq_weeks: RefreshWeeks::default(),
                loaded_preset: None,
                computer_flattened: None,
            },
            tone: ToneEdit {
                band: 0,
                row: ToneRow::Band,
                point: 0,
                point_line: None,
                xf_line: None,
                xf_note: None,
                sp_note: None,
                header_loudness: false,
                mode_before_flat: None,
                mode_before_loudness: None,
                edit_line: None,
                target_cache: TargetCache::default(),
            },
            session: Session {
                product,
                device_name,
                unit: None,
                connected: true,
                feed_open: true,
                holding: None,
                daemon: false,
                daemon_version: None,
                daemon_version_warned: None,
                config_changed_told: None,
                join_hint: None,
                gone_told: None,
                daemon_gone: false,
                passphrase_note: None,
                facts: None,
            },
            host: Host::new(now),
            mascot: Mascot::new(now),
            clock: String::new(),
            now,
            spinner_epoch: now,
        };
        app.set_animations(&tui.effects, false);
        app
    }

    /// Advance the clock every age and rate is measured against, drop a toast that has had
    /// its time, and a band step and a load the device has not confirmed in theirs
    /// ([`ANSWER_LIFETIME`]), forget the easings that have landed, and carry the `Now playing`
    /// row's words too wide for the row on by a cell, and the Volume's climb on by the time that
    /// has passed ([`glide`]).
    pub fn tick(&mut self, now: Instant) {
        self.now = now;
        self.overlays.conn.expire(now);
        if let Some(tooltip) = self.pointer.tooltip.as_mut() {
            tooltip.age(now);
        }
        if let Some(toast) = &self.notices.toast
            && now.saturating_duration_since(self.notices.toast_at) >= toast.lifetime()
        {
            self.notices.toast = None;
        }
        // A step the device has not answered in its time is not stepped from any more, and a
        // recall's own steps give up with its `loading`, which is timed from the same moment.
        self.readings
            .pending
            .retain(|_, pending| !pending.expired(now));
        if self
            .readings
            .loading
            .as_ref()
            .is_some_and(|loading| loading.expired(now))
        {
            self.readings.loading = None;
        }
        self.readings
            .easings
            .retain(|_, easing| !easing.landed(now));
        self.advance_climb(now);
        let words = self
            .host
            .now_playing()
            .map_or(0, |words| device::cells(&words));
        // Words that fit beside any picture stand still whatever the picture takes, so the
        // screen is not laid out to measure it.
        let reserved = match words <= device::words_beside_art() {
            true => 0,
            false => device::art_reserved(self),
        };
        self.host.marquee.advance(
            now,
            words,
            device::now_playing_room(self.size).saturating_sub(reserved),
        );
    }

    /// Note how big the terminal is: what the help overlay and the `Now playing` row's marquee
    /// are worked out against.
    pub fn resize(&mut self, width: u16, height: u16) {
        // A tooltip is placed against the frame it was drawn in.
        if self.size != (width, height) {
            self.pointer.tooltip = None;
        }
        self.size = (width, height);
        // The open help overlay's offset holds inside the view the new size gives it; a closed
        // one starts again from its first key when it opens.
        if self.overlays.help_open {
            let (_, most) = super::screens::help::scroll_of(self);
            self.overlays.help_scroll = self.overlays.help_scroll.min(most);
        }
    }

    /// The glyph the header's or the Devices screen's spinner is showing now: whichever of them
    /// draws, it draws the same one, from the same clock.
    pub fn spinner(&self) -> char {
        spinner::glyph(self.now.saturating_duration_since(self.spinner_epoch))
    }

    /// Whether a spinner is turning on the screen right now; keeps the loop's tick at a hundred
    /// milliseconds or faster. The header's own spins while the link is down and this session's
    /// feed could still bring it back; the Devices screen's spins while showing and a listed
    /// device carries a retry wait.
    ///
    /// A link that is down is not always a wait: a direct session past its connect deadline, or
    /// a switch the loop has given up on, has nothing left trying to reconnect, which
    /// [`Session::feed_open`] says by then being false — the header shows that hollow rather than
    /// spinning for a reconnect that is never coming.
    pub fn spinning(&self) -> bool {
        (!self.session.connected && self.session.feed_open)
            || (self.screen == Screen::Devices
                && self
                    .devices
                    .rows
                    .iter()
                    .any(|row| row.retry_in_secs.is_some()))
    }

    /// Whether the host's own audio is drawn and reached: the loop reaches the host for this
    /// session and `[tui] host_audio` is on. Nothing of the host is drawn, read or pressed
    /// while this is false.
    pub fn host_shown(&self) -> bool {
        self.host.linked && self.tui.host_audio
    }

    /// Whether this session wants the daemon's view of the host, which is when `[tui] host_audio`
    /// is on and `SVOLDER_HOST=off` does not hold it off: what a daemon's feed is asked for the
    /// host by, whether or not the host is linked yet.
    pub fn asks_host(&self) -> bool {
        self.tui.host_audio && !self.host.held_off
    }

    /// Whether a tooltip may show now: the `tooltips` setting is on, mouse capture is on and no
    /// box is open.
    pub(crate) fn tips_allowed(&self) -> bool {
        self.tui.tooltips && self.mouse && !self.modal_open()
    }

    /// Whether a tooltip waits for its delay to pass, which keeps the loop's tick short enough
    /// to show it on time.
    pub fn tooltip_pending(&self) -> bool {
        self.tips_allowed() && self.pointer.tooltip.is_some_and(|tooltip| !tooltip.shown)
    }

    /// How long the event loop waits between ticks.
    pub fn tick_period(&self) -> Duration {
        TICK_PERIOD
    }

    /// Draw as `tui` asks, with `effects` the animation layer that configuration resolved to on
    /// this terminal.
    ///
    /// The four animation flags are the layer's own answers, not the table's: a terminal that
    /// cannot show the animations leaves them off whatever the file says. Mouse capture is not
    /// among them (`M` turns it, as the Settings box does too); the seam that turns it lives
    /// in the event loop, which sets [`App::mouse`] there.
    pub(crate) fn set_look(&mut self, tui: &TuiConfig, effects: &Effects) {
        self.tui = tui.clone();
        self.set_animations(&tui.effects, effects.capable());
        // Host audio switched off takes the Host group's rows away, so a Controls cursor on one
        // of them comes back to the last row there still is.
        if self.screen == Screen::Device {
            self.set_selected(self.selected().min(self.controls_len().saturating_sub(1)));
        }
    }

    /// Take in the parts of `config` the TUI reads while it runs: the Connect category's two
    /// sections and the AutoEq refresh.
    pub(crate) fn take_config(&mut self, config: &Config) {
        self.overlays.conn.load(config);
        self.presets.autoeq_weeks = config.autoeq.refresh_weeks;
    }

    /// The four animations this app draws itself: each of them on where `effects` switches it
    /// on, the layer as a whole is on, and the terminal can show any of it (`capable`).
    ///
    /// Every path that sets them comes through here, so an app can never be left drawing a
    /// glide the animation layer is not running.
    fn set_animations(&mut self, effects: &EffectsConfig, capable: bool) {
        let running = capable && effects.enabled;
        self.effects = running;
        self.eased = running && effects.eased;
        self.breathing = running && effects.breathing;
        self.peak_hold = running && effects.peak_hold;
        self.list_fade = running && effects.list_fade;
    }

    /// Show `message` in an Info toast until [`App::tick`] has aged it out.
    ///
    /// The moment it is stamped with is this app's own clock rather than one read here, so a
    /// toast lives from the tick before it to its kind's [`ToastKind::lifetime`] after that.
    /// A newer toast of any kind replaces the one showing.
    pub fn notify(&mut self, message: impl Into<String>) {
        self.put_toast(message.into(), ToastKind::Info);
    }

    /// Show `message` in a Failure toast: a refusal, an error or a warning.
    pub fn warn(&mut self, message: impl Into<String>) {
        self.put_toast(message.into(), ToastKind::Failure);
    }

    /// Show `message` in a Wait toast, which stays while it is said again at least every
    /// [`ToastKind::lifetime`] and goes that long after the last time, or at [`App::end_wait`].
    pub fn waiting(&mut self, message: impl Into<String>) {
        self.put_toast(message.into(), ToastKind::Wait);
    }

    /// Clear a Wait toast, leaving a toast of any other kind showing.
    pub fn end_wait(&mut self) {
        if self
            .notices
            .toast
            .as_ref()
            .is_some_and(|toast| toast.kind == ToastKind::Wait)
        {
            self.notices.toast = None;
        }
    }

    /// The toast showing now, if there is one.
    pub fn toast(&self) -> Option<&Toast> {
        self.notices.toast.as_ref()
    }

    /// Take the toast away, as a click on it does. That press reached nothing under the toast,
    /// so it makes no double-click with the next press on the same cell.
    pub fn dismiss_toast(&mut self) {
        self.notices.toast = None;
        self.pointer.last_press = None;
        self.pointer.double_click = false;
    }

    /// Show `text` in an Info toast that stays [`ADVICE_LIFETIME`], longer than a notice, so
    /// a tip can be read.
    pub fn advise(&mut self, text: impl Into<String>) {
        self.put_toast(text.into(), ToastKind::Info);
        if let Some(toast) = &mut self.notices.toast {
            toast.lasts = Some(ADVICE_LIFETIME);
        }
    }

    fn put_toast(&mut self, text: String, kind: ToastKind) {
        self.notices.toast = Some(Toast {
            text,
            kind,
            lasts: None,
        });
        self.notices.toast_at = self.now;
    }

    /// Say what a failure was, and keep the step it names for the advice line.
    ///
    /// The two halves land where the screen has room for them: the toast carries what
    /// happened, and `a` shows what to do about it. A failure that is not the device's — the
    /// configuration file refusing a write, say — leaves every write on its way to the device
    /// as it was; [`App::write_failed`] is the one for a write to the device.
    pub fn notify_failure(&mut self, failure: &Failure) {
        self.warn(failure.what.clone());
        self.notices.advice = Some(failure.next.clone());
        self.notices.step_shown = false;
    }

    /// Say what a failed write to the device, or a device feed that ended, was, as
    /// [`App::notify_failure`] does, and stop waiting on the device: a recall still loading is
    /// over and no band step is pending any more, since what failed leaves the writes before it
    /// nothing certain to be answered with. A Volume climbing is over too.
    pub fn write_failed(&mut self, failure: &Failure) {
        self.notify_failure(failure);
        self.readings.loading = None;
        self.readings.pending.clear();
        self.pointer.climb = None;
    }

    /// What one key press can move: the screen, the row, band or tone row a list has focused,
    /// which pane has it, and the flags and overlays a key alone opens. keys.rs's tests compare
    /// two of these taken around one press to confirm every key of the table really does something.
    #[cfg(test)]
    pub(crate) fn snapshot_for_test(&self) -> Moved {
        Moved {
            screen: self.screen,
            selected: self.selected(),
            band: self.tone.band,
            tone_row: self.tone.row,
            pane: self.pane,
            side: self.side,
            advice: self.notices.show_advice,
            advised: self.notices.advice_presses,
            mouse: self.mouse,
            settings: self.overlays.settings,
            menu: self.overlays.menu,
            help: self.overlays.help_open,
            editing: self.devices.editor.is_some(),
            split: self.tui.device_split.to_bits(),
            confirming: self.devices.confirm.is_some(),
            presets: self.presets.preset_list.is_some() || self.presets.device_list.is_some(),
            sets: self.speakers_box.list.is_some(),
            edit_line: self.tone.edit_line.is_some(),
            quitting: self.overlays.quitting,
        }
    }
}

/// `index` moved by `dir` around a cycle `len` long: what a choice row steps through. Past the
/// last choice is the first again, and before the first is the last.
pub(super) fn cycled(index: usize, dir: isize, len: usize) -> usize {
    let Ok(len) = isize::try_from(len) else {
        return index;
    };
    if len == 0 {
        return 0;
    }
    let index = isize::try_from(index).unwrap_or(0);
    usize::try_from((index + dir).rem_euclid(len)).unwrap_or(0)
}

/// `index` moved by `delta` and held inside `0..len`.
pub(super) fn stepped(index: usize, delta: isize, len: usize) -> usize {
    let Some(last) = len.checked_sub(1) else {
        return 0;
    };
    let index = index.min(last);
    if delta < 0 {
        index.saturating_sub(delta.unsigned_abs())
    } else {
        index.saturating_add(delta.unsigned_abs()).min(last)
    }
}

#[cfg(test)]
mod tests;
