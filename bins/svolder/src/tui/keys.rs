// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The keys every screen listens for.
//!
//! One table names every key `App::key` reads, what it does, which screens list it and when
//! it is worth pressing there, so the help overlay and the setup wizard's key row both draw
//! from the very facts a key press acts on rather than from a hand-written string of their own.

use crossterm::event::KeyCode;
use svolder_ipc::config::TITLE_METER;
use svolder_profiles::device_presets;

use super::app::{App, EffectSwitch, Screen};
use super::screens::settings::Setting;

/// Where [`hints_for`] puts a key: the arrow keys first, then what a screen's own keys do,
/// then the keys that switch screens, then the keys that mean the same thing everywhere.
const NAV: u8 = 0;
const ACTIONS: u8 = 1;
const SCREENS: u8 = 2;
const GLOBAL: u8 = 3;

/// One key the help overlay or the wizard's key row names: what it reads as, what it does,
/// which screens list it, and when it is worth pressing there.
#[derive(Debug, Clone, Copy)]
pub struct KeyHint {
    /// The key itself: what the wizard's mouse target and a key press both reach for.
    pub code: KeyCode,
    /// What is printed for the key.
    pub label: &'static str,
    /// What is printed for what the key does.
    pub does: &'static str,
    /// The screens that list this key.
    pub screens: &'static [Screen],
    /// When the key is worth pressing.
    pub when: When,
    /// The group this key sits in: navigation, actions, screens or global, in that
    /// fixed order.
    pub(crate) group: u8,
    /// The settings row this key turns the other way, for the uppercase keys that toggle a
    /// mode of the TUI's own: what the key press asks for and what that row names as its
    /// shortcut, both read from here.
    pub toggles: Option<Setting>,
    /// The full word for what the key does, where `does` is an abbreviation of a single word:
    /// the help overlay has room for it and prefers it, and the key row, which has none, keeps
    /// `does`.
    pub long: Option<&'static str>,
}

/// A condition [`available`] answers against the screen a key would be pressed on, for the keys
/// that are not always worth pressing: shown dimmed rather than dropped when it does not hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum When {
    /// Always worth pressing.
    Always,
    /// Only on a profile with a tone control.
    HasEq,
    /// Only on a profile with a tone control, or with presets of its own.
    HasPresets,
    /// Only on a profile that names a mute control.
    HasMute,
    /// Only on a profile with a writable phase toggle.
    HasPhase,
    /// Only on a profile with a writable NOS toggle.
    HasNos,
    /// Only on a profile with a balance control.
    HasBalance,
    /// Only on a profile with a volume control.
    HasVolume,
    /// Only where a toggle or one of the host's rows is there for the key to press.
    HasToggle,
    /// Only on a device whose profile offers the identify cue.
    CanIdentify,
    /// Only on a wizard step that offers choices.
    Choosing,
    /// Only on a wizard step that takes a typed line.
    Typing,
    /// Only on a wizard step the wizard can still go back from.
    Backable,
    /// Only while the Settings box is on its MCP token row.
    TokenRow,
    /// Only on a wizard step with more to say than the screen is showing.
    Scrolling,
}

/// What the keys that are not always worth pressing are answered against.
///
/// The screens take these from the [`App`] they draw; the setup wizard, which has no app behind
/// it, says them for itself — which is what lets one key table serve both.
#[derive(Debug, Clone, Copy, Default)]
pub struct Scene {
    /// Whether the device being shown has a tone control.
    pub has_eq: bool,
    /// Whether it has presets to list: a tone control's, or its own.
    pub has_presets: bool,
    /// Whether it has a mute the keys can turn.
    pub has_mute: bool,
    /// Whether it has a phase toggle the keys can flip.
    pub has_phase: bool,
    /// Whether it has a NOS toggle the keys can flip.
    pub has_nos: bool,
    /// Whether it has a balance the keys can centre.
    pub has_balance: bool,
    /// Whether it has no volume for the keys to move: a flag that reads false by default, so a
    /// screen that says nothing of it leaves the key worth pressing.
    pub no_volume: bool,
    /// Whether it has no toggle and shows no host rows for the key to press: false by default,
    /// like [`Scene::no_volume`].
    pub no_toggle: bool,
    /// Whether its profile offers no identify cue: a flag that reads false by default, so a
    /// screen that says nothing of it leaves the key worth pressing.
    pub no_identify: bool,
    /// Whether the step being shown takes a typed line rather than a choice.
    pub typing: bool,
    /// Whether that line is a row the arrows step between its answers, as the Passphrase step's
    /// is, rather than a cursor along text.
    pub stepping: bool,
    /// Whether the step being shown has more to say than the screen is showing of it.
    pub scrolls: bool,
    /// Whether the wizard can still go back from the step being shown.
    pub can_go_back: bool,
    /// How many bands the device's tone control has, which is how far the band keys reach;
    /// none without a tone control.
    pub bands: usize,
    /// Whether `q` asks before it quits, which is what its description says.
    pub confirms_quit: bool,
    /// Whether the Settings box is on its MCP token row.
    pub on_token_row: bool,
}

