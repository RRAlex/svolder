/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The hands on the sheet's presets: a tone preset applied, saved under a name, renamed or deleted. A
// name is asked for in a form and the thing that cannot be undone in a question (ask.js); nothing is
// sent before the answer. Every write goes through `core.actions`. A viewer who may not write taps
// the same words and the core says why nothing happens.
//
// The computer's presets are the same list (the person's own renamed, deleted and saved the same way); a preset
// recalled there that a connected device plays too is the core's question (core.ask), whose card is dressed by
// Night drive's frame tokens.
//
// The sheet is held while a form, a question or a list is open, so a change from elsewhere does not
// take the words from under it; it is drawn from the newest model when the last one closes. What is
// asked is asked of the device that was shown when it opened.

import { askForm, askQuestion } from './ask.js';
import { presetsOf } from './draw/sheet.js';
import { inertOf } from './inert.js';
import { closeOf, openList } from './lists.js';
import { SHEET } from './words.js';

/** The preset the bands are on if it is one of the person's own: its name, else null. */
function ownOn(presets) {
  const on = presets?.rows?.find((r) => r.on);
  const name = on?.name ?? presets?.status?.name;
  return (presets?.rows ?? []).find((r) => !r.built_in && r.name === name)?.name ?? null;
}

/** Wire the presets of the sheet (`sheet.wrap`) until `signal` aborts. */
export function wirePresets({
  core, act, sheet, signal,
}) {
  const store = core.model;
  const { keep, release, wrap } = sheet;
  const onComputer = () => core.computer.side() === 'computer';

  /** What holds the presets of the side shown: the device, or the computer's tone shaped like one. */
  const holder = (device) => {
    if (!onComputer()) return device;
    const tone = core.computer.toneOf(store.page());
    return tone ? { ...device, presets: presetsOf(tone) } : device;
  };

  /** The computer's list is read again once a write of it is answered ok (a name changed it). */
  const reread = async (answer) => {
    if (answer?.ok === true && onComputer()) await core.computer.resync();
    return answer;
  };

  /** The place a form or a question opens in: the ask of the section the word is in, found now (the
   * sheet may have been drawn again since the word was). */
  const within = (el) => el.closest('.nd-sec')?.querySelector('.nd-ask') ?? wrap.querySelector('.nd-ask');

  /** Hold the sheet while an ask is open; the ask's `onClose` gives the hold back. */
  const ask = (open) => keep.around(() => open(() => keep.dropLater()));

  /** Open `options` as a list under `opener`, holding the sheet; `then` hears the value chosen. */
  function pick(opener, label, options, then) {
    if (closeOf(opener)) return;
    keep.around(() => openList({
      opener, label, options, choose: then, signal, onClose: release,
    }));
  }

  /** Ask for a new name of the preset `from` and rename it. */
  function rename(el, from, opened) {
    ask((onClose) => askForm({
      within: within(el), kind: 'rename', value: from, signal, opener: el, onClose,
      submit: (text) => act.act('preset', text, (a) => a.renamePreset(from, text, opened)).then(reread),
    }));
  }

  /** Ask whether to delete the preset `from`; delete it only on the answer. */
  function remove(el, from, opened) {
    ask((onClose) => askQuestion({
      within: within(el), words: SHEET.askDelete(from), yes: 'delete', no: 'keep',
      signal, opener: el, onClose,
      go: () => act.act('preset', from, (a) => a.deletePreset(from, opened)).then(reread),
    }));
  }

  /** Rename or delete: the preset the bands are on if it is the person's own, else a list of the
   * person's own to choose from. */
  function ownPreset(el, device, label, then) {
    const own = (device.presets?.rows ?? []).filter((r) => !r.built_in);
    const current = ownOn(device.presets);
    if (current) {
      then(current);
      return;
    }
    pick(el, label, own.map((r) => ({ value: r.name, text: r.name, on: false })), then);
  }

  wrap.addEventListener('click', (event) => {
    const el = event.target.closest?.('#nd-presets [data-act]');
    if (!el) return;
    const device = store.device();
    if (!device) return;
    const what = el.dataset.act;
    // A word that cannot write does nothing, but for the viewer the core turns away: that call is
    // made so the toast says why.
    if (el.getAttribute('aria-disabled') === 'true' && !inertOf(store.page())) return;
    const opened = { device: store.shown() };
    if (what === 'preset') {
      const { name } = el.dataset;
      const chip = el.id;
      act.act('preset', name, (a) => (onComputer() ? a.computer.recallPreset(name) : a.recallPreset(name, opened)))
        .catch(() => {})
        .then(() => {
          // the question card gave the focus back to a chip the sheet may have drawn again meanwhile
          const doc = wrap.ownerDocument;
          if (chip && (!doc.activeElement || doc.activeElement === doc.body)) doc.getElementById(chip)?.focus({ preventScroll: true });
        });
    } else if (what === 'preset-save') {
      ask((onClose) => askForm({
        within: within(el), kind: 'save', value: '', signal, opener: el, onClose,
        submit: (text) => act.act('preset', text, (a) => (onComputer() ? a.computer.savePreset(text) : a.savePreset(text, opened))),
      }));
    } else if (what === 'preset-rename') {
      ownPreset(el, holder(device), SHEET.whichRename, (name) => rename(el, name, opened));
    } else if (what === 'preset-delete') {
      ownPreset(el, holder(device), SHEET.whichDelete, (name) => remove(el, name, opened));
    }
  }, { signal });
}
