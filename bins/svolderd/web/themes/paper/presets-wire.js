/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The hands on the presets: a tone preset applied, saved under a name, renamed or deleted, and the
// device's own presets, a slot recalled or written over, a built-in setting applied. Names are
// asked for in a form and the two things that cannot be undone in a question (ask.js); a slot is
// written only when its question is answered `save`, with `confirm: true`; nothing is sent
// before. Every write goes through `core.actions`. A viewer who may not write taps the same words
// and the core says why nothing happens.
//
// A section with a form, a question or a list open is held, so a change from elsewhere does not
// take the words from under it; it is drawn from the newest model when the last one closes. What
// is asked is asked of the device that was shown when it opened.

import { askForm, askQuestion, closeAsk } from './ask.js';
import { closeList, closeOf, openList } from './lists.js';
import { keeper } from './live.js';
import { WORDS } from './words.js';

/** Wire the two presets sections of the sheet in `root` until `signal` aborts. */
export function wirePresets({
  root, core, live, act, signal,
}) {
  const store = core.model;

  /** Wire one section: `name` is its element's name, `onWord(el, section)` hears a tap on a word. */
  function wireSection(name, onWord) {
    const box = root.querySelector(`#p-${name}`);
    if (!box) return;
    const keep = keeper(live, name);
    const section = {
      keep,
      /** The place a form or question is drawn in, found now: the section may have been drawn
       * again since the word was drawn. */
      within: () => box.querySelector('.p-ask'),
      /** Open `list` under `opener` holding the section; `then` hears the value chosen. */
      list(opener, label, options, then) {
        if (closeOf(opener)) return;
        keep.around(() => openList({
          opener, label, options, choose: then, signal, onClose: () => keep.drop(),
        }));
      },
      /** Hold the section while an ask is open; the ask's `onClose` gives the hold back. */
      ask(open) {
        keep.around(() => open(() => keep.drop()));
      },
    };
    // Another device is shown: what is open for the old one is closed.
    live.onSwitch(() => {
      closeList();
      closeAsk();
      keep.reset();
    });
    box.addEventListener('click', (event) => {
      const el = event.target.closest?.('[data-act]');
      if (!el) return;
      const device = store.device();
      if (!device) return;
      // A word that cannot write does nothing, but for the viewer the core turns away: that call
      // is made so the toast says why.
      if (el.getAttribute('aria-disabled') === 'true' && !device.inert) return;
      onWord(el, device, section);
    }, { signal });
  }

  const target = () => ({ device: store.shown() });

  wireSection('presets', (el, device, section) => {
    const { act: what, name } = el.dataset;
    if (what === 'preset') {
      act.act('preset', name, (a) => a.recallPreset(name, target())).catch(() => {});
      return;
    }
    const opened = target();
    if (what === 'preset-save') {
      section.ask((onClose) => askForm({
        within: section.within(), kind: 'save', value: '', signal, opener: el, onClose,
        submit: (text) => act.act('preset', text, (a) => a.savePreset(text, opened)),
      }));
    } else if (what === 'preset-edit') {
      const own = (device.presets?.rows ?? []).filter((r) => !r.built_in);
      const options = own.flatMap((r) => [
        { value: `rename:${r.name}`, text: WORDS.rename(r.name), on: false },
        { value: `delete:${r.name}`, text: WORDS.remove(r.name), on: false },
      ]);
      section.list(el, WORDS.editList, options, (value) => {
        const [kind, ...rest] = value.split(':');
        const from = rest.join(':');
        if (kind === 'rename') {
          section.ask((onClose) => askForm({
            within: section.within(), kind: 'rename', value: from, signal, opener: el, onClose,
            submit: (text) => act.act('preset', text, (a) => a.renamePreset(from, text, opened)),
          }));
        } else {
          section.ask((onClose) => askQuestion({
            within: section.within(), words: WORDS.askDelete(from), yes: 'delete', no: 'keep',
            signal, opener: el, onClose,
            go: () => act.act('preset', from, (a) => a.deletePreset(from, opened)),
          }));
        }
      });
    }
  });

  wireSection('device-presets', (el, device, section) => {
    const { act: what } = el.dataset;
    if (what === 'slot') {
      const slot = Number(el.dataset.slot);
      act.act('slot', slot, (a) => a.recallSlot(slot, target())).catch(() => {});
    } else if (what === 'setting') {
      const { name } = el.dataset;
      act.act('setting', name, (a) => a.applySetting(name, target())).catch(() => {});
    } else if (what === 'slot-save-list') {
      const opened = target();
      const options = (device.device_presets?.slots ?? []).map((s) => ({
        value: s.slot, text: s.name, on: false,
      }));
      section.list(el, WORDS.saveList, options, (value) => {
        const slot = options.find((o) => o.value === value);
        // The write is sent only from the question's `save`.
        section.ask((onClose) => askQuestion({
          within: section.within(), words: WORDS.askSlot(slot.text), yes: 'save', no: 'keep',
          signal, opener: el, onClose,
          go: () => act.act('slot', slot.value, (a) => a.saveSlot(slot.value, true, opened)),
        }));
      });
    }
  });
}
