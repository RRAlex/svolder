// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

// The indicator's command side without the shell: the argument vectors it runs, the restart
// schedule, the scroll accumulator, the one-call-at-a-time queue and the failure lines. The Gio
// part that spawns them lives in runner.js.

export const PROGRAM = 'svolder';
export const PANEL_ARGS = ['panel', '--json'];

// Every call goes to the daemon only: no autospawn, no direct BLE session, never `--yes`.
export function cliArgv(args) {
    return [PROGRAM, '--transport', 'daemon', ...args];
}

// A click's call: only a daemon that already answers is used, one is never started.
export function clickArgv(args) {
    return [PROGRAM, '--no-start', '--transport', 'daemon', ...args];
}

// `base` with `--step N` for N notches (one notch is the command's own step).
export function stepArgs(base, notches) {
    return notches > 1 ? [...base, '--step', String(notches)] : [...base];
}

// The dB as the CLI reads it: one decimal, a plain minus sign, never `-0.0`.
export function dbArg(db) {
    const text = (Math.round(db * 10) / 10).toFixed(1);
    return text === '-0.0' ? '0.0' : text;
}

export function volumeSetArgs(actions, db) {
    return actions.volume_set.map(a => a.replaceAll('{db}', dbArg(db)));
}

// The mute button toggles from the state the model shows.
export function muteArgs(actions, muted) {
    return muted ? actions.mute_off : actions.mute_on;
}

export function eqSwitchArgs(actions, on) {
    return on ? actions.eq_off : actions.eq_on;
}

export const BACKOFF_SECONDS = [1, 2, 4, 8, 16, 30];
export const HEALTHY_MS = 60000;

// The restart delays: 1, 2, 4, 8, 16, then 30 s; a run that stayed up 60 s starts the schedule
// over.
export class Backoff {
    constructor() {
        this._index = 0;
    }

    // The delay in ms before the next start, after a run that lasted `ranMs`.
    next(ranMs) {
        if (ranMs >= HEALTHY_MS)
            this._index = 0;
        const delay = BACKOFF_SECONDS[Math.min(this._index, BACKOFF_SECONDS.length - 1)] * 1000;
        this._index++;
        return delay;
    }
}

// Scroll events in, whole volume notches out. Notches that arrive while a call runs wait here
// and go out in the next call as `--step N`; opposite notches cancel.
export class ScrollAccumulator {
    constructor() {
        this._pending = 0;
        this._fraction = 0;
    }

    // One discrete notch: +1 up, -1 down.
    addNotch(direction) {
        this._pending += direction;
    }

    // A smooth scroll delta, positive for up; whole notches come out as the delta adds up.
    addSmooth(delta) {
        this._fraction += delta;
        const whole = Math.trunc(this._fraction);
        this._fraction -= whole;
        this._pending += whole;
    }

    // The net notches waiting, signed, and forgotten.
    take() {
        const n = this._pending;
        this._pending = 0;
        return n;
    }

    get pending() {
        return this._pending;
    }
}

// The arguments for `net` notches, or null when they cancel out.
export function scrollArgs(actions, net) {
    if (net === 0)
        return null;
    return net > 0 ? stepArgs(actions.volume_up, net) : stepArgs(actions.volume_down, -net);
}

// One call at a time per control; a request that comes while one runs replaces the one waiting,
// so the last word wins.
export class ControlQueue {
    constructor() {
        this._running = new Set();
        this._waiting = new Map();
    }

    // `job` runs now, or after the one running for `key`. A job waiting replaces an earlier one
    // of the same `kind` only, so a scroll never drops the slider's write that waits beside it.
    submit(key, job, kind = '') {
        if (this._running.has(key)) {
            const slot = `${key}\u0000${kind}`;
            this._waiting.delete(slot);
            this._waiting.set(slot, {key, job});
            return;
        }
        this._start(key, job);
    }

    _start(key, job) {
        this._running.add(key);
        Promise.resolve().then(job).catch(() => {}).finally(() => {
            this._running.delete(key);
            for (const [slot, next] of this._waiting) {
                if (next.key === key) {
                    this._waiting.delete(slot);
                    this._start(key, next.job);
                    break;
                }
            }
        });
    }

    busy(key) {
        return this._running.has(key);
    }

    // Forget every waiting request (disable()).
    clear() {
        this._waiting.clear();
    }
}

const firstLine = lines => {
    const line = (lines ?? []).map(l => l.trim()).find(l => l !== '') ?? '';
    return line.replace(/^svolder:\s*/, '');
};

// The popup's failure line for a stream that ended: the exit code, or a spawn failure when `code`
// is null.
export function streamFailure(code, stderrLines) {
    const detail = firstLine(stderrLines);
    switch (code) {
    case null:
        return {headline: 'svolder is not on PATH', detail: ''};
    case 3:
        return {headline: 'the svolder daemon is not running', detail};
    case 2:
        return {headline: detail || 'several devices: name one in the svolder config', detail: ''};
    case 7:
        return {headline: 'the svolder daemon is too old for this indicator', detail: 'svolder daemon restart'};
    default:
        return {headline: `svolder panel stopped (exit ${code})`, detail};
    }
}

// The one line a finished click shows when it did not succeed; null on success. A verb that
// needs an answer and has no terminal exits 1 with a line naming `--yes`; exit 4 is the device not
// confirming in time.
export function clickFailure(code, stderrLines) {
    if (code === 0)
        return null;
    if (code === null)
        return 'svolder is not on PATH';
    if (code === 1 && (stderrLines ?? []).some(l => l.includes('--yes')))
        return 'needs a confirmation: open svolder';
    const first = firstLine(stderrLines);
    if (code === 4)
        return first || 'the DAC did not confirm in time';
    return first || `svolder exited ${code}`;
}
