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
async function clickAt(x, y) {
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
    pointer.notify_button(t(), Clutter.BUTTON_PRIMARY, Clutter.ButtonState.PRESSED);
    await Scripting.sleep(100);
    pointer.notify_button(t(), Clutter.BUTTON_PRIMARY, Clutter.ButtonState.RELEASED);
}

const centre = actor => {
    const [x, y] = actor.get_transformed_position();
    const [w, h] = actor.get_transformed_size();
    return [x + w / 2, y + h / 2, w, h, x, y];
};

const eqCalls = () => {
    const log = readText(env('SVOLDER_FAKE_LOG'));
    return {
        on: log.split('host\teq\ton\t').length - 1,
        off: log.split('host\teq\toff\t').length - 1,
    };
};

// The Computer EQ row: a click on the switch's track, on its knob and Space each toggle it once
// and leave the presets shut; a click on the row's name opens the presets.
async function eqChecks(indicator) {
    const row = indicator._eqRow;
    const expect = async (on, off, what) => {
        await until(() => eqCalls().on === on && eqCalls().off === off, what, 3000);
        record(eqCalls().on === on && eqCalls().off === off, what, JSON.stringify(eqCalls()));
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
    await expect(1, 0, 'a click on the empty side of the switch runs host eq on');
    const [kx, ky] = centre(row.switch._handle);
    await clickAt(kx, ky);
    await expect(1, 1, 'a click on the knob runs host eq off');
    row.grab_key_focus();
    const keyboard = global.stage.context.get_backend().get_default_seat()
        .create_virtual_device(Clutter.InputDeviceType.KEYBOARD_DEVICE);
    keyboard.notify_keyval(GLib.get_monotonic_time(), Clutter.KEY_space, Clutter.KeyState.PRESSED);
    keyboard.notify_keyval(GLib.get_monotonic_time(), Clutter.KEY_space, Clutter.KeyState.RELEASED);
    await expect(2, 1, 'Space on the row runs host eq on');
    const name = findLabel(row, 'Computer EQ');
    const [nx, ny] = centre(name);
    await clickAt(nx, ny);
    await Scripting.sleep(500);
    record(row.menu.isOpen, 'a click on the row name opens the presets');
    row.menu.close();
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
        await eqChecks(indicator);
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
