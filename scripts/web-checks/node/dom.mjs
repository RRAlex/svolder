/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// A page for the node harnesses that need a real DOM: jsdom, the apt package `node-jsdom`.
//
// When to use it: whenever a case needs the focus (`document.activeElement`, what `focus()` and a
// removed element do to it), a key that travels (capture on the window before a listener of the
// document, `stopImmediatePropagation`, `preventDefault` read back), `closest`, `matches` and
// `querySelector` over real markup, `innerHTML` parsed as a browser parses it (escaping), or the
// history (`pushState`, `popstate`, Back). A hand-rolled fake of these answers what its author
// thought the browser does. A harness for pure code (a draw that returns a string, the menu's
// rows, a table) needs no page at all and imports nothing from here.
//
// What jsdom does not do: it lays nothing out (every size and place is 0, `elementFromPoint` is
// not there), knows no `inert` (a harness reads `el.inert` as the flag the page set), runs no
// media query, and a step of its history lands in a later task than a browser's (wait for
// `popstate`). Those belong to a browser check.
//
// Where jsdom is missing, a harness that imports this file skips loudly, never passes silently:
// it prints `SKIPPED <harness>: jsdom is missing` and exits 77, which the gate counts as a
// failure. With WEB_CHECK_ALLOW_NO_JSDOM=1 in the environment it prints the same line and exits
// 0, and run-all.sh starts without jsdom; the line is the harness's last, so the gate's row
// shows it. WEB_CHECK_JSDOM_MODULE names another module to load in its place (node/dom-skip.mjs
// names one that does not exist, to prove the skip).
import { createRequire } from 'node:module';
import { basename } from 'node:path';

/** The exit status of a harness that skipped for want of jsdom and was not allowed to. */
export const SKIPPED = 77;

let JSDOM = null;
try {
  ({ JSDOM } = createRequire(import.meta.url)(process.env.WEB_CHECK_JSDOM_MODULE ?? 'jsdom'));
} catch {
  const allowed = process.env.WEB_CHECK_ALLOW_NO_JSDOM === '1';
  const how = allowed ? '' : '; WEB_CHECK_ALLOW_NO_JSDOM=1 lets the gate pass without it';
  console.log(`SKIPPED ${basename(process.argv[1] ?? 'a harness')}: jsdom is missing (the apt `
    + `package node-jsdom)${how}`);
  process.exit(allowed ? 0 : SKIPPED);
}

/**
 * A page whose body is `html`, at `url`. Returns `{ window, document, key, settle }`:
 * `key(name, { on, ...init })` sends a keydown of `name` to `on` (the focused element by default)
 * and returns the event once every listener has run; `settle()` resolves after the tasks queued
 * so far (a step of the history lands in one).
 */
export function page(html = '', { url = 'http://svolder.test/' } = {}) {
  const { window } = new JSDOM(`<!doctype html><html><body>${html}</body></html>`, { url });
  const { document } = window;
  const key = (name, { on = document.activeElement ?? document.body, ...init } = {}) => {
    const event = new window.KeyboardEvent('keydown', {
      key: name, bubbles: true, cancelable: true, ...init,
    });
    on.dispatchEvent(event);
    return event;
  };
  const settle = () => new Promise((resolve) => { window.setTimeout(resolve, 0); });
  return { window, document, key, settle };
}
