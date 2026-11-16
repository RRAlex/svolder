// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Carrying out what the user asked for: a key press, a click, or an intent, against what
//! the loop is watching.

use super::*;

/// Press `button` on the player the daemon follows, which the daemon does: a toast says why if
/// the daemon refuses. The keys and clicks ask for no press where there is no player.
fn press_through_daemon(watching: &mut Watching, button: svolder_host::Button) {
    let request = svolder_ipc::Request::HostPress {
        button: button.method().to_ascii_lowercase(),
    };
    let ask = watching
        .backend
        .as_ref()
        .and_then(|backend| backend.daemon_request(request));
    match ask {
        Some(ask) => watching.host.press_through(ask),
        None => watching.app.notify(crate::tui::host::NO_PLAYER),
    }
}

/// Queue one write of the preset library for the job that carries it out.
///
/// A daemon that reads another file than this session edits would take the write into its own
/// file and leave the list looking as if it were lost: it is refused in words, as the other
/// configuration writes are.
fn write_preset<O: Open>(opener: &Opener<O>, watching: &mut Watching, op: PresetOp) {
    if let Some(words) = watching.app.overlays.conn.read_only() {
        watching.app.warn(words);
        return;
    }
    let socket = super::devices::daemon_socket(opener, watching);
    watching.jobs.write_preset(jobs::PresetJob {
        socket,
        file: opener.path.clone(),
        op,
    });
}

/// Focus row `row` of whichever preset list is open: the device's own, or the tone control's.
fn focus_list(app: &mut App, row: usize) {
    match app.presets.device_list.is_some() {
        true => device_presets::focus(app, row),
        false => presets::focus(app, row),
    }
}

/// Read the presets from the configuration file again, for the list that has just opened.
fn read_presets<O: Open>(opener: &Opener<O>, watching: &mut Watching) {
    match Config::load_from(&opener.path) {
        Ok(config) => presets::reread(&mut watching.app, config.presets),
        Err(e) => {
            watching.app.presets.landed = None;
            watching
                .app
                .notify_failure(&Failure::of(Kind::ConfigUnreadable, e.to_string()));
        }
    }
}

