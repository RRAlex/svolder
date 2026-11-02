// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! How the TUI looks: the `[tui]` table and the theme and animation layer built from it, and
//! what a frame does with them — the accent a device's family and level resolve to, the
//! animations a frame just drawn earns, and how long the loop waits before the next one.

use std::path::Path;
use std::time::{Duration, Instant};

use ratatui::layout::Rect;
use ratatui::style::Color;

use svolder_ipc::config::RefreshWeeks;
use svolder_ipc::{ColorDepth, Config, TuiConfig};
use svolder_proto::FunctionId;

use ratatui::Frame;

use crate::tui::app::{App, METER_PERIOD, Screen, TuiChange};
use crate::tui::boxes::toast;
use crate::tui::boxes::tooltip;
use crate::tui::decoration::effects::{self, Effects};
use crate::tui::render::FrameAreas;
use crate::tui::terminal::{FrameSync, MouseCapture};
use crate::tui::theme::{Role, Theme};
use crate::tui::widgets;
use crate::tui::widgets::spinner;

/// How long the loop waits between ticks while the animation layer has something to move:
/// about thirty frames a second.
const FAST_TICK: Duration = Duration::from_millis(33);

/// Where a copy goes: it is handed the whole escape sequence and answers whether the write
/// took.
pub(in crate::tui) type Clipboard = Box<dyn FnMut(&str) -> std::io::Result<()>>;

/// Where the bytes that make the Web UI box's link a hyperlink go, after the frame that drew it.
pub(in crate::tui) type LinkOut = Box<dyn FnMut(&[u8])>;

/// The process environment as the TUI reads it: the value of a variable, if set.
pub(in crate::tui) type Env =
    std::sync::Arc<dyn Fn(&str) -> Option<std::ffi::OsString> + Send + Sync>;

/// What puts a text on the clipboard of the display the TUI runs under, with the tool that did,
/// or why none could. It runs on a thread of its own, as a tool can take seconds to come back.
pub(in crate::tui) type Copier =
    std::sync::Arc<dyn Fn(&str) -> crate::tui::clipboard::Tried + Send + Sync>;

/// What opens a link in a browser: it answers why not, in words, when it could not. It runs on a
/// thread of its own, as an opener can take seconds to come back.
pub(in crate::tui) type Browser = std::sync::Arc<dyn Fn(&str) -> Result<(), String> + Send + Sync>;

/// How the TUI looks, and what a change to that is written back to.
///
/// The `[tui]` table is held here rather than read once at startup, because the Settings box
/// changes it while the loop is running: every change is taken into the table, applied to the
/// theme and the animation layer built from it, and saved to the configuration file. The
/// terminal's own answers — whether colour was refused, and the depth it reported — are kept
/// alongside, since a change to `colors` or `monochrome` resolves against them again.
pub(in crate::tui) struct Look {
    /// The `[tui]` table as this session holds it.
    pub(in crate::tui) tui: TuiConfig,
    /// The styles the screens draw with, rebuilt from `tui` whenever it changes.
    pub(in crate::tui) theme: Theme,
    /// The animation layer, pointed at `tui`'s switches whenever they change.
    pub(in crate::tui) effects: Effects,
    /// Whether colour was refused outright, by `--no-color` or `NO_COLOR`.
    no_color: bool,
    /// The colour depth the terminal reported, which `colors = "auto"` resolves to.
    detected: ColorDepth,
    /// The configuration file a change is written to.
    path: std::path::PathBuf,
    /// Where the Connect category looks for the MCP token.
    pub(in crate::tui) token_site: super::TokenSite,
    /// Where a copy of the token, a link or a pairing code goes first: the terminal, as an OSC 52
    /// sequence.
    pub(in crate::tui) clipboard: Clipboard,
    /// The environment the Web UI box's copies read: whether tmux or screen is around the
    /// terminal, and which display there is.
    pub(in crate::tui) env: Env,
    /// Where a copy goes besides the terminal: the display's clipboard tool.
    pub(in crate::tui) copier: Copier,
    /// Where a click on the Web UI box's link opens it: the desktop's `xdg-open`, never on the
    /// terminal.
    pub(in crate::tui) browser: Browser,
    /// Where the escape sequences that make the Web UI box's link a terminal hyperlink go.
    pub(in crate::tui) link_out: LinkOut,
    /// What brackets the loop's frames in synchronized output while the intro's exit plays.
    pub(in crate::tui) sync: FrameSync,
}

