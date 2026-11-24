// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

// An automation script for `gnome-shell-test-tool --headless --extension <zip>`: the real shell
// loads the indicator and this script looks at what it drew. Settings come from the environment
// (scripts/gnome-extension.sh sets them):
//   SVOLDER_HEADLESS_MODE     `ok` (a stream that stays up) or `exit3` (a stream that fails)
//   SVOLDER_HEADLESS_RESULT   file that gets one line per check
//   SVOLDER_FAKE_FIXTURE, SVOLDER_FAKE_LOG, SVOLDER_FAKE_PIDFILE   the fake svolder's files
//   SVOLDER_HEADLESS_SHOTS    when set, a path prefix: <prefix>-bar.png and <prefix>-menu.png

import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Shell from 'gi://Shell';
import St from 'gi://St';

import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as Scripting from 'resource:///org/gnome/shell/ui/scripting.js';

const UUID = 'svolder-indicator@rralex.github.io';
const env = name => GLib.getenv(name) ?? '';

const failures = [];

function record(ok, what, detail = '') {
    const line = `${ok ? 'ok  ' : 'FAIL'} ${what}${ok || !detail ? '' : ` (${detail})`}`;
    const file = Gio.File.new_for_path(env('SVOLDER_HEADLESS_RESULT'));
    const stream = file.append_to(Gio.FileCreateFlags.NONE, null);
    stream.write_all(`${line}\n`, null);
    stream.close(null);
    if (!ok)
        failures.push(what);
}

async function until(cond, what, ms = 15000) {
    for (let waited = 0; waited < ms; waited += 100) {
        if (cond())
            return true;
        await Scripting.sleep(100);
    }
    record(false, what, `not after ${ms} ms`);
    return false;
}

function readText(path) {
    try {
        const [, bytes] = Gio.File.new_for_path(path).load_contents(null);
        return new TextDecoder().decode(bytes);
    } catch (_e) {
        return '';
    }
}

// Every St.Label text under `actor` that is visible, in tree order.
function labels(actor, out = []) {
    if (!actor.visible)
        return out;
    if (actor instanceof St.Label)
        out.push(actor.clutter_text.text);
    for (const child of actor.get_children())
        labels(child, out);
    return out;
}

function findLabel(actor, text) {
    if (actor instanceof St.Label && actor.clutter_text.text === text)
        return actor;
    for (const child of actor.get_children()) {
        const found = findLabel(child, text);
        if (found)
            return found;
    }
    return null;
}

const logCount = word => readText(env('SVOLDER_FAKE_LOG')).split('\n')
    .filter(l => l.split('\t').includes(word)).length;

const pidGone = pid => {
    const stat = readText(`/proc/${pid}/stat`);
    return stat === '' || stat.split(') ')[1]?.startsWith('Z');
};

async function shoot(path, x, y, w, h) {
    const stream = Gio.File.new_for_path(path).replace(null, false, Gio.FileCreateFlags.NONE, null);
    const shot = new Shell.Screenshot();
    await new Promise((resolve, reject) => {
        shot.screenshot_area(x, y, w, h, stream, (s, res) => {
            try {
                resolve(s.screenshot_area_finish(res));
            } catch (e) {
                reject(e);
            }
        });
    });
    stream.close(null);
}

// The pointer to an empty corner and no key focus, so a shot shows no hover or focus.
function park() {
    if (pointer === null)
        pointer = global.stage.context.get_backend().get_default_seat()
            .create_virtual_device(Clutter.InputDeviceType.POINTER_DEVICE);
    pointer.notify_absolute_motion(GLib.get_monotonic_time(), 5, 715);
    global.stage.set_key_focus(null);
}