impl Scene {
    /// What `app` answers these against. An app is always on a screen that chooses: typing is
    /// the wizard's own state.
    pub fn of(app: &App) -> Scene {
        Scene {
            has_eq: app.profile.eq.is_some(),
            has_presets: app.profile.eq.is_some() || device_presets::offered(&app.profile),
            has_mute: app.mutes(),
            has_phase: app.flips("phase"),
            has_nos: app.flips("nos"),
            has_balance: app.balance().is_some(),
            no_volume: app.volume().is_none(),
            no_toggle: !app.has_toggle_or_host_rows(),
            no_identify: !app.identify_offered(),
            typing: false,
            stepping: false,
            scrolls: false,
            can_go_back: false,
            bands: app.profile.eq.as_ref().map_or(0, |eq| eq.bands.len()),
            confirms_quit: app.tui.confirm_quit,
            on_token_row: app
                .overlays
                .settings
                .is_some_and(|at| crate::tui::screens::settings::current(at) == Setting::McpToken),
        }
    }
}

/// The screens a running TUI shows, which is where a key that means the same thing everywhere
/// is listed. The wizard is not among them: it is a screen of its own, before there is a device
/// to act on, and reads its own few keys.
const EVERY_SCREEN: &[Screen] = &[
    Screen::Device,
    Screen::Tone,
    Screen::Telemetry,
    Screen::Devices,
];

/// The screens and the Settings box, which is the section `Settings` names: where the keys the
/// box takes as well as the screens do are listed.
const EVERY_SCREEN_AND_BOX: &[Screen] = &[
    Screen::Device,
    Screen::Tone,
    Screen::Telemetry,
    Screen::Devices,
    Screen::Settings,
];

/// What the table calls the digits that pick a band, which the wizard's key row and the help
/// overlay write as the digits the profile on the screen has bands for: see
/// [`KeyHint::label_in`].
const BAND_KEYS: &str = "1-N";

/// What the table says `p` does.
const PRESETS: &str = "presets";

/// What the help overlay says `p` does where the list is the tone control's, whose own keys it
/// names.
const PRESETS_LONG: &str = "presets (a i h inside)";

/// What the table says `q` does, which [`KeyHint::does_in`] writes as [`QUIT_ASKS`] where the
/// settings have it ask first.
const QUIT: &str = "quit";

/// What `q` does where it asks before it quits: the ellipsis a menu item that opens a question
/// carries.
const QUIT_ASKS: &str = "quit…";

/// How the table writes `Enter`.
const ENTER: &str = "Enter";

/// The band keys as a screen writes them for a tone control of one to nine bands: `1` for a
/// single band, `1-N` up to nine. A tenth band has no digit of its own, so a longer tone
/// control is reached by the first nine.
const BAND_LABELS: [&str; 9] = ["1", "1-2", "1-3", "1-4", "1-5", "1-6", "1-7", "1-8", "1-9"];