impl Look {
    /// The look `tui` asks for on a terminal that reported `detected`, with colour refused or
    /// not, saving every change to `path`.
    pub(in crate::tui) fn new(
        tui: TuiConfig,
        no_color: bool,
        detected: ColorDepth,
        path: std::path::PathBuf,
    ) -> Look {
        let theme = Theme::from_config(&tui, no_color, detected);
        let mut effects = Effects::new(shows_effects(&theme), &tui.effects);
        effects.set_transition(tui.transition);
        Look {
            tui,
            theme,
            effects,
            no_color,
            detected,
            path,
            token_site: super::TokenSite::process(),
            clipboard: Box::new(|sequence| {
                crate::tui::decoration::title::emit(sequence);
                Ok(())
            }),
            env: std::sync::Arc::new(|name| std::env::var_os(name)),
            copier: std::sync::Arc::new(|text| {
                let var = |name: &str| std::env::var_os(name);
                crate::tui::clipboard::copy_with_tool(
                    &var,
                    &|program| crate::tui::clipboard::on_path(&var, program),
                    &crate::tui::clipboard::run_tool,
                    text,
                )
            }),
            browser: std::sync::Arc::new(|link| {
                let desktop = super::web_box::desktop_session(|name| std::env::var_os(name));
                super::web_box::open_in_browser(desktop, Path::new("xdg-open"), link)
            }),
            link_out: Box::new(|bytes| {
                use std::io::Write;
                let mut out = std::io::stdout();
                let _ = out.write_all(bytes);
                let _ = out.flush();
            }),
            sync: crate::tui::terminal::no_sync(),
        }
    }

    /// Take `change` into the session's `[tui]` table, apply it to everything drawn from it,
    /// and write the change into the configuration file.
    ///
    /// What is drawn comes from the session's own table; what is written comes from the file:
    /// the file's `[tui]` is read as it stands, the one change applied to it, and only the
    /// keys the change alters are rewritten, every other byte of the file staying as it was, so
    /// a key edited by hand since the TUI started stays and a deleted file returns as the
    /// template with this change alone.
    ///
    /// The setting applies whether or not it can be saved: a file that could not be written is
    /// reported in a toast, and what the user asked for stands for the rest of the
    /// session. The exception is a mouse toggle the terminal refuses: nothing changes, nothing
    /// is written, and a toast says capture did not change.
    pub(super) fn change<M: MouseCapture>(
        &mut self,
        change: &TuiChange,
        app: &mut App,
        mouse: &mut M,
    ) {
        if let TuiChange::Mouse(on) = change {
            if mouse.set_mouse(*on).is_err() {
                app.warn("mouse capture did not change: the terminal refused");
                return;
            }
            app.mouse = *on;
        }
        change.apply(&mut self.tui);
        self.redraw(app);
        // A palette or a depth is every colour on the screen at once, which is what the sweep a
        // screen change earns marks here too; the Settings box is drawn in the theme's own
        // colours and shows the change on the next frame, so a change made in it sweeps nothing.
        app.frame_flags.screen_changed |= change.sweeps() && app.overlays.settings.is_none();
        if let Err(e) = Config::update_tui(&self.path, |tui| change.apply(tui)) {
            app.warn(format!("settings not saved: {e}"));
        }
    }

    /// Take the AutoEq refresh `weeks` into the app and write it to `[autoeq]` of the file, the
    /// way [`Look::change`] writes `[tui]`: it applies whether or not the file could be written,
    /// and a file that could not be is said in a toast.
    pub(super) fn change_autoeq(&mut self, weeks: RefreshWeeks, app: &mut App) {
        app.presets.autoeq_weeks = weeks;
        if let Err(e) = Config::update_autoeq(&self.path, |autoeq| autoeq.refresh_weeks = weeks) {
            app.warn(format!("settings not saved: {e}"));
        }
    }

    /// Point the theme and the animation layer at the `[tui]` table as it stands, and the app
    /// at what the two of them answer.
    fn redraw(&mut self, app: &mut App) {
        self.theme = Theme::from_config(&self.tui, self.no_color, self.detected);
        self.theme.set_family(app.profile.family);
        // What changes is what the next event arms, and whether a terminal that can no longer
        // show the animations arms anything at all; a layer left switched off drops whatever
        // was mid-animation, which is what puts the loop back on its slow tick.
        self.effects.set_capable(shows_effects(&self.theme));
        self.effects.configure(&self.tui.effects);
        self.effects.set_transition(self.tui.transition);
        app.set_look(&self.tui, &self.effects);
        app.mascot.capable = self.mascot_capable();
    }

    /// Whether the terminal can show the squirrel's colours: 256 or more, and not monochrome.
    pub(in crate::tui) fn mascot_capable(&self) -> bool {
        matches!(
            self.theme.depth(),
            ColorDepth::TrueColor | ColorDepth::Ansi256
        ) && !self.theme.monochrome()
    }
}