async function shots(indicator) {
    const prefix = env('SVOLDER_HEADLESS_SHOTS');
    if (prefix === '')
        return;
    const monitor = Main.layoutManager.primaryMonitor;
    const width = 440;
    try {
        park();
        await Scripting.sleep(300);
        await shoot(`${prefix}-bar.png`, monitor.x + monitor.width - width, 0, width, Main.panel.height);
        indicator.menu.open();
        park();
        await Scripting.sleep(800);
        await shoot(`${prefix}-menu.png`, monitor.x + monitor.width - width, 0, width, 560);
        if (indicator._pathItem.visible) {
            const [hx, hy] = centre(indicator._bins[2]);
            await movePointer(hx, hy);
            await Scripting.sleep(400);
            await shoot(`${prefix}-hover.png`, monitor.x + monitor.width - width, 0, width, 560);
            park();
        }
        if (indicator._scenesRow.visible && indicator._scenesRow.menu.numMenuItems > 1) {
            indicator._scenesRow.menu.open();
            park();
            await Scripting.sleep(600);
            await shoot(`${prefix}-scenes.png`, monitor.x + monitor.width - width, 0, width, 560);
        }
        indicator.menu.close();
    } catch (e) {
        record(false, 'screenshots', e.message);
    }
}

let pointer = null;

// Motion, press and release are separate events the shell handles in turn. On GNOME 51 a motion
// sent while a menu holds the grab can be dropped, so the motion is repeated until the shell
// reports the pointer where it was sent, and only then pressed.
async function movePointer(x, y) {
    if (pointer === null)
        pointer = global.stage.context.get_backend().get_default_seat()
            .create_virtual_device(Clutter.InputDeviceType.POINTER_DEVICE);
    const t = () => GLib.get_monotonic_time();
    pointer.notify_absolute_motion(t(), x, y);
    await Scripting.sleep(100);
    for (let tries = 0; tries < 20; tries++) {
        const [px, py] = global.get_pointer();
        if (Math.abs(px - x) < 2 && Math.abs(py - y) < 2)
            break;
        pointer.notify_absolute_motion(t(), x, y);
        await Scripting.sleep(100);
    }
    // A second motion on the spot: the shell settles the pointer's crossing events with it.
    pointer.notify_absolute_motion(t(), x + 1, y);
    await Scripting.sleep(100);
    pointer.notify_absolute_motion(t(), x, y);
    global.stage.queue_redraw();
    await Scripting.sleep(300);
}

async function clickAt(x, y) {
    await movePointer(x, y);
    const t = () => GLib.get_monotonic_time();
    pointer.notify_button(t(), Clutter.BUTTON_PRIMARY, Clutter.ButtonState.PRESSED);
    await Scripting.sleep(100);
    pointer.notify_button(t(), Clutter.BUTTON_PRIMARY, Clutter.ButtonState.RELEASED);
}

const centre = actor => {
    const [x, y] = actor.get_transformed_position();
    const [w, h] = actor.get_transformed_size();
    return [x + w / 2, y + h / 2, w, h, x, y];
};

// How many times the CLI was called with `args` (tab separated, as the fake logs them).
const callsOf = args => readText(env('SVOLDER_FAKE_LOG')).split(`${args.join('\t')}\t\n`).length - 1;

