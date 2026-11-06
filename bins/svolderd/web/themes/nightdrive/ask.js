/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// What a written word asks before it writes: a form for a name (save a preset, rename one) and a
// question for something that cannot be undone (delete a preset, replace what a slot of the device
// holds). Both are drawn in a place the section keeps for them (`within`), in place, as a field and
// words, and give the focus back to the word that opened them when they close (found again by its
// id if its section was drawn again meanwhile). An answer that is refused leaves the form or the
// question open, with what was typed: the core has said why. The question's safe answer has the
// focus, so Enter keeps things as they are. One is open at a time.

import { esc } from './draw/esc.js';
import { SHEET } from './words.js';

/** The ask open now, or nothing. */
let current = null;

/** The answer the word under `event`'s target stands for, from its `data-answer`. */
const answerOf = (event) => event.target.closest?.('[data-answer]')?.dataset.answer ?? null;

/** Draw `html` inside `within`, close the one open, and wire what both share: `finish(answer)`
 * is called with the answer a word gives, Escape gives `no`, the page going closes it. Returns
 * `{ close, box }`. */
function open({ within, html, className, signal, onClose, opener, finish, no }) {
  if (current) current.close();
  const box = within.ownerDocument.createElement('div');
  box.className = className;
  box.innerHTML = html;
  within.replaceChildren(box);
  // The sheet scrolls inside itself: bring the ask into view.
  box.scrollIntoView?.({ block: 'nearest' });
  const stop = new AbortController();
  const openerId = opener?.id;
  let closed = false;
  const handle = {
    box,
    close(refocus = true) {
      if (closed) return;
      closed = true;
      stop.abort();
      box.remove();
      if (current === handle) current = null;
      // The word that opened it may have been drawn again meanwhile: its twin by id gets the focus.
      const back = opener?.isConnected ? opener
        : (openerId ? within.ownerDocument.getElementById(openerId) : null);
      if (refocus) back?.focus({ preventScroll: true });
      onClose?.();
    },
  };
  current = handle;
  box.addEventListener('keydown', (event) => {
    if (event.key !== 'Escape') return;
    event.preventDefault();
    event.stopPropagation();
    finish(no);
  }, { signal: stop.signal });
  signal?.addEventListener('abort', () => handle.close(false), { once: true, signal: stop.signal });
  return handle;
}

/** Turn the words of `box` off while an answer is on its way, back on if it was refused. */
function busy(box, on) {
  for (const el of box.querySelectorAll('button, input')) el.disabled = on;
}

/**
 * A form for a name inside `within`: a ruled input (`maxlength` 40, required) holding `value`,
 * then `save` or `rename` and `cancel`. `submit(text)` sends it and resolves the answer: closed
 * on `ok`, open as it was (the text kept) otherwise. `opener` gets the focus back and `onClose`
 * hears every way it ends. `kind` is `save` or `rename`. Returns `{ close }`.
 */
export function askForm({
  within, kind, value = '', submit, signal, opener, onClose,
}) {
  const label = kind === 'rename' ? SHEET.newName : SHEET.presetName;
  const yes = SHEET.answers[kind];
  let handle = null;
  let sending = false;
  async function send() {
    const input = handle.box.querySelector('input');
    if (sending || !input.reportValidity()) return;
    sending = true;
    busy(handle.box, true);
    let answer = null;
    try {
      answer = await submit(input.value);
    } catch {
      answer = null;
    }
    sending = false;
    if (answer?.ok === true) {
      handle.close();
      return;
    }
    busy(handle.box, false);
    input.focus({ preventScroll: true });
  }
  handle = open({
    within, signal, onClose, opener, className: 'nd-askbox',
    html: `<form class="nd-form"><input class="nd-input" type="text" maxlength="40" required`
      + ` autocomplete="off" aria-label="${esc(label)}" value="${esc(value)}">`
      + ` <button type="submit" class="nd-btn nd-pill" data-answer="${esc(kind)}">${esc(yes)}</button>`
      + ` <button type="button" class="nd-btn nd-pill" data-answer="cancel">`
      + `${esc(SHEET.answers.cancel)}</button></form>`,
    finish: (answer) => {
      if (answer === 'cancel' && !sending) handle.close();
    },
    no: 'cancel',
  });
  const form = handle.box.querySelector('form');
  form.addEventListener('submit', (event) => {
    event.preventDefault();
    send();
  });
  handle.box.addEventListener('click', (event) => {
    if (answerOf(event) === 'cancel' && !sending) handle.close();
  });
  const input = form.querySelector('input');
  input.focus({ preventScroll: true });
  input.select();
  return { close: () => handle.close() };
}

/**
 * A question inside `within`: `words`, then the answer `yes` (`delete` or `save`) and `no`
 * (`keep`). `go()` is called only when `yes` is given, and resolves the answer: closed on `ok`,
 * open otherwise. `no` and Escape close it and send nothing. `keep` has the focus. Returns
 * `{ close }`.
 */
export function askQuestion({
  within, words, yes, no = 'keep', go, signal, opener, onClose,
}) {
  let handle = null;
  let sending = false;
  async function send() {
    if (sending) return;
    sending = true;
    busy(handle.box, true);
    let answer = null;
    try {
      answer = await go();
    } catch {
      answer = null;
    }
    sending = false;
    if (answer?.ok === true) {
      handle.close();
      return;
    }
    busy(handle.box, false);
    handle.box.querySelector(`[data-answer="${no}"]`)?.focus({ preventScroll: true });
  }
  handle = open({
    within, signal, onClose, opener, className: 'nd-askbox',
    html: `<p class="nd-question" role="alert">${esc(words)}</p>`
      + `<p class="nd-set"><button type="button" class="nd-btn nd-pill${yes === 'delete' || yes === 'remove' ? ' danger' : ''}" `
      + `data-answer="${esc(yes)}">${esc(SHEET.answers[yes])}</button>`
      + ` <button type="button" class="nd-btn nd-pill" data-answer="${esc(no)}">`
      + `${esc(SHEET.answers[no])}</button></p>`,
    finish: (answer) => {
      if (answer === no && !sending) handle.close();
    },
    no,
  });
  handle.box.addEventListener('click', (event) => {
    const answer = answerOf(event);
    if (answer === yes) send();
    else if (answer === no && !sending) handle.close();
  });
  handle.box.querySelector(`[data-answer="${no}"]`).focus({ preventScroll: true });
  return { close: () => handle.close() };
}

/** Close the form or question that is open, if any, without giving the focus back (the sheet
 * changed under it). */
export function closeAsk() {
  current?.close(false);
}
