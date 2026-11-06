/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The hands on an amp's open area: its power and switches, Identify, and Unlink (a question in
// place first). The amp is not the device shown, so every write names its URL (`{ device: url }`);
// the URL is read from the hop chosen at the moment of the tap, and nothing is written once the
// amp has left the line (a chosen amp that is unplugged, unlisted or unlinked gives the box back to
// the shown device, and the area is drawn for that one). A viewer who may not write taps the same
// words and the core says why nothing happens.

import { askQuestion, closeAsk } from './ask.js';
import { AMP } from './words.js';

/** Wire the amp's area in `root` until `signal` aborts; `keep` is the count of holds on the open
 * area. */
export function wireAmp({
  root, core, live, act, view, keep, signal,
}) {
  const box = root.querySelector('#sg-open');
  const store = core.model;

  /** The amp the area shows now: its hop on the line and its model; null once it left. */
  function current() {
    const hop = view.hops().find((one) => one.id === view.chosen);
    if (!hop || hop.kind !== 'device' || !hop.url) return null;
    const device = store.device(hop.url);
    return device ? { hop, device, url: hop.url } : null;
  }

  // Another device is shown, or another hop chosen, or the amp left: the question closes.
  const away = () => {
    closeAsk();
    keep.reset();
  };
  live.onSwitch(away);
  live.onChoice(away);

  box.addEventListener('click', (event) => {
    const el = event.target.closest?.('[data-act]');
    if (!el || !el.closest('#sg-amp-power, #sg-amp-about')) return;
    const amp = current();
    if (!amp) return;
    const what = el.dataset.act;
    if (what === 'power' || what === 'power-toggle') {
      const p = amp.device.power;
      const target = what === 'power' ? p : (p?.toggles ?? []).find((t) => t.key === el.dataset.key);
      if (target?.json) {
        act.act(`control:${target.key}`, target.on !== true,
          (a) => a.setControl(target.key, JSON.parse(target.json), { device: amp.url })).catch(() => {});
      }
    } else if (what === 'amp-identify') {
      core.actions.identify({ device: amp.url });
    } else if (what === 'amp-unlink') {
      const hopId = el.dataset.hopId;
      if (amp.device.inert) {
        core.actions.hops.remove(hopId);
        return;
      }
      const within = el.closest('#sg-amp-about').querySelector('.sg-ask');
      keep.around(() => askQuestion({
        within, words: AMP.askUnlink(amp.device.name), yes: 'unlink', no: 'keep', signal, opener: el,
        onClose: () => keep.dropLater(),
        go: () => core.actions.hops.remove(hopId),
      }));
    }
  }, { signal });
}
