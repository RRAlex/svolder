/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The hands on the hops of the line: the `+` at its end with the sheet it opens, and in the open area
// Edit (its form in place), Move earlier / later and Remove (a question first). Every write is
// `core.actions.hops`, which sends the change to the configuration and answers the list as it now
// stands. The device the hop follows and the device an amp links are named as the configuration
// names them (`after`, `device`), which is the name the fleet lists. The sheet and the form hold the
// open area while they stand, so nothing typed is lost to a reading from elsewhere. A viewer who may
// not write sees the same words and the core says why nothing happens.

import { askQuestion } from './ask.js';
import { editForm, plusSheet } from './draw/hop.js';
import { HOPS } from './words.js';

/** What the fields of `form` hold: the chip chosen (an amp's URL and name, or a kind), the name and
 * the note as typed. */
function read(form) {
  const chip = form.querySelector('[role=radio][aria-checked=true]');
  const name = form.querySelector('input[id$="-name"]');
  const note = form.querySelector('input[id$="-note"]');
  return {
    device: chip?.dataset.device ?? null,
    deviceName: chip?.dataset.name ?? null,
    kind: chip?.dataset.kind ?? null,
    name: name ? name.value.trim() : null,
    note: note ? note.value.trim() : '',
  };
}

/** The fields that were off before a send, so that they stay off when it is refused. */
const wasOff = new WeakSet();

/** Turn the words of `form` off while an answer is on its way, back on (those that were on) if it
 * was refused. */
function busy(form, on) {
  for (const el of form.querySelectorAll('button, input')) {
    if (on) {
      if (el.disabled) wasOff.add(el);
      el.disabled = true;
    } else if (wasOff.delete(el) === false) {
      el.disabled = false;
    }
  }
}

/** Wire the menu's Add device row and the hops' buttons in `root` until `signal` aborts (it returns
 * `{ add, canAdd }`: what the row runs, and whether it can); `keep` is the count of holds
 * on the open area. */
