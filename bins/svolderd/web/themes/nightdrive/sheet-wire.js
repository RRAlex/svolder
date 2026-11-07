/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The neon sheet's frame and its first row: opened by the dash's `DEVICE ▴` or the menu's `Device…`
// row, closed by ✕, Escape or a tap on the scene, with the focus going in and, on closing, back to
// what opened it. While it stands the scene and the dash under it are out of reach (`inert`) and
// dimmed, and the scene goes on drawing. The sheet is a dialog; its sections are drawn from the
// model by `draw` and the hands on them are bands-wire.js and presets-wire.js (this file works the
// mode and the input). Every write goes through `core.actions`; a viewer who may not write taps the
// same chips and the core says why nothing happens.
//
// The sheet is held (`keep`) while a value is typed, a list or a question is open or a stepper is
// held, so a change from elsewhere does not take it from under the hand; it is drawn from the
// newest model afterwards. Another device shown redraws it for that device and closes what was open
// for the old one.

import { computerView } from './computer-view.js';
import { drawSheet, sheetFrame } from './draw/sheet.js';
import { inertOf } from './inert.js';
import { closeAsk } from './ask.js';
import { closeList, closeOf, openList } from './lists.js';
import { createRelease, keeper } from './live.js';
import { placeMore, placeSheet } from './place.js';
import { SHEET } from './words.js';

/** What the sheet puts out of reach and dims while it stands. */
const BEHIND = '.nd-scene, #nd-layer';

/**
 * Wire the sheet in `root` (`#nd-sheet-wrap`) to `core` and `view` until `signal` aborts. `live` is
 * `createLive`'s handle, `act` the write log. Returns `{ draw(page), open(opener), close(refocus),
 * isOpen(), body(), wrap, keep, release }`; `keep` and `release` are the sheet's holds (live.js) for the wires
 * that work its other sections.
 */