impl KeyHint {
    /// What a screen that looks like `scene` prints for this key: the table's own label, but
    /// for the band keys, which read one to as many bands as the device has — `1-3` on a
    /// three-band tone control, `1-5` on a five-band one — and keep the table's `1-N` where
    /// there is no tone control to count.
    pub fn label_in(&self, scene: Scene) -> &'static str {
        match self.label {
            BAND_KEYS => match scene.bands {
                0 => BAND_KEYS,
                bands => BAND_LABELS[bands.min(BAND_LABELS.len()) - 1],
            },
            label => label,
        }
    }

    /// What the help overlay says this key does on a screen that looks like `scene`: the full
    /// word where the table has one, and otherwise what the key row says.
    pub fn long_in(&self, scene: Scene) -> &'static str {
        match self.long {
            // The keys the tone control's list names on itself exist only where it does.
            Some(PRESETS_LONG) if !scene.has_eq => PRESETS,
            Some(long) => long,
            None => self.does_in(scene),
        }
    }

    /// What a screen that looks like `scene` says this key does: the table's own words, but for
    /// `q`, which says it asks first where the settings have it ask, and for the wizard's `←→`,
    /// which cycle a row's answers where they would move along a typed line.
    pub fn does_in(&self, scene: Scene) -> &'static str {
        match self.does {
            QUIT if scene.confirms_quit => QUIT_ASKS,
            "move" if self.code == KeyCode::Right && scene.stepping => "cycle",
            does => does,
        }
    }
}