export function wireHops({
  root, core, live, view, keep, signal,
}) {
  const store = core.model;
  const wrap = root.querySelector('#sg-sheet');
  const box = root.querySelector('#sg-open');
  /** The sheet and the edit form open now (their `close(refocus)`), or null. */
  let sheet = null;
  let edit = null;

  const inert = () => {
    const viewer = store.page().viewer ?? {};
    return viewer.guest === true || viewer.writes === false;
  };
  const rowOf = (id) => (store.page().fleet?.hops ?? []).find((row) => row.id === id);
  const dacName = () => (store.page().fleet?.devices ?? []).find((d) => d.url === store.shown())?.name ?? null;

  /** A chip of a kind group chosen: it alone is checked; an amp svolder controls gives its own name,
   * a kind gives the name back as it was typed. */
  function pick(form, chip) {
    const name = form.querySelector('input[id$="-name"]');
    for (const one of form.querySelectorAll('[role=radio]')) {
      const on = one === chip;
      one.classList.toggle('on', on);
      one.setAttribute('aria-checked', String(on));
    }
    if (!name) return;
    if (chip.dataset.device) {
      if (!name.disabled) form.dataset.typed = name.value;
      name.value = chip.dataset.name;
      name.disabled = true;
    } else {
      name.disabled = false;
      // Leaving a connected amp's own name: what was typed before it, else nothing.
      name.value = form.dataset.typed ?? '';
    }
    const note = form.querySelector('#sg-sheet-note');
    if (note) note.textContent = chip.dataset.device ? HOPS.linkedNote : HOPS.declaredNote;
  }
  const onChip = (event) => {
    const chip = event.target.closest?.('[data-act=hop-kind]');
    if (chip) pick(chip.closest('form'), chip);
  };

  // ---- the sheet ----

  /** The page behind the sheet is out of reach while it stands. */
  const behind = () => root.querySelectorAll('.sg-bandwrap, #sg-open, #sg-shelf, #sg-now, #sg-slim');
  const shut = (on) => behind().forEach((el) => { el.inert = on; });

  function closeSheet(refocus = true) {
    if (!sheet) return;
    sheet = null;
    shut(false);
    wrap.hidden = true;
    wrap.replaceChildren();
    if (refocus) root.ownerDocument.querySelector('#frame .fxbtn')?.focus({ preventScroll: true });
    keep.dropLater();
  }

  async function sendSheet(form) {
    const f = read(form);
    if (!form.reportValidity()) return;
    const after = dacName();
    if (after === null) return;
    const base = { note: f.note || undefined, after };
    const body = f.device ? { device: f.deviceName, ...base } : { name: f.name, kind: f.kind, ...base };
    const before = new Set(view.hops().map((hop) => hop.id));
    busy(form, true);
    let answer = null;
    try {
      answer = await core.actions.hops.add(body);
    } catch {
      answer = null;
    }
    if (!sheet) return;
    if (answer?.ok !== true) {
      busy(form, false);
      (form.querySelector('input:not([disabled])') ?? form.querySelector('button')).focus({ preventScroll: true });
      return;
    }
    closeSheet();
    const fresh = view.hops().find((hop) => !before.has(hop.id));
    if (fresh) view.choose(fresh.id);
  }

  function openSheet() {
    closeSheet(false);
    if (dacName() === null) return;
    wrap.innerHTML = plusSheet(store.page());
    wrap.hidden = false;
    sheet = { close: closeSheet };
    shut(true);
    keep.take();
    const form = wrap.querySelector('form');
    form.addEventListener('submit', (event) => {
      event.preventDefault();
      sendSheet(form);
    });
    (form.querySelector('[role=radio][aria-checked=true]') ?? form.querySelector('input')).focus({ preventScroll: true });
  }

  /** The menu's Add device row: the sheet, or for a viewer who may not write the core's refusal. */
  function add() {
    if (inert()) {
      core.actions.hops.add({});
      return;
    }
    openSheet();
  }
  /** Whether the row can do anything: the sheet needs a shown device; a viewer who may not write gets
   * the core's refusal. */
  const canAdd = () => inert() || dacName() !== null;
  wrap.addEventListener('click', (event) => {
    onChip(event);
    if (event.target === wrap || event.target.closest?.('[data-act=sheet-cancel]')) closeSheet();
  }, { signal });
  wrap.addEventListener('keydown', (event) => {
    if (event.key === 'Escape') {
      event.preventDefault();
      closeSheet();
      return;
    }
    if (event.key !== 'Tab') return;
    const order = [...wrap.querySelectorAll('button:not([disabled]), input:not([disabled])')];
    if (order.length === 0) {
      // Every field is off while a send is on its way: the focus still stays in the sheet.
      event.preventDefault();
      return;
    }
    const at = order.indexOf(event.target);
    const to = event.shiftKey ? (at <= 0 ? order.length - 1 : at - 1) : (at === order.length - 1 ? 0 : at + 1);
    event.preventDefault();
    order[to].focus({ preventScroll: true });
  }, { signal });

  // ---- Edit, Move, Remove ----

  function closeEdit(refocus = true) {
    if (!edit) return;
    const place = box.querySelector('#sg-hop-edit');
    edit = null;
    place?.replaceChildren();
    if (refocus) box.querySelector('#sg-hop-edit-open')?.focus({ preventScroll: true });
    keep.dropLater();
  }

  async function sendEdit(form, row) {
    if (!form.reportValidity()) return;
    const f = read(form);
    const index = (store.page().fleet?.hops ?? []).findIndex((one) => one.id === row.id);
    const note = f.note === '' ? null : f.note;
    busy(form, true);
    let answer = null;
    try {
      answer = await core.actions.hops.edit(row.id, row.device ? { note } : { name: f.name, kind: f.kind, note });
    } catch {
      answer = null;
    }
    if (answer?.ok !== true) {
      if (!edit) return;
      busy(form, false);
      (form.querySelector('input:not([disabled])') ?? form.querySelector('button')).focus({ preventScroll: true });
      return;
    }
    closeEdit(false);
    // A new name is a new id: the box follows the hop to where the list now has it.
    const now = answer.hops?.[index];
    const hop = now && view.hops().find((one) => one.hopId === now.id);
    if (hop) view.choose(hop.id);
    (box.querySelector('#sg-hop-edit-open') ?? root.querySelector(`[data-hop="${CSS.escape(hop?.id ?? '')}"]`))
      ?.focus({ preventScroll: true });
  }

  function openEdit(row) {
    closeEdit(false);
    const place = box.querySelector('#sg-hop-edit');
    if (!place) return;
    place.innerHTML = editForm(row);
    place.scrollIntoView?.({ block: 'nearest' });
    edit = { row };
    keep.take();
    const form = place.querySelector('form');
    form.addEventListener('submit', (event) => {
      event.preventDefault();
      sendEdit(form, row);
    });
    form.addEventListener('keydown', (event) => {
      if (event.key !== 'Escape') return;
      event.preventDefault();
      event.stopPropagation();
      closeEdit();
    });
    (form.querySelector('input:not([disabled])')).focus({ preventScroll: true });
  }

  box.addEventListener('click', (event) => {
    onChip(event);
    const el = event.target.closest?.('[data-act]');
    if (!el || !(el.dataset.act.startsWith('hop-') || el.dataset.act === 'edit-cancel')) return;
    if (el.dataset.act === 'edit-cancel') {
      closeEdit();
      return;
    }
    const id = el.dataset.hopId;
    const what = el.dataset.act;
    if (what === 'hop-kind') return;
    // A viewer who may not write: the core is asked and says why (it offers no way to say it
    // without a call, and it refuses before any request); nothing is drawn or posted.
    if (inert()) {
      const ask = { 'hop-edit': () => core.actions.hops.edit(id, {}), 'hop-move': () => core.actions.hops.move(id, 0),
        'hop-remove': () => core.actions.hops.remove(id) };
      ask[what]?.();
      return;
    }
    const row = rowOf(id);
    if (!row) return;
    if (what === 'hop-edit') {
      openEdit(row);
    } else if (what === 'hop-move') {
      if (el.dataset.to !== '') core.actions.hops.move(id, Number(el.dataset.to));
    } else if (what === 'hop-remove') {
      const within = el.closest('#sg-hop-main').querySelector('.sg-ask');
      keep.around(() => askQuestion({
        within, words: HOPS.askRemove(row.name), yes: 'remove', no: 'keep', signal, opener: el,
        onClose: () => keep.dropLater(),
        go: () => core.actions.hops.remove(id),
      }));
    }
  }, { signal });

  // Another hop chosen, or another device shown: what was open for the old one closes.
  live.onChoice(() => closeEdit(false));
  live.onSwitch(() => {
    closeEdit(false);
    closeSheet(false);
  });
  signal.addEventListener('abort', () => {
    shut(false);
    wrap.replaceChildren();
  }, { once: true });
  return { add, canAdd };
}