/// Carry out one intent exactly as the key that means it would, whichever asked for it: a key
/// press, or mouse support reading a click or the wheel back into the same intent.
///
/// `Quit` ends the loop; `Tui` and `SwitchDevice` are this loop's own to act on; the intents that
/// change configuration (a device, the default device, a preset) are written to the file and
/// told to whatever holds the devices; `Refresh` with the feed already closed reopens the device
/// rather than asking it something it cannot answer; everything else dispatches to the device
/// as any other write.
pub(super) async fn take_intent<O: Open, M: MouseCapture>(
    intent: Intent,
    opener: &Opener<O>,
    mut watching: Watching,
    mouse: &mut M,
    look: &mut Look,
    screen: &mut dyn Shown,
) -> Flow {
    match intent {
        Intent::Quit => {
            return Flow::Ended(LoopEnd {
                watching,
                code: 0,
                why: None,
            });
        }
        // A setting is neither a write nor anything the device has heard of: it changes how
        // this loop draws, and is saved where a hand edit would have put it.
        Intent::Tui(change) => {
            look.change(&change, &mut watching.app, mouse);
            // With capture off, a drag button's release goes to the terminal instead, so let
            // the drag go where it last was.
            let let_go = curve_drag::capture_off(&mut watching.app);
            send(let_go, &mut watching).await;
            let let_go = glide::capture_off(&mut watching.app);
            send(let_go, &mut watching).await;
            divider::capture_off(&mut watching.app);
        }
        // The refresh applies whether or not it can be saved, as a `[tui]` change does.
        Intent::AutoEqRefresh(weeks) => {
            look.change_autoeq(weeks, &mut watching.app);
        }
        Intent::Connection(change) => {
            Box::pin(connect::changed(
                opener,
                &mut watching,
                &look.token_site,
                &change,
                look,
                screen,
            ))
            .await;
        }
        Intent::RevealToken => connect::reveal(&mut watching, &look.token_site),
        Intent::CopyToken => {
            connect::copy(&mut watching, look);
        }
        Intent::RotateToken => connect::rotate(&mut watching, &look.token_site).await,
        Intent::WebLink => web_box::asked_by_key(&mut watching, true),
        Intent::WebPair => web_box::asked_by_key(&mut watching, false),
        Intent::CopyWebLink => web_box::copy(&mut watching, look),
        Intent::OpenWebLink => web_box::open(&mut watching, look),
        Intent::RotateWebToken => connect::rotate_web(&mut watching, &look.token_site).await,
        Intent::SwitchDevice(name) => {
            if Box::pin(switch_device(opener, &mut watching, &name, look, screen)).await
                == Switched::GaveUp
            {
                return gave_up(watching);
            }
        }
        Intent::NewDevice => return Flow::Adding(watching),
        Intent::UpdateDevice(name, device) => {
            let switched = Box::pin(update_device(
                opener,
                &mut watching,
                &name,
                &device,
                look,
                screen,
            ))
            .await;
            if switched == Switched::GaveUp {
                return gave_up(watching);
            }
        }
        Intent::RemoveDevice(name) => {
            match Box::pin(remove_device(opener, &mut watching, &name, look, screen)).await {
                Removed::Kept => {}
                Removed::Empty => return Flow::Adding(watching),
                Removed::GaveUp => return gave_up(watching),
            }
        }
        Intent::DefaultDevice(name) => {
            let op = match &name {
                Some(device) => DeviceConfigOp::SetDefault {
                    device: device.clone(),
                },
                None => DeviceConfigOp::ClearDefault,
            };
            write_device(opener, &mut watching, look, screen, op, move |path| {
                Config::set_default_device(path, name.as_deref())
            })
            .await;
        }
        // The preset list's writes are queued: the daemon writes the library while one runs, the
        // file's own writer does when none does, and the loop is never held for either.
        Intent::SavePreset(preset) => {
            let op = PresetOp::Save {
                preset: Box::new(preset),
                force: false,
            };
            write_preset(opener, &mut watching, op);
        }
        Intent::UpdatePreset(name, mut preset) => {
            preset.name = name;
            let op = PresetOp::Update {
                preset: Box::new(preset),
            };
            write_preset(opener, &mut watching, op);
        }
        Intent::RenamePreset(from, to) => {
            write_preset(opener, &mut watching, PresetOp::Rename { from, to });
        }
        Intent::RemovePreset(name) => {
            write_preset(opener, &mut watching, PresetOp::Delete { name });
        }
        // The list shows the file as it stands when it opens, whatever wrote it since.
        Intent::ReadPresets => read_presets(opener, &mut watching),
        Intent::AutoEqSearch { ask, soon } => {
            let call = watching.backend.as_ref().and_then(|backend| {
                let request = crate::autoeq::search_op(&ask.query, ask.refresh);
                let within = Some(crate::autoeq::SEARCH_WAIT);
                let call = backend.daemon_long_request(request, within)?;
                Some(Box::pin(call) as jobs::Call)
            });
            watching.jobs.search(call, ask, soon);
        }
        Intent::AutoEqImport {
            variant,
            name,
            target,
        } => {
            let socket = watching.backend.as_ref().and_then(Backend::daemon_socket);
            watching.jobs.import(socket, *variant, name, target);
        }
        Intent::HostEq(op) => computer::eq(&mut watching, *op),
        Intent::MoveToDac(name) => computer::move_to_dac(&mut watching, name),
        // The computer's volume and mute: a request to the daemon when joined to one, else the
        // loop's own sink. Nothing of it is asked while host audio is not shown.
        Intent::HostVolume(percent) if watching.app.host_shown() => {
            computer::volume(&mut watching, percent);
        }
        Intent::HostMute if watching.app.host_shown() => computer::mute(&mut watching),
        Intent::HostPress(button) if watching.app.host_shown() => {
            if watching.app.host.daemon_owns() {
                press_through_daemon(&mut watching, button);
            } else if !watching.host.press(button) {
                watching.app.notify(crate::tui::host::NO_PLAYER);
            }
        }
        Intent::HostVolume(_) | Intent::HostMute | Intent::HostPress(_) => {}
        // The link is gone for good, so there is nothing to ask: reading the device again is
        // what opening it again does.
        // The same goes for a loop with no device open: reading it again is opening it again.
        Intent::Refresh if !watching.feed_open || watching.app.session.holding.is_some() => {
            // Whatever a wait for the daemon would find, this open looks at it first.
            watching.reconnect = None;
            let landing = matches!(watching.app.session.holding, Some(Holding::Nothing { .. }));
            match Box::pin(switch(opener, &mut watching, None, look, screen)).await {
                Switched::GaveUp => return gave_up(watching),
                // A retry of a start that opened nothing is a start: it lands on the device
                // screen, the way a switch from the list does, not on the list it was asked from.
                Switched::Opened if landing => watching.app.settle_on(Screen::Device),
                Switched::Opened | Switched::Stayed => {}
            }
        }
        other => dispatch(other, &mut watching).await,
    }
    Flow::Continue(watching)
}

