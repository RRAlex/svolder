/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The hands on the transport, the amp's power and switches, the card's `try now` and *Device
// settings…* (which opens the Devices popup); and the clock that keeps the card's seconds to the
// next try, the track note's time and the footer's date current while the page is on show. Every
// write goes through `core.actions` (the power and the switches are logged by `act`). A viewer who
// may not write taps the same words and the core says why nothing happens.

import { ink } from './draw/notes.js';
import { trackLine } from './notes.js';
import { WORDS, dateWords } from './words.js';

/** How often the seconds, the note and the date are written again (ms). */
const TICK_MS = 1000;

/** Wire the sections of the sheet in `root` until `signal` aborts. */
export function wireDevice({
  root, core, act, signal,
}) {
  const store = core.model;
  const on = (id, fn) => root.querySelector(`#p-${id}`)?.addEventListener('click', (event) => {
    const el = event.target.closest?.('[data-act]');
    if (el) fn(el, el.dataset.act);
  }, { signal });

  on('playing', (el, what) => {
    if (what === 'transport') core.actions.press(el.dataset.button);
  });

  on('power', (el, what) => {
    const p = store.device()?.power;
    if (!p) return;
    const key = el.dataset.key;
    const toggle = (p.toggles ?? []).find((c) => c.key === key);
    const target = what === 'power' && key === p.key ? p : what === 'toggle' ? toggle : null;
    if (!target?.json) return;
    act.act(`control:${key}`, target.on !== true, (a) => a.setControl(key, JSON.parse(target.json),
      { device: store.shown() })).catch(() => {});
  });

  on('card', (_el, what) => {
    if (what === 'try') core.actions.connect({ device: store.shown() });
  });

  on('device', (_el, what) => {
    if (what === 'device-settings' && store.shown()) core.popups.openDevice(store.shown());
  });

  // ---- the clock ----

  /** When this page first saw each drawn retry line, by this browser's clock. */
  const arrived = new WeakMap();
  const seen = (el) => {
    if (!arrived.has(el)) arrived.set(el, Date.now());
    return arrived.get(el);
  };

  function tick() {
    // The track note's line, worked out as the first draw works it out, on the daemon's clock now.
    for (const el of root.querySelectorAll('.pn[data-note="track"][data-clock]')) {
      const d = el.dataset;
      const text = trackLine({
        status: d.status,
        length_us: d.length ? Number(d.length) : null,
        album: d.album,
        progress: d.position === undefined ? null : {
          position_us: Number(d.position), rate_milli: Number(d.rate), read_at_ms: Number(d.readAt),
        },
      }, Number(d.clock) + (Date.now() - Number(d.drawn)));
      if (d.words !== text) {
        d.words = text;
        el.innerHTML = ink(text);
      }
    }
    // The footer's date, written again when the day turns over under an open page.
    const left = root.querySelector('#p-foot .p-left');
    if (left) {
      const text = left.textContent;
      const at = text.lastIndexOf(' · ');
      const dated = at < 0 ? text : `${text.slice(0, at + 3)}${dateWords(Date.now())}`;
      if (dated !== text) left.textContent = dated;
    }
    for (const el of root.querySelectorAll('.p-retry[data-secs]')) {
      const left = Math.max(0, Math.ceil(Number(el.dataset.secs) - (Date.now() - seen(el)) / 1000));
      const text = WORDS.retryIn(left);
      if (el.textContent !== text) el.textContent = text;
    }
  }

  let timer = 0;
  const watch = core.lifecycle.watchVisible({
    onVisible(shown) {
      clearInterval(timer);
      timer = 0;
      if (shown) {
        tick();
        timer = setInterval(tick, TICK_MS);
      }
    },
  });
  signal.addEventListener('abort', () => {
    clearInterval(timer);
    watch.stop();
  }, { once: true });
}