/// Every key `App::key` reads, in no particular order; [`hints_for`] is what puts a screen's
/// own in key-row order.
static HINTS: &[KeyHint] = &[
    // Navigation: the arrow keys, wherever a screen gives them something to move or change.
    KeyHint {
        code: KeyCode::Down,
        label: "↑↓",
        does: "move",
        screens: &[
            Screen::Device,
            Screen::Telemetry,
            Screen::Devices,
            Screen::Settings,
        ],
        when: When::Always,
        group: NAV,
        toggles: None,
        long: None,
    },
    KeyHint {
        code: KeyCode::Right,
        label: "←→",
        does: "change",
        screens: &[Screen::Device, Screen::Settings],
        when: When::Always,
        group: NAV,
        toggles: None,
        long: None,
    },
    KeyHint {
        code: KeyCode::Down,
        label: "↑↓",
        does: "move",
        screens: &[Screen::Tone],
        when: When::Always,
        group: NAV,
        toggles: None,
        long: None,
    },
    KeyHint {
        code: KeyCode::Right,
        label: "←→",
        does: "change",
        screens: &[Screen::Tone],
        when: When::Always,
        group: NAV,
        toggles: None,
        long: None,
    },
    KeyHint {
        code: KeyCode::Right,
        label: "←→",
        does: "raw ±1",
        screens: &[Screen::Telemetry],
        when: When::Always,
        group: NAV,
        toggles: None,
        long: None,
    },
    // The wizard: the arrows move between a step's choices, or along the line a step that
    // takes one is being typed into.
    KeyHint {
        code: KeyCode::Down,
        label: "↑↓",
        does: "choose",
        screens: &[Screen::Setup],
        when: When::Choosing,
        group: NAV,
        toggles: None,
        long: None,
    },
    KeyHint {
        code: KeyCode::Right,
        label: "←→",
        does: "move",
        screens: &[Screen::Setup],
        when: When::Typing,
        group: NAV,
        toggles: None,
        long: None,
    },
    KeyHint {
        code: KeyCode::PageDown,
        label: "PgUp/PgDn",
        does: "scroll",
        screens: &[Screen::Setup],
        when: When::Scrolling,
        group: NAV,
        toggles: None,
        long: None,
    },
    // Actions: what a screen's own keys do to the row or band the cursor is on. On the device
    // screen `Space` and `Enter` are one key: a toggle turned, the host's mute turned, or the
    // transport button under the cursor pressed.
    KeyHint {
        code: KeyCode::Char(' '),
        label: "␣/Enter",
        does: "toggle/press",
        screens: &[Screen::Device],
        when: When::HasToggle,
        group: ACTIONS,
        toggles: None,
        long: None,
    },
    KeyHint {
        code: KeyCode::Char('+'),
        label: "+/=/-",
        does: "vol",
        screens: &[Screen::Device],
        when: When::HasVolume,
        group: ACTIONS,
        toggles: None,
        long: Some("volume"),
    },
    KeyHint {
        code: KeyCode::Char('0'),
        label: "0",
        does: "centre balance",
        screens: &[Screen::Device],
        when: When::HasBalance,
        group: ACTIONS,
        toggles: None,
        long: None,
    },
    KeyHint {
        code: KeyCode::Char('m'),
        label: "m",
        does: "mute",
        screens: EVERY_SCREEN,
        when: When::HasMute,
        group: ACTIONS,
        toggles: None,
        long: None,
    },
    KeyHint {
        code: KeyCode::Char('i'),
        label: "i",
        does: "phase",
        screens: EVERY_SCREEN,
        when: When::HasPhase,
        group: ACTIONS,
        toggles: None,
        long: None,
    },
    KeyHint {
        code: KeyCode::Char('o'),
        label: "o",
        does: "NOS",
        screens: EVERY_SCREEN,
        when: When::HasNos,
        group: ACTIONS,
        toggles: None,
        long: None,
    },
    KeyHint {
        code: KeyCode::Tab,
        label: "Tab/⇧Tab",
        does: "pane",
        screens: &[Screen::Device, Screen::Tone],
        when: When::HasEq,
        group: ACTIONS,
        toggles: None,
        long: None,
    },
    KeyHint {
        code: KeyCode::Char('2'),
        label: BAND_KEYS,
        does: "band",
        screens: &[Screen::Tone],
        when: When::HasEq,
        group: ACTIONS,
        toggles: None,
        long: None,
    },
    KeyHint {
        code: KeyCode::Char('.'),
        label: ", . ⇧←→",
        does: "freq",
        screens: &[Screen::Tone],
        when: When::HasEq,
        group: ACTIONS,
        toggles: None,
        long: Some("frequency"),
    },
    KeyHint {
        code: KeyCode::Char(']'),
        label: "[ ] ⇧↑↓",
        does: "Q",
        screens: &[Screen::Tone],
        when: When::HasEq,
        group: ACTIONS,
        toggles: None,
        long: None,
    },
    // The line the focused band's frequency, gain and Q are typed into, in place of its row: a
    // modal of its own, which names its own keys on itself.
    KeyHint {
        code: KeyCode::Enter,
        label: ENTER,
        does: "edit band",
        screens: &[Screen::Tone],
        when: When::HasEq,
        group: ACTIONS,
        toggles: None,
        long: None,
    },
    // A flat curve: every band at 0 dB on its centre frequency and centre Q, as the Flat
    // preset. The device screen's `0` centres the balance instead, which is the same "back to
    // neutral" for what it shows.
    KeyHint {
        code: KeyCode::Char('0'),
        label: "0",
        does: "flatten",
        screens: &[Screen::Tone],
        when: When::HasEq,
        group: ACTIONS,
        toggles: None,
        long: None,
    },
    // The tone control's bypass, from the tone-control screen and from the device screen's
    // Tone pane: toggles, the bands kept either way, so a mode back to EQ brings them back.
    KeyHint {
        code: KeyCode::Char('f'),
        label: "f",
        does: "flat on/off",
        screens: &[Screen::Device, Screen::Tone],
        when: When::HasEq,
        group: ACTIONS,
        toggles: None,
        long: None,
    },
    // The preset list, from the tone-control screen and from the device screen's Tone pane, or the
    // device's own presets from the device screen: a box of its own over either, which names its
    // own keys on itself.
    KeyHint {
        code: KeyCode::Char('p'),
        label: "p",
        does: "presets",
        screens: &[Screen::Device, Screen::Tone],
        when: When::HasPresets,
        group: ACTIONS,
        toggles: None,
        long: Some(PRESETS_LONG),
    },
    KeyHint {
        code: KeyCode::Enter,
        label: ENTER,
        does: "switch",
        screens: &[Screen::Devices],
        when: When::Always,
        group: ACTIONS,
        toggles: None,
        long: None,
    },
    KeyHint {
        code: KeyCode::Char('n'),
        label: "n",
        does: "new",
        screens: &[Screen::Devices],
        when: When::Always,
        group: ACTIONS,
        toggles: None,
        long: None,
    },
    KeyHint {
        code: KeyCode::Char('u'),
        label: "u",
        does: "update",
        screens: &[Screen::Devices],
        when: When::Always,
        group: ACTIONS,
        toggles: None,
        long: None,
    },
    KeyHint {
        code: KeyCode::Char('x'),
        label: "x",
        does: "remove",
        screens: &[Screen::Devices],
        when: When::Always,
        group: ACTIONS,
        toggles: None,
        long: None,
    },
    // Whatever a wizard step asks for, `Enter` is what answers it: the choice the cursor is
    // on, or the line as it has been typed.
    KeyHint {
        code: KeyCode::Enter,
        label: ENTER,
        does: "confirm",
        screens: &[Screen::Setup],
        when: When::Always,
        group: ACTIONS,
        toggles: None,
        long: None,
    },
    KeyHint {
        code: KeyCode::Char(' '),
        label: "␣/Enter",
        does: "toggle",
        screens: &[Screen::Settings],
        when: When::Always,
        group: ACTIONS,
        toggles: None,
        long: None,
    },
    // The Connect category's token row: `R` replaces the token there, and is the rainbow-scroll
    // toggle on every other row (its own hint, above, turns that row). These three are the
    // row's own keys, worth pressing only on it, and the one uppercase key that is not a toggle.
    KeyHint {
        code: KeyCode::Char('v'),
        label: "v",
        does: "show token",
        screens: &[Screen::Settings],
        when: When::TokenRow,
        group: ACTIONS,
        toggles: None,
        long: None,
    },
    KeyHint {
        code: KeyCode::Char('c'),
        label: "c",
        does: "copy token",
        screens: &[Screen::Settings],
        when: When::TokenRow,
        group: ACTIONS,
        toggles: None,
        long: None,
    },
    KeyHint {
        code: KeyCode::Char('R'),
        label: "R",
        does: "new token",
        screens: &[Screen::Settings],
        when: When::TokenRow,
        group: ACTIONS,
        toggles: None,
        long: None,
    },
    // The Settings box's categories: `Tab` and `⇧Tab` cycle them, the digits pick one.
    KeyHint {
        code: KeyCode::Tab,
        label: "Tab/⇧Tab",
        does: "category",
        screens: &[Screen::Settings],
        when: When::Always,
        group: ACTIONS,
        toggles: None,
        long: None,
    },
    KeyHint {
        code: KeyCode::Char('1'),
        label: "1-5",
        does: "category",
        screens: &[Screen::Settings],
        when: When::Always,
        group: ACTIONS,
        toggles: None,
        long: None,
    },
    // Ask the unit to show itself: on the Devices list, the one under the cursor.
    KeyHint {
        code: KeyCode::Char('b'),
        label: "b",
        does: "identify",
        screens: EVERY_SCREEN,
        when: When::CanIdentify,
        group: ACTIONS,
        toggles: None,
        long: None,
    },
    KeyHint {
        code: KeyCode::Char('r'),
        label: "r",
        does: "refresh",
        screens: EVERY_SCREEN,
        when: When::Always,
        group: ACTIONS,
        toggles: None,
        long: None,
    },
    // Screens: the keys that switch to another screen, listed everywhere but the screen they
    // switch to.
    KeyHint {
        code: KeyCode::Esc,
        label: "Esc",
        does: "back, or the menu",
        screens: &[
            Screen::Tone,
            Screen::Telemetry,
            Screen::Devices,
            Screen::Settings,
        ],
        when: When::Always,
        group: SCREENS,
        toggles: None,
        long: None,
    },
    // In the wizard it is the step before this one rather than another screen, which is the
    // same word for the same key; once the device is written there is no step to return to.
    KeyHint {
        code: KeyCode::Esc,
        label: "Esc",
        does: "back",
        screens: &[Screen::Setup],
        when: When::Backable,
        group: SCREENS,
        toggles: None,
        long: None,
    },
    KeyHint {
        code: KeyCode::Char('c'),
        label: "c",
        does: "controls",
        screens: &[Screen::Tone, Screen::Telemetry, Screen::Devices],
        when: When::Always,
        group: SCREENS,
        toggles: None,
        long: None,
    },
    KeyHint {
        code: KeyCode::Char('e'),
        label: "e",
        does: "tone",
        screens: &[Screen::Device, Screen::Telemetry, Screen::Devices],
        when: When::HasEq,
        group: SCREENS,
        toggles: None,
        long: None,
    },
    KeyHint {
        code: KeyCode::Char('t'),
        label: "t",
        does: "telem",
        screens: &[Screen::Device, Screen::Tone, Screen::Devices],
        when: When::Always,
        group: SCREENS,
        toggles: None,
        long: Some("telemetry"),
    },
    // The device list is where a device is added, changed and taken away as well as switched
    // to, so it is worth reaching with one device configured, or with none.
    KeyHint {
        code: KeyCode::Char('d'),
        label: "d",
        does: "devs",
        screens: &[Screen::Device, Screen::Tone, Screen::Telemetry],
        when: When::Always,
        group: SCREENS,
        toggles: None,
        long: Some("devices"),
    },
    KeyHint {
        code: KeyCode::Char('s'),
        label: "s",
        does: "settings",
        screens: &[
            Screen::Device,
            Screen::Tone,
            Screen::Telemetry,
            Screen::Devices,
        ],
        when: When::Always,
        group: SCREENS,
        toggles: None,
        long: None,
    },
    // Global: the keys that mean the same thing everywhere. The uppercase ones turn a mode of
    // the TUI's own the other way and write it to the configuration file, which is the same
    // switch the Settings box shows for it.
    KeyHint {
        code: KeyCode::Char('M'),
        label: "M",
        does: "toggle mouse capture",
        screens: EVERY_SCREEN_AND_BOX,
        when: When::Always,
        group: GLOBAL,
        toggles: Some(Setting::Mouse),
        long: None,
    },
    KeyHint {
        code: KeyCode::Char('E'),
        label: "E",
        does: "toggle effects",
        screens: EVERY_SCREEN_AND_BOX,
        when: When::Always,
        group: GLOBAL,
        toggles: Some(Setting::Effects),
        long: None,
    },
    KeyHint {
        code: KeyCode::Char('H'),
        label: "H",
        does: "toggle hue drift",
        screens: EVERY_SCREEN_AND_BOX,
        when: When::Always,
        group: GLOBAL,
        toggles: Some(Setting::HueDrift),
        long: None,
    },
    KeyHint {
        code: KeyCode::Char('R'),
        label: "R",
        does: "toggle the rainbow scroll",
        screens: EVERY_SCREEN_AND_BOX,
        when: When::Always,
        group: GLOBAL,
        toggles: Some(Setting::SpectrumDrift),
        long: None,
    },
    KeyHint {
        code: KeyCode::Char('F'),
        label: "F",
        does: "toggle runner",
        screens: EVERY_SCREEN_AND_BOX,
        when: When::Always,
        group: GLOBAL,
        toggles: Some(Setting::Effect(EffectSwitch::Frames)),
        long: None,
    },
    KeyHint {
        code: KeyCode::Char('V'),
        label: "V",
        does: "toggle the title meter",
        screens: EVERY_SCREEN_AND_BOX,
        when: When::Always,
        group: GLOBAL,
        toggles: Some(Setting::Key(&TITLE_METER)),
        long: None,
    },
    // The advice line is drawn in the device screen's Status pane and nowhere else, so the key
    // that turns it is named there and nowhere else. It says whose advice it is, for a reader
    // who has never had a fault to be advised about.
    KeyHint {
        code: KeyCode::Char('a'),
        label: "a",
        does: "advice, or all clear",
        screens: &[Screen::Device],
        when: When::Always,
        group: GLOBAL,
        toggles: None,
        long: None,
    },
    KeyHint {
        code: KeyCode::Char('?'),
        label: "?",
        does: "help",
        screens: EVERY_SCREEN_AND_BOX,
        when: When::Always,
        group: GLOBAL,
        toggles: None,
        long: None,
    },
    KeyHint {
        code: KeyCode::Char('q'),
        label: "q",
        does: QUIT,
        screens: EVERY_SCREEN_AND_BOX,
        when: When::Always,
        group: GLOBAL,
        toggles: None,
        long: None,
    },
    // On a wizard step that takes a typed line, `q` is a letter of whatever is being typed;
    // Ctrl-C leaves from there, as it does from every wait.
    KeyHint {
        code: KeyCode::Char('q'),
        label: "q",
        does: QUIT,
        screens: &[Screen::Setup],
        when: When::Choosing,
        group: GLOBAL,
        toggles: None,
        long: None,
    },
];

