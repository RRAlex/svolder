/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The hands on the tone: the mode and the preset words that open lists, the loudness strength's
// two steppers, and the band table's `−` and `+` (one device step a tap, for frequency, gain and Q
// alike). The graph's points are graph.js's. Every write goes through `core.actions`; the mode and
// the preset are logged by `act`. A viewer who may not write taps the same words, and the core
// says why nothing happens.
//
// The tone section is held while a list of it is open, so a change from elsewhere does not take the
// word from under the list: it is drawn from the newest model when the list closes.

import { wireGraph } from './graph.js';
import { closeList, closeOf, openList } from './lists.js';
import { keeper } from './live.js';
import { WORDS } from './words.js';

/** Wire the tone of the sheet in `root` until `signal` aborts. */
export function wireTone({
  root, core, live, act, signal,
}) {
  const box = root.querySelector('#p-tone');
  if (!box) return;
  const store = core.model;
  const { BAND_PARAM } = core.drag;

  // A drag and an open list may hold the section together: it is let go when both are done.
  const keep = keeper(live, 'tone');

  live.onSwitch(() => {
    closeList();
    keep.reset();
  });

  /** A tap that cannot write: the core turns it away and says why (a guest, a page whose writes
   * are off); the mode asked is the one it is already in. */
  const refuse = () => core.actions.setMode(store.part('tone')?.mode);

  /** Open the list `options` under `opener`, holding the tone section while it is open. */
  function list(opener, label, options, choose) {
    // The hold is taken before the list opens (opening one closes another, whose release must
    // not let the section go) and given back if it cannot open.
    keep.around(() => openList({
      opener, label, options, choose, signal, onClose: () => keep.drop(),
    }));
  }

  box.addEventListener('click', (event) => {
    const el = event.target.closest?.('[data-act]');
    if (!el) return;
    const device = store.device();
    const tone = store.part('tone');
    if (!tone || !device) return;
    const { act: what } = el.dataset;
    if (what === 'mode-list' || what === 'preset-list') {
      if (device.inert) {
        refuse();
        return;
      }
      if (closeOf(el)) return;
      if (what === 'mode-list') {
        list(el, WORDS.modeList, (tone.modes ?? []).map((m) => ({
          value: m.name, text: m.label, on: m.on,
        })), (name) => {
          core.toneNote.clicked(name);
          act.act('mode', name, (a) => a.setMode(name)).catch(() => {});
        });
      } else {
        list(el, WORDS.presetList, (device.presets?.rows ?? []).map((r) => ({
          value: r.name, text: r.name, on: r.on,
        })), (name) => {
          act.act('preset', name, (a) => a.recallPreset(name)).catch(() => {});
        });
      }
    } else if (what === 'band') {
      core.actions.setBand(Number(el.dataset.band), BAND_PARAM[el.dataset.param],
        { step: Number(el.dataset.step) }, { device: store.shown() });
    } else if (what === 'loudness') {
      core.actions.setLoudness({ step: Number(el.dataset.step) }, { device: store.shown() });
    }
  }, { signal });

  wireGraph({
    root, core, live, keep, refuse, signal,
  });
}