export function wireSheet({
  root, core, live, act, view, signal,
}) {
  const store = core.model;
  const page = root.querySelector('#nightdrive');
  const wrap = root.querySelector('#nd-sheet-wrap');
  const keep = keeper(live, 'sheet');
  const release = createRelease(root.ownerDocument, keep, signal);
  let dialog = null;
  let body = null;
  /** What opened the sheet, `{ id, el }`, for the focus to go back to. */
  let opener = null;

  const inert = () => inertOf(store.page());
  const controlOf = (key) => (store.device()?.controls ?? []).find((c) => c.key === key);

  // ---- drawing ----
  function draw(model) {
    if (!dialog) return;
    if (!model?.device) {
      close();
      return;
    }
    const focus = body.contains(root.ownerDocument.activeElement) ? root.ownerDocument.activeElement.id : '';
    const scrolled = body.querySelector('#nd-pscroll')?.scrollTop ?? 0;
    body.innerHTML = drawSheet(model, view, computerView(core));
    placeSheet(body);
    const table = body.querySelector('#nd-pscroll');
    if (table) table.scrollTop = scrolled;
    placeMore(body);
    if (focus) {
      // a row removed takes its `×` with it: the focus stays in the dialog
      (body.querySelector(`#${CSS.escape(focus)}`) ?? dialog).focus({ preventScroll: true });
    }
  }

  // ---- opening and closing ----
  /** The page behind the sheet is out of reach and dimmed while it stands. */
  function shut(on) {
    for (const el of root.querySelectorAll(BEHIND)) el.inert = on;
    if (on) page.dataset.sheet = 'open';
    else delete page.dataset.sheet;
  }

  function close(refocus = true) {
    if (!dialog) return;
    closeList();
    closeAsk();
    keep.reset();
    dialog = null;
    body = null;
    shut(false);
    wrap.hidden = true;
    wrap.replaceChildren();
    if (!refocus) return;
    const doc = root.ownerDocument;
    const back = (opener?.id && doc.getElementById(opener.id))
      || (opener?.el?.isConnected ? opener.el : null)
      || doc.querySelector('#frame .fxbtn');
    back?.focus({ preventScroll: true });
  }

  function open(from) {
    if (dialog || !store.shown() || !store.device()) return;
    closeList();
    opener = from ? { id: from.id, el: from } : null;
    wrap.innerHTML = sheetFrame();
    dialog = wrap.querySelector('#nd-dialog');
    body = wrap.querySelector('#nd-sheet-body');
    wrap.hidden = false;
    shut(true);
    live.draw('sheet');
    const first = body.querySelector('[aria-pressed="true"]') ?? body.querySelector('button:not([disabled])')
      ?? wrap.querySelector('#nd-sheet-x');
    first.focus({ preventScroll: true });
  }
  view.openSheet = open;
  view.closeSheet = close;

  // ✕, and a tap on the scene (a press that began and ended on the backdrop: not a drag that left the sheet)
  let downOnBackdrop = false;
  wrap.addEventListener('pointerdown', (event) => { downOnBackdrop = event.target === wrap; }, { signal });
  wrap.addEventListener('scroll', (event) => {
    if (event.target.id === 'nd-pscroll') placeMore(body);
  }, { signal, capture: true, passive: true });
  wrap.addEventListener('click', (event) => {
    if (event.target.closest?.('[data-act="sheet-close"]')) close();
    else if (event.target === wrap && downOnBackdrop) close();
    downOnBackdrop = false;
  }, { signal });
  wrap.addEventListener('keydown', (event) => {
    if (event.key === 'Escape') {
      event.preventDefault();
      close();
      return;
    }
    if (event.key !== 'Tab') return;
    // the focus stays in the dialog
    const order = [...wrap.querySelectorAll('button:not([disabled]), input:not([disabled]), select:not([disabled])')];
    if (order.length === 0) {
      event.preventDefault();
      return;
    }
    const at = order.indexOf(event.target);
    const last = order.length - 1;
    if (!event.shiftKey && at === last) {
      event.preventDefault();
      order[0].focus({ preventScroll: true });
    } else if (event.shiftKey && at <= 0) {
      event.preventDefault();
      order[last].focus({ preventScroll: true });
    }
  }, { signal });

  // Another device is shown: a list or a question open for the old one is closed and every hold let go.
  live.onSwitch(() => {
    closeList();
    closeAsk();
    keep.reset();
  });
  signal.addEventListener('abort', () => close(false), { once: true });

  // ---- the mode and the input ----
  /** Write the option of the input control `c` through the log. */
  const writeInput = (c, option) => act.act('control:input', option.label,
    (a) => a.setControl('input', JSON.parse(option.json), { device: store.shown() })).catch(() => {});

  wrap.addEventListener('click', (event) => {
    const el = event.target.closest?.('#nd-mode [data-act], #nd-input [data-act]');
    if (!el) return;
    const device = store.device();
    if (!device) return;
    const what = el.dataset.act;
    if (what === 'mode' && core.computer.side() === 'computer') {
      // Off or EQ on the computer's EQ; a viewer who may not write is told so by the core
      act.act('mode', el.dataset.mode, (a) => a.computer.setMode(el.dataset.mode)).catch(() => {});
      return;
    }
    if (what === 'mode') {
      if (inert()) {
        // the core turns the tap away and says why; the mode asked is the one it is already in
        core.actions.setMode(store.part('tone')?.mode);
        return;
      }
      core.toneNote.clicked(el.dataset.mode);
      act.act('mode', el.dataset.mode, (a) => a.setMode(el.dataset.mode, { device: store.shown() })).catch(() => {});
      return;
    }
    const c = controlOf('input');
    if (!c) return;
    if (what === 'input') {
      const option = c.options?.find((o) => o.label === el.dataset.value);
      if (option) writeInput(c, option);
    } else if (what === 'input-list') {
      if (inert()) {
        const option = c.options?.find((o) => o.on) ?? c.options?.[0];
        if (option) writeInput(c, option);
        return;
      }
      if (closeOf(el)) return;
      keep.around(() => openList({
        opener: el,
        label: c.label ?? SHEET.input,
        options: (c.options ?? []).map((o) => ({ value: o.label, text: o.label, on: o.on })),
        choose(label) {
          const option = c.options.find((o) => o.label === label);
          if (option) writeInput(c, option);
        },
        signal,
        onClose: release,
      }));
    }
  }, { signal });

  return {
    draw, open, close, keep, release, wrap, isOpen: () => dialog !== null, body: () => body,
  };
}