/// Carry out one key press: whatever the app reads it as, carried out in the order it asked
/// for it.
///
/// A click on a tab or on a word of a border comes through here too, so a key pressed and a key
/// clicked go the very same way.
pub(super) async fn take_key<O: Open, M: MouseCapture>(
    key: KeyEvent,
    opener: &Opener<O>,
    mut watching: Watching,
    mouse: &mut M,
    look: &mut Look,
    screen: &mut dyn Shown,
) -> Flow {
    let intents = watching.app.key(key);
    take_intents(intents, opener, watching, mouse, look, screen).await
}

/// The end a `q` or Ctrl-C earned while a write of the configuration file was waited for: the
/// write was seen through, and the loop ends now.
fn quit_if_asked(flow: Flow) -> Flow {
    match flow {
        Flow::Continue(watching) if watching.give_up_asked => gave_up(watching),
        other => other,
    }
}

/// Carry out `intents` one after the other as [`take_intent`] does, stopping at the first that
/// ends or changes what is watched.
pub(super) async fn take_intents<O: Open, M: MouseCapture>(
    intents: Vec<Intent>,
    opener: &Opener<O>,
    mut watching: Watching,
    mouse: &mut M,
    look: &mut Look,
    screen: &mut dyn Shown,
) -> Flow {
    for intent in intents {
        match quit_if_asked(take_intent(intent, opener, watching, mouse, look, screen).await) {
            Flow::Continue(next) => watching = next,
            ended => return ended,
        }
    }
    Flow::Continue(watching)
}

