// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

// A tiny test runner for `gjs -m`: test(name, fn), then run() prints one line per test and exits 1
// when any failed.

import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import System from 'system';

const tests = [];

export function test(name, fn) {
    tests.push({name, fn});
}

const show = v => JSON.stringify(v);

export function equal(actual, expected, what = 'value') {
    if (show(actual) !== show(expected))
        throw new Error(`${what}: expected ${show(expected)}, got ${show(actual)}`);
}

export function ok(cond, what) {
    if (!cond)
        throw new Error(what);
}

export async function run(suite) {
    let failed = 0;
    for (const {name, fn} of tests) {
        try {
            await fn();
            print(`ok    ${suite}: ${name}`);
        } catch (e) {
            failed++;
            print(`FAIL  ${suite}: ${name}\n      ${e.message}`);
        }
    }
    print(`${suite}: ${tests.length - failed} of ${tests.length} passed`);
    if (failed > 0)
        System.exit(1);
}

const testsDir = GLib.path_get_dirname(GLib.filename_from_uri(import.meta.url)[0]);

// The lines of a fixture file under tests/fixtures/ (or $SVOLDER_FIXTURES_DIR), parsed.
export function fixture(name) {
    const dir = GLib.getenv('SVOLDER_FIXTURES_DIR') ?? `${testsDir}/fixtures`;
    const file = Gio.File.new_for_path(`${dir}/${name}.jsonl`);
    const [, bytes] = file.load_contents(null);
    return new TextDecoder().decode(bytes).split('\n').filter(l => l !== '').map(l => JSON.parse(l));
}

// `base` with `patch` laid over it: objects merge, everything else replaces.
export function patched(base, patch) {
    const out = JSON.parse(JSON.stringify(base));
    const lay = (dst, src) => {
        for (const [k, v] of Object.entries(src)) {
            if (v !== null && typeof v === 'object' && !Array.isArray(v) && dst[k] !== null && typeof dst[k] === 'object')
                lay(dst[k], v);
            else
                dst[k] = v;
        }
    };
    lay(out, patch);
    return out;
}
