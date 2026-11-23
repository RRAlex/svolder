// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

// The Gio side of the command line: the long-lived `svolder panel --json` stream with its restart
// schedule, and the one-shot calls a click makes. Every child, read, source and cancellable it
// makes is dropped by stop(), which is synchronous.

import Gio from 'gi://Gio';
import GLib from 'gi://GLib';

import {Backoff, clickArgv, cliArgv, PANEL_ARGS, streamFailure} from './process.js';

const MAX_STDERR_LINES = 8;

// Whether the first line of a stream says no device is known (`device: null`).
const firstLineHasNoDevice = line => {
    try {
        return JSON.parse(line).device === null;
    } catch (_e) {
        return false;
    }
};

const exitCodeOf = proc => {
    if (proc.get_if_exited())
        return proc.get_exit_status();
    return 128 + (proc.get_if_signaled() ? proc.get_term_sig() : 0);
};

export class Runner {
    // `handlers`: onLine(text), onFailure({headline, detail}).
    constructor(handlers) {
        this._handlers = handlers;
        this._backoff = new Backoff();
        this._generation = 0;
        this._stopped = true;
        this._cancellable = null;
        this._proc = null;
        this._restartId = 0;
        this._calls = new Set();
        this._failing = false;
        this._live = false;
        this._quickRestart = false;
    }

    // Whether the stream has sent a line and its child still runs. Calls run only then: the CLI's
    // own launcher chain could start a daemon when none answers.
    get live() {
        return this._live;
    }

    start() {
        this._stopped = false;
        this._cancellable = new Gio.Cancellable();
        this._spawn();
    }

    stop() {
        this._stopped = true;
        this._live = false;
        this._generation++;
        if (this._restartId) {
            GLib.Source.remove(this._restartId);
            this._restartId = 0;
        }
        this._cancellable?.cancel();
        this._cancellable = null;
        this._proc?.force_exit();
        this._proc = null;
        for (const call of this._calls)
            call.force_exit();
        this._calls.clear();
    }

    _spawn() {
        const generation = ++this._generation;
        let proc;
        try {
            proc = new Gio.Subprocess({
                argv: cliArgv(PANEL_ARGS),
                flags: Gio.SubprocessFlags.STDOUT_PIPE | Gio.SubprocessFlags.STDERR_PIPE,
            });
            proc.init(this._cancellable);
        } catch (_e) {
            this._ended(generation, null, [], GLib.get_monotonic_time());
            return;
        }
        this._proc = proc;
        const startedAt = GLib.get_monotonic_time();
        const stderrLines = [];
        let waiting = 3;
        let code = 0;
        let live = false;
        let beganWithoutDevice = false;
        const done = () => {
            if (--waiting === 0)
                this._ended(generation, code, stderrLines, startedAt, beganWithoutDevice);
        };

        proc.wait_async(this._cancellable, (p, res) => {
            if (generation !== this._generation)
                return;
            this._live = false;
            try {
                p.wait_finish(res);
                code = exitCodeOf(p);
            } catch (_e) {
                code = 1;
            }
            done();
        });

        const stdout = new Gio.DataInputStream({base_stream: proc.get_stdout_pipe(), close_base_stream: true});
        const stderr = new Gio.DataInputStream({base_stream: proc.get_stderr_pipe(), close_base_stream: true});
        this._read(generation, stdout, line => {
            if (!live) {
                live = true;
                this._live = true;
                beganWithoutDevice = firstLineHasNoDevice(line);
                if (this._failing) {
                    this._failing = false;
                    console.log('svolder: the panel stream is back');
                }
            }
            this._handlers.onLine(line);
        }, done);
        this._read(generation, stderr, line => {
            if (stderrLines.length < MAX_STDERR_LINES)
                stderrLines.push(line);
        }, done);
    }

    _read(generation, stream, onLine, onEnd) {
        stream.read_line_async(GLib.PRIORITY_DEFAULT, this._cancellable, (s, res) => {
            if (generation !== this._generation)
                return;
            let line = null;
            try {
                [line] = s.read_line_finish_utf8(res);
            } catch (_e) {
                line = null;
            }
            if (line === null) {
                s.close_async(GLib.PRIORITY_DEFAULT, null, null);
                onEnd();
                return;
            }
            onLine(line);
            this._read(generation, stream, onLine, onEnd);
        });
    }

    _ended(generation, code, stderrLines, startedAt, beganWithoutDevice) {
        if (generation !== this._generation || this._stopped)
            return;
        this._proc = null;
        this._live = false;
        const ranMs = (GLib.get_monotonic_time() - startedAt) / 1000;
        let delay;
        if (code === 0) {
            // A stream that began with no device ends with exit 0 once one appears: start it again
            // at once (once in a row; a second quick end waits as any other). Any other clean exit
            // starts again after the usual pause. Neither is a failure.
            const at_once = beganWithoutDevice && !(this._quickRestart && ranMs < 1000);
            this._quickRestart = at_once;
            delay = at_once ? 0 : this._backoff.next(ranMs);
        } else {
            this._quickRestart = false;
            delay = this._backoff.next(ranMs);
            this._handlers.onFailure(streamFailure(code, stderrLines));
        }
        if (code !== 0 && !this._failing) {
            this._failing = true;
            console.warn(`svolder: the panel stream ended (${code === null ? 'not started' : `exit ${code}`}); trying again with a growing pause`);
        }
        this._restartId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, delay, () => {
            this._restartId = 0;
            if (!this._stopped)
                this._spawn();
            return GLib.SOURCE_REMOVE;
        });
    }

    // One call: resolves {code, stderrLines}; code null when the program did not start.
    run(args) {
        return new Promise(resolve => {
            if (!this._live) {
                resolve({code: null, stderrLines: [], skipped: true});
                return;
            }
            let proc;
            try {
                proc = new Gio.Subprocess({
                    argv: clickArgv(args),
                    flags: Gio.SubprocessFlags.STDOUT_SILENCE | Gio.SubprocessFlags.STDERR_PIPE,
                });
                proc.init(this._cancellable);
            } catch (_e) {
                resolve({code: null, stderrLines: []});
                return;
            }
            this._calls.add(proc);
            proc.communicate_utf8_async(null, this._cancellable, (p, res) => {
                this._calls.delete(p);
                if (this._stopped) {
                    resolve({code: null, stderrLines: []});
                    return;
                }
                try {
                    const [, , stderr] = p.communicate_utf8_finish(res);
                    resolve({code: exitCodeOf(p), stderrLines: (stderr ?? '').split('\n')});
                } catch (_e) {
                    resolve({code: null, stderrLines: []});
                }
            });
        });
    }
}