/// Carry out one [`MouseAction`] that mouse support resolved a click or the wheel to.
///
/// `Focus` and `Band` move where the keys already point; `Screen` shows another screen the way
/// `e`, `t`, `d` or `Esc` would; `Key` presses a key the way the keyboard does; `Intent` is
/// carried out through [`take_intent`] like any other, noting the row it writes to as a key
/// press does. `BandEdit` is finished the same way a band key's writes are, so the wheel over a
/// band in Flat mode switches the tone control to EQ as the key would; `ToggleTone` and
/// `StepMode` likewise finish through `Intent`. `Setting` moves the Settings box to the category
/// and row clicked. `Transport` focuses the Transport row on the
/// button clicked and presses it, as `Enter` there would. `EditBand` opens the edit line on the
/// value double-clicked. `BarPress`, `BarTo` and `BarRelease` set a level bar from the cell under
/// the pointer, hold it while the button is down and write where it is let go
/// ([`glide`]). `Grab` holds a band's point on the tone curve, focusing the band;
/// `DragTo` carries out the drag, finished as a band key's writes are; `Release` lets go of it,
/// writing where it let go.
pub(super) async fn apply_action<O: Open, M: MouseCapture>(
    action: MouseAction,
    opener: &Opener<O>,
    mut watching: Watching,
    mouse: &mut M,
    look: &mut Look,
    screen: &mut dyn Shown,
) -> Flow {
    match action {
        MouseAction::Mascot => {
            let now = watching.app.now;
            watching.app.mascot.click(now);
            Flow::Continue(watching)
        }
        MouseAction::Focus(pane, row) => {
            watching.app.focus(pane, row);
            Flow::Continue(watching)
        }
        MouseAction::Band(band) => {
            watching.app.focus_band(band);
            Flow::Continue(watching)
        }
        MouseAction::Screen(screen) => {
            watching.app.show(screen);
            Flow::Continue(watching)
        }
        MouseAction::Key(code) => {
            take_key(KeyEvent::from(code), opener, watching, mouse, look, screen).await
        }
        MouseAction::Intent(intent) => {
            // A click that writes a value is the user changing it, as the key would, so the
            // row it writes to earns the same sweep.
            watching.app.touched(std::slice::from_ref(&intent));
            quit_if_asked(take_intent(intent, opener, watching, mouse, look, screen).await)
        }
        MouseAction::BandEdit(writes) => {
            let intents = watching.app.band_edit(writes);
            watching.app.touched(&intents);
            take_intents(intents, opener, watching, mouse, look, screen).await
        }
        MouseAction::ToggleTone(row) => {
            let intents = watching.app.click_tone_row(row);
            watching.app.touched(&intents);
            take_intents(intents, opener, watching, mouse, look, screen).await
        }
        MouseAction::PointField { point, field } => {
            let intents = watching.app.click_point_field(point, field);
            watching.app.touched(&intents);
            take_intents(intents, opener, watching, mouse, look, screen).await
        }
        MouseAction::Crossfeed { row, step } => {
            let intents = watching.app.click_crossfeed(row, step);
            watching.app.touched(&intents);
            take_intents(intents, opener, watching, mouse, look, screen).await
        }
        MouseAction::CrossfeedOn => {
            let intents = watching.app.xf_toggle(None);
            watching.app.touched(&intents);
            take_intents(intents, opener, watching, mouse, look, screen).await
        }
        MouseAction::CrossfeedModel(dir) => {
            let intents = watching.app.xf_model_step(dir);
            watching.app.touched(&intents);
            take_intents(intents, opener, watching, mouse, look, screen).await
        }
        MouseAction::Target(side) => {
            let intents = watching.app.click_target(side);
            take_intents(intents, opener, watching, mouse, look, screen).await
        }
        MouseAction::StepMode(dir) => {
            let intents = watching.app.step_tone_mode(dir);
            watching.app.touched(&intents);
            take_intents(intents, opener, watching, mouse, look, screen).await
        }
        MouseAction::Transport(button) => {
            let intents = watching.app.click_transport(button);
            take_intents(intents, opener, watching, mouse, look, screen).await
        }
        MouseAction::PresetPick(row) => {
            focus_list(&mut watching.app, row);
            take_key(
                KeyEvent::from(crossterm::event::KeyCode::Enter),
                opener,
                watching,
                mouse,
                look,
                screen,
            )
            .await
        }
        MouseAction::PresetGo(row) => {
            focus_list(&mut watching.app, row);
            Flow::Continue(watching)
        }
        MouseAction::Setting(at) => {
            watching.app.overlays.settings = Some(at);
            Flow::Continue(watching)
        }
        MouseAction::DismissToast => {
            watching.app.dismiss_toast();
            Flow::Continue(watching)
        }
        MouseAction::EditBand { band, field } => {
            edit_line::open_at(&mut watching.app, band, field);
            Flow::Continue(watching)
        }
        MouseAction::Grab { band, gesture, at } => {
            curve_drag::start(&mut watching.app, band, gesture, at);
            Flow::Continue(watching)
        }
        MouseAction::DragTo { at, curve } => {
            let intents = curve_drag::to(&mut watching.app, curve, at);
            watching.app.touched(&intents);
            take_intents(intents, opener, watching, mouse, look, screen).await
        }
        MouseAction::BarPress { bar, at, area } => {
            let intents = glide::press(&mut watching.app, bar, area, at);
            watching.app.touched(&intents);
            take_intents(intents, opener, watching, mouse, look, screen).await
        }
        MouseAction::BarTo { at, area } => {
            let intents = glide::to(&mut watching.app, area, at);
            watching.app.touched(&intents);
            take_intents(intents, opener, watching, mouse, look, screen).await
        }
        MouseAction::BarRelease { at, area } => {
            let intents = glide::release(&mut watching.app, area.map(|area| (area, at)));
            watching.app.touched(&intents);
            take_intents(intents, opener, watching, mouse, look, screen).await
        }
        MouseAction::DividerPress { at } => {
            divider::press(&mut watching.app, at);
            Flow::Continue(watching)
        }
        MouseAction::DividerTo { at } => {
            divider::to(&mut watching.app, at);
            Flow::Continue(watching)
        }
        MouseAction::DividerRelease => {
            divider::release(&mut watching.app);
            Flow::Continue(watching)
        }
        MouseAction::DividerReset => {
            divider::reset(&mut watching.app, Instant::now());
            Flow::Continue(watching)
        }
        MouseAction::Release { at, curve } => {
            let intents = curve_drag::release(&mut watching.app, curve.map(|curve| (curve, at)));
            watching.app.touched(&intents);
            take_intents(intents, opener, watching, mouse, look, screen).await
        }
    }
}