/// Every key the table names, whichever screen takes it.
///
/// This is the whole of what the TUI listens for, which is what the rules the help overlay
/// prints are checked against and what the documentation is written from.
pub fn all() -> &'static [KeyHint] {
    HINTS
}

/// The key that turns `setting` the other way from any screen, for a settings row one of the
/// uppercase keys also turns, and `None` for a row only the Settings box changes.
pub fn toggled_by(setting: Setting) -> Option<&'static KeyHint> {
    HINTS.iter().find(|hint| hint.toggles == Some(setting))
}

/// The settings row `code` turns the other way, for the uppercase keys that toggle a TUI-wide
/// mode, and `None` for every other key.
pub fn setting_turned_by(code: KeyCode) -> Option<Setting> {
    HINTS
        .iter()
        .find(|hint| hint.code == code && hint.toggles.is_some())
        .and_then(|hint| hint.toggles)
}

/// The keys `screen` lists, in key-row order: navigation, then actions, then the keys
/// that switch to another screen, then the keys that mean the same thing everywhere.
pub fn hints_for(screen: Screen) -> Vec<&'static KeyHint> {
    let mut hints: Vec<&'static KeyHint> = HINTS
        .iter()
        .filter(|hint| hint.screens.contains(&screen))
        .collect();
    hints.sort_by_key(|hint| hint.group);
    hints
}

