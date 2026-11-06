/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The hands on the shelf's presets: a tone preset applied, saved under a name, renamed or deleted,
// and the device's own presets, a slot recalled or written over, a built-in setting applied. A name
// is asked for in a form and the two things that cannot be undone in a question (ask.js); a slot is
// written only when its question is answered `Save`, with `confirm: true`; nothing is sent before.
// Every write goes through `core.actions`. A viewer who may not write taps the same words and the
// core says why nothing happens.
//
// The shelf is held while a form, a question or a list is open, so a change from elsewhere does not
// take the words from under it; it is drawn from the newest model when the last one closes. What is
// asked is asked of the device that was shown when it opened.

import { askForm, askQuestion, closeAsk } from './ask.js';
import { closeList, closeOf, openList } from './lists.js';
import { presetOf } from './presets.js';
import { SHELF } from './words.js';

/** Wire the presets of the shelf in `root` until `signal` aborts; `keep` is the shelf's count of
 * holds and `release()` gives one back once the press that closed a list is over (press.js). */
export function wirePresets({
  root, core, live, act, view, keep, release, signal,
}) {
  const box = root.querySelector('#sg-shelf');
  const target = () => ({ device: view.target() });

  /** The place a form or a question opens in: the ask of the section the word is in, found now (the
   * shelf may have been drawn again since the word was). */
  const within = (el) => el.closest('.sg-box2')?.querySelector('.sg-ask')
    ?? box.querySelector('.sg-ask');

  /** Open `list` under `opener` holding the shelf; `then` hears the value chosen. */
  function pick(opener, label, options, then) {
    if (closeOf(opener)) return;
    keep.around(() => openList({
      opener, label, options, choose: then, signal, onClose: release,
    }));
  }

  /** Hold the shelf while an ask is open; the ask's `onClose` gives the hold back. */
  const ask = (open) => keep.around(() => open(() => keep.dropLater()));

  // Another device is shown, or another hop chosen: what is open for the old one is closed.
  const away = () => {
    closeList();
    closeAsk();
    keep.reset();
  };
  live.onSwitch(away);
  live.onChoice(away);

  /** Ask for a new name of the preset `from` and rename it. */
  function rename(el, from, opened) {
    ask((onClose) => askForm({
      within: within(el), kind: 'rename', value: from, signal, opener: el, onClose,
      submit: (text) => act.act('preset', text, (a) => a.renamePreset(from, text, opened)),
    }));
  }

  /** Ask whether to delete the preset `from`; delete it only on the answer. */
  function remove(el, from, opened) {
    ask((onClose) => askQuestion({
      within: within(el), words: SHELF.askDelete(from), yes: 'delete', no: 'keep',
      signal, opener: el, onClose,
      go: () => act.act('preset', from, (a) => a.deletePreset(from, opened)),
    }));
  }

  /** Rename or delete: the preset the bands are on if it is the person's own, else a list of the
   * person's own to choose from. */
  function ownPreset(el, device, label, then) {
    const own = (device.presets?.rows ?? []).filter((r) => !r.built_in);
    const on = presetOf(device.presets);
    const current = own.find((r) => r.name === on?.name);
    if (current) {
      then(current.name);
      return;
    }
    pick(el, label, own.map((r) => ({ value: r.name, text: r.name, on: false })), then);
  }

  box.addEventListener('click', (event) => {
    const el = event.target.closest?.('[data-act]');
    if (!el) return;
    const device = view.device();
    if (!device) return;
    const what = el.dataset.act;
    // A word that cannot write does nothing, but for the viewer the core turns away: that call is
    // made so the toast says why.
    if (el.getAttribute('aria-disabled') === 'true' && !device.inert) return;
    const opened = target();
    if (what === 'preset') {
      act.act('preset', el.dataset.name, (a) => a.recallPreset(el.dataset.name, opened)).catch(() => {});
    } else if (what === 'preset-save') {
      ask((onClose) => askForm({
        within: within(el), kind: 'save', value: '', signal, opener: el, onClose,
        submit: (text) => act.act('preset', text, (a) => a.savePreset(text, opened)),
      }));
    } else if (what === 'preset-rename') {
      ownPreset(el, device, SHELF.whichRename, (name) => rename(el, name, opened));
    } else if (what === 'preset-delete') {
      ownPreset(el, device, SHELF.whichDelete, (name) => remove(el, name, opened));
    } else if (what === 'slot') {
      const slot = Number(el.dataset.slot);
      act.act('slot', slot, (a) => a.recallSlot(slot, opened)).catch(() => {});
    } else if (what === 'setting') {
      act.act('setting', el.dataset.name, (a) => a.applySetting(el.dataset.name, opened)).catch(() => {});
    } else if (what === 'slot-save-list') {
      const options = (device.device_presets?.slots ?? []).map((s) => ({
        value: s.slot, text: s.name, on: false,
      }));
      pick(el, SHELF.saveList, options, (value) => {
        const slot = options.find((o) => String(o.value) === String(value));
        // The write is sent only from the question's `Save`.
        ask((onClose) => askQuestion({
          within: within(el), words: SHELF.askSlot(slot.text), yes: 'save', no: 'keep',
          signal, opener: el, onClose,
          go: () => act.act('slot', slot.value, (a) => a.saveSlot(slot.value, true, opened)),
        }));
      });
    }
  }, { signal });
}