// The Computer EQ row: a click on the switch's track, on its knob and Space each toggle it once
// and leave the presets shut; a click on the row name opens the presets. The calls are the ones
// the line's `switch` gives (the bypass, or `host eq on`); a line with no switch is an older
// svolder's, and the row falls back to `host eq on|off` from the state it shows.
async function eqChecks(indicator, fixtureLine) {
    const row = indicator._eqRow;
    const flip = fixtureLine.computer_eq.switch?.args ?? null;
    const flipText = flip ? flip.join(' ') : 'host eq on|off';
    record(row.toggleButton.accessible_name === (flip ? fixtureLine.computer_eq.switch.name : 'Computer EQ on or off'),
        'the switch is named for what it does', row.toggleButton.accessible_name);
    const expect = async (n, what) => {
        // [flips so far] for a line with a switch; [on, off] for an older one.
        const calls = () => flip ? [callsOf(flip)] : [callsOf(['host', 'eq', 'on']), callsOf(['host', 'eq', 'off'])];
        // An older line's switch flips the EQ: each toggle runs `off` while it shows on, else `on`.
        let shown = fixtureLine.computer_eq.on;
        const sent = {on: 0, off: 0};
        for (let i = 0; i < n; i++) {
            sent[shown ? 'off' : 'on']++;
            shown = !shown;
        }
        // A move back to the state the line shows runs nothing: the knob click after the first one.
        const want = flip ? [[1, 1, 2][n - 1]] : [sent.on, sent.off];
        await until(() => JSON.stringify(calls()) === JSON.stringify(want), what, 3000);
        record(JSON.stringify(calls()) === JSON.stringify(want), what, JSON.stringify(calls()));
        record(!row.menu.isOpen, `${what}: the presets stay shut`);
    };
    // The first virtual click after an open only closes the menu; make it and open again.
    await clickAt(5, 600);
    for (let waited = 0; indicator.menu.isOpen && waited < 2000; waited += 100)
        await Scripting.sleep(100);
    await Scripting.sleep(300);
    indicator.menu.open();
    await until(() => indicator.menu.isOpen && row.switch.mapped, 'the menu is open again', 3000);
    await Scripting.sleep(500);

    const [, cy, w, , x] = centre(row.switch);
    await clickAt(x + w - 4, cy);
    await expect(1, `a click on the empty side of the switch runs ${flipText}`);
    const [kx, ky] = centre(row.switch._handle);
    await clickAt(kx, ky);
    await expect(2, flip ? 'a click back to the state shown runs nothing' : `a click on the knob runs ${flipText}`);
    row.grab_key_focus();
    const keyboard = global.stage.context.get_backend().get_default_seat()
        .create_virtual_device(Clutter.InputDeviceType.KEYBOARD_DEVICE);
    keyboard.notify_keyval(GLib.get_monotonic_time(), Clutter.KEY_space, Clutter.KeyState.PRESSED);
    keyboard.notify_keyval(GLib.get_monotonic_time(), Clutter.KEY_space, Clutter.KeyState.RELEASED);
    await expect(3, `Space on the row runs ${flipText}`);
    const name = findLabel(row, 'Computer EQ');
    const [nx, ny] = centre(name);
    await clickAt(nx, ny);
    await Scripting.sleep(500);
    record(row.menu.isOpen, 'a click on the row name opens the presets');
    row.menu.close();
}