/// Whether a terminal can show the animation layer at all.
///
/// tachyonfx writes RGB colours, which a 256-colour terminal cannot show faithfully and a
/// monochrome screen has none of.
fn shows_effects(theme: &Theme) -> bool {
    theme.depth() == ColorDepth::TrueColor && !theme.monochrome()
}

/// Point the theme at the device the frame about to be drawn is of, and at how loud what it
/// plays is.
///
/// The accent follows whatever device the loop is watching, so switching device recolours the
/// screen the frame after it opens. The animation layer is told the level the app reads from
/// its one source — the device's meters while they stream, else the capture of the host's sink
/// ([`App::level`]) — and answers where along the palette's spectrum the decoration is
/// drawing, which is nowhere at all unless the hue-drift mode is on: the rule, the key hints
/// and the pane contours travel, and every value the screen carries stays in the accent it
/// rests at.
pub(super) fn colour(look: &mut Look, app: &App) {
    look.theme.set_family(app.profile.family);
    look.effects.set_level(app.level());
    look.theme.set_glow(look.effects.glow_position());
    look.effects.set_surfaces(&look.theme);
}

/// Paint over the frame the widgets just drew: the spectrum over the curve they drew (the
/// curve's own colouring, drawn whether or not the animation layer runs), then whatever
/// tachyonfx is running. Every effect changes how the frame's cells look, never what they say,
/// but for the squirrel's while it waits out the way in of the panes it stands on.
///
/// A frame with a modal box on it has everything outside the box dimmed again after that, so an
/// effect that started before the box opened (a shimmer, a lap, a glow still playing over the
/// area it was armed on) never brightens a cell the box dims. The squirrel's greys under the box
/// are put back over that dimming, and the menu's join row stays undimmed.
///
/// The intro's exit, while it plays, is painted over all of that: it is the whole frame as drawn,
/// the dimming included, with the panes it holds in bare pane grey.
///
/// The noise or melt a screen change is playing comes last of all, over the whole frame as it
/// would otherwise be shown (a box opened while one plays included), since what it covers is
/// the screen the user just moved to, for the moment it lasts. A toast and a tooltip are the
/// exception to all of it: their cells come out of the frame as the widgets drew them.
pub(in crate::tui) fn paint_over(
    frame: &mut Frame,
    areas: &FrameAreas,
    theme: &Theme,
    effects: &mut Effects,
    last_tick: Duration,
) {
    // The toast and the tooltip are set aside first and put back last, so neither an effect,
    // the dimming nor a transition changes a cell of them.
    let toast = areas
        .toast
        .map(|area| toast::lift(frame.buffer_mut(), area));
    let tooltip = areas
        .tooltip
        .map(|area| toast::lift(frame.buffer_mut(), area));
    spectrum_over(frame, areas, theme, effects);
    effects.hold_back(&areas.behind);
    effects.render(frame, last_tick);
    if let Some(modal) = areas.modal {
        let keep: Vec<Rect> = std::iter::once(modal).chain(areas.join).collect();
        widgets::surfaces::dim_around(frame.buffer_mut(), &keep, theme);
        if let Some(sprite) = &areas.sprite {
            sprite.put_back(frame.buffer_mut());
        }
    }
    if effects.reforming() {
        let frames: Vec<Rect> = areas.frames.iter().map(|(_, frame)| *frame).collect();
        effects.paint_reform(frame.buffer_mut(), areas.logo, &frames);
    }
    effects.paint_transition(frame.buffer_mut(), theme, last_tick);
    if let Some(toast) = toast {
        toast.put_back(frame.buffer_mut());
    }
    if let Some(tooltip) = tooltip {
        tooltip.put_back(frame.buffer_mut());
    }
}

/// Colour the spectrum over the curve the widgets drew, if one was drawn: the curve's own
/// colouring, applied whether or not the animation layer runs.
pub(super) fn spectrum_over(
    frame: &mut Frame,
    areas: &FrameAreas,
    theme: &Theme,
    effects: &Effects,
) {
    if let Some(curve) = areas.curve {
        effects::spectrum(
            frame.buffer_mut(),
            curve,
            theme,
            effects.spectrum_hue(),
            areas.curve_muted,
        );
    }
}