/// The keys more than one of the running screens takes, in key-row order.
///
/// A key several screens list means the same thing on each of them, so the help overlay names
/// these once under a heading of their own rather than under every screen that takes them.
pub fn shared() -> Vec<&'static KeyHint> {
    let mut hints: Vec<&'static KeyHint> = HINTS.iter().filter(|hint| shared_key(hint)).collect();
    hints.sort_by_key(|hint| hint.group);
    hints
}

/// The keys `screen` alone takes, in key-row order: what it does that no other screen does,
/// which is the whole of what the help overlay lists under its name.
pub fn only_on(screen: Screen) -> Vec<&'static KeyHint> {
    hints_for(screen)
        .into_iter()
        .filter(|hint| !shared_key(hint))
        .collect()
}

/// Whether more than one of the sections the overlay opens over lists `hint`. The wizard is
/// left out of the count, as it is out of [`EVERY_SCREEN_AND_BOX`]: its keys answer a step
/// rather than a screen the overlay opens over.
fn shared_key(hint: &KeyHint) -> bool {
    EVERY_SCREEN_AND_BOX
        .iter()
        .filter(|screen| hint.screens.contains(screen))
        .count()
        > 1
}

/// Whether the help overlay leaves `hint` out on a screen that looks like `scene`: the keys
/// for a control a device either has or never will (phase, NOS) are not listed where it has
/// none, where the keys that merely depend on the screen's state stay listed, dimmed.
pub fn hidden_in(hint: &KeyHint, scene: Scene) -> bool {
    matches!(hint.when, When::HasPhase | When::HasNos) && !available_in(hint, scene)
}

/// Whether `hint` is worth pressing on `app` now. A hint that answers `false` still lists in
/// the wizard's key row and the help overlay, dimmed rather than dropped: the reason is worth
/// knowing even where it does not hold.
pub fn available(hint: &KeyHint, app: &App) -> bool {
    available_in(hint, Scene::of(app))
}

/// Whether `hint` is worth pressing on a screen that looks like `scene`, which is the same
/// question [`available`] asks of an app and the one the wizard asks of the step it is on.
pub fn available_in(hint: &KeyHint, scene: Scene) -> bool {
    match hint.when {
        When::Always => true,
        When::HasEq => scene.has_eq,
        When::HasPresets => scene.has_presets,
        When::HasMute => scene.has_mute,
        When::HasPhase => scene.has_phase,
        When::HasNos => scene.has_nos,
        When::HasBalance => scene.has_balance,
        When::HasVolume => !scene.no_volume,
        When::HasToggle => !scene.no_toggle,
        When::CanIdentify => !scene.no_identify,
        When::Choosing => !scene.typing,
        When::Typing => scene.typing,
        When::Backable => scene.can_go_back,
        When::Scrolling => scene.scrolls,
        When::TokenRow => scene.on_token_row,
    }
}

#[cfg(test)]
mod tests;