// The strip's line: the rest line at rest, a segment's words under the pointer, the rest line
// again when it leaves and when the menu closes.
async function pathChecks(indicator, fixtureLine) {
    const path = fixtureLine.path;
    if (!path || fixtureLine.device?.present !== true) {
        record(!indicator._pathItem.visible, 'no Path strip without a path or with the DAC away');
        return;
    }
    const line = indicator._pathOff;
    // What the label says: nothing when it is drawn clear.
    const said = () => line.opacity === 0 ? '' : line.clutter_text.text.replace(/\u200b/g, '').replace(/\u2060/g, '').replace(/\u00a0/g, ' ');
    // The first virtual pointer event after an open closes the menu: make it and open again.
    if (pointer === null) {
        await movePointer(5, 600);
        await Scripting.sleep(500);
        indicator.menu.open();
        await until(() => indicator.menu.isOpen && indicator._strip.mapped, 'the menu is open for the strip', 3000);
        await Scripting.sleep(500);
    }
    // The unit of a dB figure is drawn as a small span, so the label's text has no space before it.
    const rest = path.rest.text.replace(/(?<=\d) dB(?![A-Za-z])/g, 'dB');
    record(line.opacity === (rest === '' ? 0 : 255), 'the rest line is drawn only when it has words');
    record(rest === '' || said() === rest, `the rest line reads "${rest}"`, said());
    record(path.rest.text !== 'untouched end to end' || line.style.includes('#8ff0a4'),
        'the untouched line is green');
    const rowHeight = indicator._pathItem.height;
    // The menu's width is the same whichever segment the pointer is on: the line is sized for the
    // longest words it can say.
    // The line starts beside the strip, at rest and on hover (never centred in the row).
    record(line.x === 0 && indicator._pathLine.x >= indicator._strip.x + indicator._strip.width - 1,
        'at rest the line starts right after the strip', `x=${line.x} twin at ${indicator._pathLine.x}, strip ends ${indicator._strip.x + indicator._strip.width}`);
    const menuWidth = indicator.menu.box.width;
    const rowWidth = indicator._pathItem.width;
    // The label is as tall as its own text: no floor above it that the font does not follow.
    const [, textHeight] = line.clutter_text.get_layout().get_pixel_size();
    record(Math.abs(line.height - textHeight) <= 1, "the line is as tall as its text, whatever the text scale",
        `${line.height} vs ${textHeight}`);
    for (const [i, bin] of indicator._bins.entries()) {
        const [bx, by, bw, bh] = centre(bin);
        const [, , , sh, sx] = centre(indicator._segments[i]);
        record(bh >= sh && bin.accessible_name === path.segments[i].words, `segment ${i} has its words as its name`);
        await movePointer(bx, by);
        await Scripting.sleep(300);
        record(said() === path.segments[i].words, `hovering segment ${i} shows "${path.segments[i].words}"`, said());
        // The line's accessible name is the plain words: no wrap marks (no-break spaces, joiners).
        record(line.accessible_name === path.segments[i].words, `hovering segment ${i} names the line plainly`,
            JSON.stringify(line.accessible_name));
        record(indicator._pathItem.height === rowHeight, `hovering segment ${i} leaves the row's height alone`,
            `${rowHeight} -> ${indicator._pathItem.height}`);
        // The words are shown whole: wrapped at their separators, never cut by an ellipsis.
        record(!line.clutter_text.get_layout().is_ellipsized(), `hovering segment ${i} shows its words whole`,
            said());
        record(line.x === 0, `hovering segment ${i} keeps the line beside the strip`, `x=${line.x}`);
        record(indicator.menu.box.width === menuWidth && indicator._pathItem.width === rowWidth,
            `hovering segment ${i} leaves the menu's width alone`,
            `${menuWidth}/${rowWidth} -> ${indicator.menu.box.width}/${indicator._pathItem.width}`);
        // The top of the bin, above the drawn segment, is still the segment.
        const [, , , , , ty] = centre(bin);
        await movePointer(bx, ty + 1);
        await Scripting.sleep(200);
        record(said() === path.segments[i].words, `the bin above segment ${i} still shows its words`, said());
        // The drawn gap to the next segment belongs to a bin: never the rest line.
        if (i < 2) {
            for (const past of [0.5, 1.5]) {
                await movePointer(sx + indicator._segments[i].width + past, by);
                await Scripting.sleep(200);
                record([path.segments[i].words, path.segments[i + 1].words].includes(said()),
                    `${past} px past segment ${i} shows a segment's words, not the rest line`, said());
            }
        }
    }
    await movePointer(5, 715);
    await Scripting.sleep(300);
    record(said() === rest, 'leaving the strip returns to the rest line', said());
    record(line.opacity === (rest === '' ? 0 : 255), 'leaving the strip hides the line when there is nothing to say');
    const [hx, hy] = centre(indicator._bins[2]);
    await movePointer(hx, hy);
    await Scripting.sleep(300);
    indicator.menu.close();
    await Scripting.sleep(300);
    record(indicator._hovered === null, 'closing the menu clears the hover');
    record(said() === rest, 'closing the menu returns to the rest line', said());
    record(line.opacity === (rest === '' ? 0 : 255), 'closing the menu hides the line when there is nothing to say');
    await movePointer(5, 715);
    indicator.menu.open();
    await Scripting.sleep(500);
}