/// Start whatever animation the frame just drawn earned, over the areas it drew them in.
///
/// This runs after the frame and after the input that shaped it, so every animation covers
/// exactly what was painted and a key press is never held up by one. Each flag is taken whether
/// the animation layer runs or not, so an event that happened with effects off cannot fire an
/// effect long afterwards.
///
/// The shimmer answers the user and nobody else: the row their own key wrote to, the row the
/// cursor moved onto, or (while they rest) the selected row again on the reminder schedule,
/// which belongs to the shimmer alone. The selected row is a function's row on the device and
/// tone screens and, in every list besides, the row the frame reports in [`FrameAreas::cursor`]
/// (a list or box under an overlay reports none). A reading the device pushed moves its own row
/// and sweeps nothing.
///
/// A screen change announces every frame the new screen drew; a frame the one before it did not
/// draw announces itself wherever it appeared (a pane just added, or the frames of a screen the
/// terminal just found room for). A screen with no frame to trace sweeps its whole body in
/// instead.
///
/// The light runs once round the frame the keys are in, wherever the user just moved them
/// (`Tab`, a click on a pane's title) and on arriving on a screen, once the way in has played
/// out ([`Effects::on_arrival_lap`]), never on the reminder schedule. It runs in the lift of
/// the colour the level reads where there is one (the device's meters while streaming, else the
/// host sink capture), else the lift of the frame's own drifted colour (a brighter form of the
/// contour's colour), at the pace that level asks for — round nothing where the keys are in no
/// frame (a screen that draws no block).
///
/// A modal box (the help overlay, the device editor, the removal or `q` question) is drawn over
/// a screen rather than in place of it, and reports the frames of neither: a frame under it
/// compares nothing, so the panes it covers stay as they were when it lifts and none of them
/// announces itself again.
pub(super) fn arm_effects(
    app: &mut App,
    effects: &mut Effects,
    areas: &FrameAreas,
    theme: &Theme,
    now: Instant,
) {
    effects.set_surfaces(theme);
    effects.drew_curve(areas.curve, areas.curve_muted);
    let appeared = match app.modal_open() {
        true => Vec::new(),
        false => effects.appeared(&areas.frames),
    };
    let frame_flags = app.frame_flags.take();
    let arrived = frame_flags.screen_changed;
    if arrived {
        let frames: Vec<Rect> = areas.frames.iter().map(|(_, frame)| *frame).collect();
        if !effects.on_arrival(&frames) {
            effects.on_screen_change(areas.body);
        }
    } else {
        effects.on_arrival(&appeared);
    }
    if frame_flags.pending_flash.is_some() {
        effects.on_fault(areas.header);
    }
    if frame_flags.reconnected {
        effects.on_reconnect(areas.body);
    }
    let changed: Vec<Rect> = frame_flags
        .changed_rows
        .iter()
        .filter_map(|id| row_rect(areas, *id))
        .collect();
    effects.on_value_changes(&changed, theme.accent());
    let selected = areas.cursor;
    let moved = effects.on_selection(selected.map(|cursor| (cursor.kind, cursor.index)));
    let written = frame_flags.touched_cursor;
    let left = frame_flags.left_row;
    if let Some(row) = left.and_then(|id| row_rect(areas, id)) {
        effects.on_cursor_move(row);
    }
    // The shimmer answers the user: the row a key of theirs wrote to, the row the cursor
    // moved onto, or — while they rest — the selected row, on the reminder schedule. The
    // schedule is the shimmer's own; the light answers a focus change only, below.
    let changed = frame_flags
        .touched_row
        .or_else(|| left.and(app.cursor_id()));
    let reminded = reminding(app, areas) && effects.remind(now);
    let swept = match changed {
        Some(id) => Some(id),
        None if reminded => app.cursor_id(),
        None => None,
    };
    // The selection of a list, which no function names, sweeps in the same three cases: the
    // cursor moved onto its row, a value was written from it, or the reminder came due.
    let listed = selected
        .map(|cursor| cursor.area)
        .filter(|_| moved || written || reminded);
    if let Some(row) = swept.and_then(|id| row_rect(areas, id)).or(listed) {
        effects.on_shimmer(row);
    }
    if effects.wants_reform_lap()
        && let Some(frame) = focused_rect(areas)
    {
        effects.on_reform_lap(frame, theme.glow_lift_at(effects.level_position()));
    }
    let refocused = frame_flags.pane_changed && reminding(app, areas);
    if let Some(frame) = focused_rect(areas) {
        let glow = theme.glow_lift_at(effects.level_position());
        // Arriving on a screen waits for the way in to play out; a focus change within one
        // runs the light at once.
        if arrived && reminding(app, areas) {
            effects.on_arrival_lap(frame, glow);
        } else if refocused {
            effects.on_lap(frame, glow);
        }
    }
    let alert = theme.style(Role::Alert).fg.unwrap_or(Color::Red);
    effects.on_faults(&faulted_rows(app, areas), alert);
}

