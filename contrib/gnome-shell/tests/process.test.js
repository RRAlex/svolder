// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

import {equal, fixture, ok, run, test} from './harness.js';
import {
    Backoff, BACKOFF_SECONDS, clickFailure, clickArgv, cliArgv, ControlQueue, dbArg, eqSwitchArgs, muteArgs,
    PANEL_ARGS, ScrollAccumulator, scrollArgs, stepArgs, streamFailure, volumeSetArgs,
} from '../svolder-indicator@rralex.github.io/process.js';

const [{actions}] = fixture('untouched');

test('every call goes through the daemon only, a click never starts one, none carries --yes', () => {
    equal(cliArgv(PANEL_ARGS), ['svolder', '--transport', 'daemon', 'panel', '--json']);
    for (const args of Object.values(actions)) {
        const argv = clickArgv(args);
        equal(argv.slice(0, 4), ['svolder', '--no-start', '--transport', 'daemon']);
        ok(!argv.includes('--yes'), 'no --yes');
    }
});

test('--step N only for more than one notch', () => {
    equal(stepArgs(actions.volume_up, 1), ['set', 'volume', 'up']);
    equal(stepArgs(actions.volume_up, 3), ['set', 'volume', 'up', '--step', '3']);
    equal(scrollArgs(actions, 2), ['set', 'volume', 'up', '--step', '2']);
    equal(scrollArgs(actions, -4), ['set', 'volume', 'down', '--step', '4']);
    equal(scrollArgs(actions, -1), ['set', 'volume', 'down']);
    equal(scrollArgs(actions, 0), null);
});

test('the slider writes {db} with one decimal and a plain minus sign', () => {
    equal(volumeSetArgs(actions, -24.1), ['set', 'volume', '-24.1dB']);
    equal(volumeSetArgs(actions, -24.14), ['set', 'volume', '-24.1dB']);
    equal(volumeSetArgs(actions, 0), ['set', 'volume', '0.0dB']);
    equal(dbArg(-0.04), '0.0');
});

test('mute and the computer EQ switch toggle from the state shown', () => {
    equal(muteArgs(actions, false), ['set', 'mute', 'on']);
    equal(muteArgs(actions, true), ['set', 'mute', 'off']);
    equal(eqSwitchArgs(actions, true), ['host', 'eq', 'off']);
    equal(eqSwitchArgs(actions, false), ['host', 'eq', 'on']);
});

test('backoff: 1, 2, 4, 8, 16, 30 and stays at 30', () => {
    const b = new Backoff();
    equal([...Array(8)].map(() => b.next(100)), [1000, 2000, 4000, 8000, 16000, 30000, 30000, 30000]);
    equal(BACKOFF_SECONDS, [1, 2, 4, 8, 16, 30]);
});

test('backoff restarts after a run that stayed up 60 s, not after 59', () => {
    const b = new Backoff();
    b.next(0);
    b.next(0);
    b.next(0);
    equal(b.next(59999), 8000);
    equal(b.next(60000), 1000);
    equal(b.next(0), 2000);
});

test('scroll notches add up, opposite ones cancel, take() empties', () => {
    const s = new ScrollAccumulator();
    s.addNotch(1);
    s.addNotch(1);
    s.addNotch(1);
    equal(s.take(), 3);
    equal(s.take(), 0);
    s.addNotch(1);
    s.addNotch(-1);
    s.addNotch(-1);
    equal(s.take(), -1);
});

test('smooth scrolling gives whole notches and keeps the rest', () => {
    const s = new ScrollAccumulator();
    s.addSmooth(0.4);
    equal(s.pending, 0);
    s.addSmooth(0.4);
    equal(s.pending, 0);
    s.addSmooth(0.4);
    equal(s.take(), 1);
    s.addSmooth(-2.5);
    equal(s.take(), -2);
    s.addSmooth(-0.5);
    equal(s.take(), 0);
    s.addSmooth(-0.5);
    equal(s.take(), -1);
});

test('one call at a time per control; the last request waiting wins', async () => {
    const q = new ControlQueue();
    const log = [];
    let release;
    const gate = new Promise(r => {
        release = r;
    });
    q.submit('volume', async () => {
        log.push('first');
        await gate;
    });
    q.submit('volume', async () => log.push('second'));
    q.submit('volume', async () => log.push('third'));
    q.submit('mute', async () => log.push('mute'));
    await Promise.resolve();
    await Promise.resolve();
    equal(log, ['first', 'mute']);
    equal(q.busy('volume'), true);
    release();
    await gate;
    for (let i = 0; i < 20; i++)
        await Promise.resolve();
    equal(log, ['first', 'mute', 'third']);
    equal(q.busy('volume'), false);
});

test('a scroll waiting never replaces the slider write waiting beside it', async () => {
    const q = new ControlQueue();
    const log = [];
    let release;
    const gate = new Promise(r => {
        release = r;
    });
    q.submit('volume', async () => await gate, 'step');
    q.submit('volume', async () => log.push('set -20'), 'set');
    q.submit('volume', async () => log.push('step 1'), 'step');
    q.submit('volume', async () => log.push('step 2'), 'step');
    q.submit('volume', async () => log.push('set -30'), 'set');
    release();
    for (let i = 0; i < 30; i++)
        await Promise.resolve();
    equal(log, ['step 2', 'set -30']);
});

test('a throwing job does not stop the queue', async () => {
    const q = new ControlQueue();
    const log = [];
    q.submit('a', async () => {
        throw new Error('x');
    });
    q.submit('a', async () => log.push('next'));
    for (let i = 0; i < 8; i++)
        await Promise.resolve();
    equal(log, ['next']);
});

test('stream failure lines by exit code', () => {
    equal(streamFailure(null, []), {headline: 'svolder is not on PATH', detail: ''});
    equal(streamFailure(3, ['svolder: no daemon is running. Start one with `svolder daemon start`']), {
        headline: 'the svolder daemon is not running',
        detail: 'no daemon is running. Start one with `svolder daemon start`',
    });
    equal(streamFailure(2, ['svolder: 2 devices in range: A; B. Pick one with `-d NAME`', '']).headline,
        '2 devices in range: A; B. Pick one with `-d NAME`');
    equal(streamFailure(7, []).headline, 'the svolder daemon is too old for this indicator');
    equal(streamFailure(1, ['svolder: boom']).headline, 'svolder panel stopped (exit 1)');
});

test('a click that did not succeed shows one line; only exit 1 naming --yes asks to open svolder', () => {
    equal(clickFailure(0, []), null);
    equal(clickFailure(1, ['svolder: add --yes to go on']), 'needs a confirmation: open svolder');
    equal(clickFailure(1, ['svolder: some context', 'pass --yes to say yes']), 'needs a confirmation: open svolder');
    equal(clickFailure(1, ['svolder: bad control']), 'bad control');
    equal(clickFailure(4, []), 'the DAC did not confirm in time');
    equal(clickFailure(4, ['svolder: the DAC did not answer. Try again']), 'the DAC did not answer. Try again');
    equal(clickFailure(3, ['svolder: no daemon. Start one']), 'no daemon. Start one');
    equal(clickFailure(5, []), 'svolder exited 5');
    equal(clickFailure(null, []), 'svolder is not on PATH');
});

run('process');