async function runOk(indicator, fixtureLine) {
    const device = fixtureLine.device;
    const muted = device?.volume?.muted === true;
    const shown = device?.present === true && device.volume?.bar_text != null;
    record(indicator._figure.visible === (shown && !muted), 'the bar shows the figure only when the fixture has one');
    record(indicator._mutedGlyph.actor.visible === (shown && muted), 'the muted glyph shows only when muted');
    if (!device || !device.present) {
        record(indicator._boxGlyph._state.led === 'ring' && indicator._boxGlyph._state.outline === null,
            'an away DAC draws the hollow ring and the plain outline');
        record(indicator._signalGlyph._state.filled === 0, 'an away DAC fills no signal bar');
        record(!indicator._volumeItem.visible && !indicator._inputRow.visible, 'an away DAC hides its volume and input rows');
    }
    if (shown && !muted)
        record(indicator._figure.clutter_text.text === `${device.volume.bar_text}dB`, 'the bar shows the fixture figure', indicator._figure.clutter_text.text);

    indicator.menu.open();
    await Scripting.sleep(500);
    const box = indicator.menu.box;
    const seen = labels(box);
    const want = [];
    if (device) {
        want.push(device.name);
        if (device.present && device.input)
            want.push('Input');
    }
    want.push('Scenes', 'Open svolder');
    for (const text of want)
        record(seen.includes(text), `the menu shows "${text}"`, seen.join(' | '));

    await pathChecks(indicator, fixtureLine);

    const scene = fixtureLine.scenes.choices[0];
    if (scene) {
        indicator._scenesRow.menu.open();
        await Scripting.sleep(300);
        const item = findLabel(box, scene.name)?.get_parent();
        record(item !== null && item !== undefined, `the Scenes submenu lists "${scene.name}"`);
        const before = logCount('load');
        item?.activate(Clutter.get_current_event());
        const wanted = ['--no-start', '--transport', 'daemon', ...scene.run].join('\t');
        await until(() => readText(env('SVOLDER_FAKE_LOG')).includes(`${wanted}\t`), 'a scene click reaches the CLI');
        record(logCount('load') === before + 1, 'a scene click runs one call');
        record(!readText(env('SVOLDER_FAKE_LOG')).includes('--yes'), 'no call carries --yes');
        indicator._scenesRow.menu.close();
    }
    if (fixtureLine.computer_eq)
        await eqChecks(indicator, fixtureLine);
    indicator.menu.close();
    await shots(indicator);
}

// Puts the one monitor at `scale` (a temporary configuration, in this headless shell only).
async function setScale(scale) {
    const bus = Gio.DBus.session;
    // The service lives in this very process: a synchronous call would wait for itself.
    const call = (method, params, type) => new Promise((resolve, reject) => {
        bus.call('org.gnome.Mutter.DisplayConfig', '/org/gnome/Mutter/DisplayConfig',
            'org.gnome.Mutter.DisplayConfig', method, params, type, Gio.DBusCallFlags.NONE, 5000, null,
            (conn, res) => {
                try {
                    resolve(conn.call_finish(res));
                } catch (e) {
                    reject(e);
                }
            });
    });
    const state = (await call('GetCurrentState', null,
        new GLib.VariantType('(ua((ssss)a(siiddada{sv})a{sv})a(iiduba(ssss)a{sv})a{sv})'))).recursiveUnpack();
    const [serial, monitors] = state;
    const connector = monitors[0][0][0];
    const mode = monitors[0][1].find(m => m[6]['is-current'])?.[0] ?? monitors[0][1][0][0];
    await call('ApplyMonitorsConfig', new GLib.Variant('(uua(iiduba(ssa{sv}))a{sv})', [
        serial, 1, [[0, 0, scale, 0, true, [[connector, mode, {}]]]], {},
    ]), null);
    await Scripting.sleep(1500);
}