/// Whether a reminder plays over what the frame is showing, which gates the light running
/// round the focused frame as well as the shimmer's sweep — and the light a focus change earns
/// with it.
///
/// Nothing sweeps or runs under an overlay the user is reading or typing into — the keys, the
/// device editor, the questions a removal and `q` ask — and nothing on the setup wizard, which
/// walks at its own pace. The rows and the contours beneath those are not what the user is
/// looking at. An overlay with a selection of its own (the menu, the Settings box, the preset
/// list) is what the user is looking at, and its selected row is swept.
fn reminding(app: &App, areas: &FrameAreas) -> bool {
    app.screen != Screen::Setup && (!app.modal_open() || areas.cursor.is_some())
}

/// The rows a fault the device is reporting now is written on: the header, which every screen
/// draws, and the Status pane's fault line on the screen that draws one. Nothing at all while
/// the device reports nothing wrong, which is what stops the pulse.
fn faulted_rows(app: &App, areas: &FrameAreas) -> Vec<Rect> {
    if app.faults().is_empty() {
        return Vec::new();
    }
    std::iter::once(areas.header).chain(areas.faults).collect()
}

/// Where the frame just drawn put the block the keys are going to, which is what the light
/// runs round, and nothing at all for a frame whose keys are in no block of its own.
///
/// Every block a screen draws carries its own name, so the one the keys are in is found by
/// that name rather than by where it sits: the block under a name the frame did not draw is no
/// block of that frame's.
fn focused_rect(areas: &FrameAreas) -> Option<Rect> {
    areas
        .frames
        .iter()
        .find(|(id, _)| Some(*id) == areas.focused)
        .map(|(_, frame)| *frame)
}

/// Where the frame just drawn put the row function `id` is shown on, for the screens that
/// draw rows and nothing at all for the ones that do not.
fn row_rect(areas: &FrameAreas, id: FunctionId) -> Option<Rect> {
    areas
        .rows
        .iter()
        .find(|(row, _)| *row == id)
        .map(|(_, rect)| *rect)
}

/// How long the loop waits between ticks: [`FAST_TICK`] while an animation is running, a screen
/// change's noise or melt is playing, a value is gliding to what the device confirmed, the
/// volume is climbing to a target ([`App::climbing`]), a meter's peak marker is falling, the tone
/// curve's spectrum is drifting, the Spectrum row of what the host plays is moving, or a beat of
/// the daemon's frames is pulsing the row or the borders; a hundred milliseconds or faster while
/// a spinner is turning; the meters' own pace
/// ([`METER_PERIOD`]) while the host's capture is the level's source and the breathing borders
/// or hue drift follow it (the pace an amp's meter readings redraw the same decoration at, since
/// a capture block draws no frame of its own); and the app's own slow tick otherwise, which it
/// stays at for the whole run whenever effects are off and nothing is waiting.
///
/// The hue-drift mode is not among the fast-tick reasons: the glow steps with every tick, so it
/// travels at whatever pace the loop already runs at rather than demanding a faster one (the
/// host's level feeding it only asks for the meters' pace, above). Neither is the spectrum under
/// the help overlay: the curve it colours is off-screen while the overlay is up, so nothing is
/// drifting to redraw. Nor is a tone screen whose last frame drew no curve the scroll moves: a
/// terminal too small for any screen, Loudness mode, a muted curve, any box over the screen, or
/// the scroll switched off. The spinner isn't part of that group either — it turns whether or not
/// effects are on, so it is asked for on its own.
pub(super) fn tick_period(app: &App, effects: &Effects) -> Duration {
    let period = base_period(app, effects);
    match app.tooltip_pending() {
        true => period.min(tooltip::PENDING_TICK),
        false => period,
    }
}

/// The loop's period before a pending tooltip shortens it.
fn base_period(app: &App, effects: &Effects) -> Duration {
    if let Some(period) = effects.reform_period() {
        return period;
    }
    let drifting =
        effects.curve_scrolling() && app.screen == Screen::Tone && !app.overlays.help_open;
    let moving = effects.active()
        || effects.transitioning()
        || drifting
        || app.is_easing(app.now)
        || app.climbing()
        || app.peaking(app.now)
        || app.spectrum_moving()
        || app.pulsing()
        || app.mascot_animating();
    if moving {
        return FAST_TICK;
    }
    if app.spinning() {
        return spinner::STEP;
    }
    if app.redraws_host_level() {
        return METER_PERIOD;
    }
    app.tick_period()
}

#[cfg(test)]
mod tests;