export async function run() {
    // SVOLDER_HEADLESS_TEXT_SCALE=N: the text-scaling-factor of this headless shell's own
    // GSettings (HOME and every XDG dir are scratch and the session bus is the test's own, so dconf writes only there).
    if (env('SVOLDER_HEADLESS_TEXT_SCALE') !== '') {
        const scale = Number(env('SVOLDER_HEADLESS_TEXT_SCALE'));
        const textScaleBefore = Clutter.Settings.get_default().font_dpi;
        new Gio.Settings({schema_id: 'org.gnome.desktop.interface'})
            .set_double('text-scaling-factor', Number(env('SVOLDER_HEADLESS_TEXT_SCALE')));
        await Scripting.sleep(1000);
        const after = Clutter.Settings.get_default().font_dpi;
        // The scale reached the shell: its font resolution changed with it.
        record(after !== textScaleBefore && (textScaleBefore <= 0 || Math.abs(after / textScaleBefore - scale) < 0.02),
            'the text scale took effect', `font_dpi ${textScaleBefore} -> ${after}`);
    }
    if (env('SVOLDER_HEADLESS_SCALE') !== '')
        await setScale(Number(env('SVOLDER_HEADLESS_SCALE')));
    // The shell draws the last line of the fixture: a fixture that tells a story ends in its last state.
    const lines = readText(env('SVOLDER_FAKE_FIXTURE')).split('\n').filter(l => l !== '');
    const fixture = JSON.parse(lines[lines.length - 1]);
    const mode = env('SVOLDER_HEADLESS_MODE') || 'ok';

    if (!await until(() => Main.panel.statusArea[UUID], 'the indicator is in the status area'))
        throw new Error('no indicator');
    record(true, 'the indicator is in the status area');
    const indicator = Main.panel.statusArea[UUID];

    if (mode === 'ok') {
        if (!await until(() => JSON.stringify(indicator._view) === JSON.stringify(fixture), 'the last panel line is drawn'))
            throw new Error('no first line');
        await runOk(indicator, fixture);
    } else if (mode === 'exit0') {
        // A stream that began with no device ends with exit 0: started again at once, quietly.
        if (!await until(() => logCount('panel') >= 2, 'a clean exit starts the stream again', 3000))
            throw new Error('no restart');
        const times = readText(`${env('SVOLDER_FAKE_LOG')}.times`).split('\n').filter(l => l !== '').map(Number);
        record((times[1] - times[0]) / 1e6 < 500, 'the restart is at once, not after the backoff', `${(times[1] - times[0]) / 1e6} ms`);
        record(indicator._failure === null, 'a clean exit shows no failure');
        indicator.menu.open();
        await Scripting.sleep(300);
        const seen = labels(indicator.menu.box).join(' | ');
        record(!seen.includes('svolder panel stopped') && !seen.includes('not running'), 'the popup has no failure line', seen);
        indicator.menu.close();
    } else {
        if (!await until(() => indicator._failure !== null, 'a failing stream shows its failure'))
            throw new Error('no failure');
        indicator.menu.open();
        await Scripting.sleep(300);
        const seen = labels(indicator.menu.box).join(' | ');
        record(seen.includes('the svolder daemon is not running'), 'the failure line is in the popup', seen);
        record(indicator._view === null, 'the bar is in the away state');
        indicator.menu.close();
        // While the stream is down nothing but the stream's own restart is spawned: a click that
        // gets through anyway (a queued one) must not reach the CLI.
        indicator._runAction('scene', ['scene', 'load', 'Desk']);
        indicator._runAction('mute', ['set', 'mute', 'on']);
        await Scripting.sleep(600);
        const log = readText(env('SVOLDER_FAKE_LOG'));
        record(!log.includes('scene\tload') && !log.includes('mute\ton'), 'no click reaches the CLI while the stream is down');
        await until(() => logCount('panel') >= 2, 'the stream is started again after the backoff', 6000);
        await shots(indicator);
    }

    // The sources the extension holds right before disable: the restart timer (a failing stream)
    // and the slider's write timer (a moved slider).
    const held = [];
    if (mode === 'ok') {
        indicator.menu.open();
        await Scripting.sleep(300);
        if (indicator._volumeItem.visible) {
            const v = indicator._slider.value;
            indicator._slider.value = v > 0.5 ? v - 0.2 : v + 0.2;
            held.push(['the slider timer', indicator._sliderTimer]);
        }
    } else {
        await until(() => indicator._runner._restartId !== 0, 'a restart is pending', 6000);
        held.push(['the restart timer', indicator._runner._restartId]);
    }
    const mainContext = GLib.MainContext.default();
    for (const [name, id] of held)
        record(id !== 0 && mainContext.find_source_by_id(id) !== null, `${name} is a live source before disable`);
    const started = logCount('panel');
    const pid = readText(env('SVOLDER_FAKE_PIDFILE')).trim();
    Main.extensionManager.disableExtension(UUID);
    await Scripting.sleep(300);
    record(Main.panel.statusArea[UUID] === undefined, 'disable removes the indicator');
    if (mode === 'ok' && pid !== '')
        if (await until(() => pidGone(pid), 'the fake CLI is gone after disable', 3000))
            record(true, 'the fake CLI is gone after disable');
    for (const [name, id] of held)
        record(mainContext.find_source_by_id(id) === null, `${name} is gone after disable`);
    await Scripting.sleep(2500);
    record(logCount('panel') === started, 'no stream is started again after disable', `${started} -> ${logCount('panel')}`);

    if (failures.length > 0)
        throw new Error(`failed: ${failures.join('; ')}`);
}
