/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// What a written word asks before it writes: a form for a name (save a preset, rename one) and a
// question for something that cannot be undone (delete a preset, replace what a slot of the device
// holds). Both are drawn in a place the caller keeps for them (`within`), in place, as a field and
// words, and give the focus back to the word that opened them when they close (found again by its
// id if its section was drawn again meanwhile). An answer that is refused leaves the form or the
// question open, with what was typed: the core has said why. The question's safe answer has the
// focus, so Enter keeps things as they are.
//
// One is open at a time in each scope. The scope is the modal of the stack that `within` sits in
// (`modalOf`: the element the stack was given, in the page or not, so an ask a popup draws before
// it is opened or closes after it has left is the popup's), else the page: an ask opened in a
// popup closes the ask open in that popup and leaves the page's alone, and the other way round. A
// modal that closes closes the ask inside it; `closeAsk()` closes the page's and `closeAsk(el)`
// the one of the modal `el` is or sits in.
//
// The look and the words are the caller's: `classes` names the classes drawn (a theme passes its
// own; the defaults, `FRAME_CLASSES`, are dressed by the `--frame-*` tokens in frame.css, for the
// popups) and `words` holds the field labels and the answers' words (`presetName`, `newName`,
// `answers: { save, rename, delete, keep, cancel, ... }`).
//
// Keys: the part attaches no key listener of its own. Each ask returns `key(event)`: Escape
// answers `no` (cancel for a form, `no` for a question) and the call returns true; every other key
// is left to the browser (typing in the field, Tab, Enter submitting the form). Whoever owns the
// keyboard hands the key in: a popup's modal from its `onKey`, a theme's page through
// `listen(ask)`. The module reaches no service of the core and may be imported directly; the one
// thing it keeps is which ask is open in each scope.

import { attr, esc } from './html.js';
import { MODAL_LEFT, modalOf } from './modal.js';

/** The classes the popups draw with; each one is dressed by frame.css. */
export const FRAME_CLASSES = {
  box: 'fi-askbox', form: 'fi-form', input: 'fi-input', button: 'fi-btn', set: 'fi-set',
  question: 'fi-question', danger: 'danger',
};

/** The scope of everything outside a modal of the stack. */
const PAGE = 'page';

/** The ask open in each scope that has one, by the modal's element or `PAGE`. */
const asks = new Map();

/** The scope `el` sits in. */
const scopeOf = (el) => modalOf(el) ?? PAGE;

/** Counts the questions asked, so each has an id of its own. */
let questions = 0;

/** The answer a word under the event's target stands for, from its `data-answer`. */
const answerOf = (event) => event.target.closest?.('[data-answer]')?.dataset.answer ?? null;

/** Draw `html` inside `within`, close the one open in its scope, and wire what both share:
 * `finish(answer)` is called with the answer Escape gives (`no`), the page going or its modal
 * leaving closes it. Returns the handle `{ close, box, key }`. */
function open({ within, html, className, signal, onClose, opener, finish, no, scroll }) {
  const scope = scopeOf(within);
  asks.get(scope)?.close();
  const box = within.ownerDocument.createElement('div');
  box.className = className;
  box.innerHTML = html;
  within.replaceChildren(box);
  // A long place scrolls: bring the ask into view (clear of what the caller pins, by its scroll
  // margin).
  if (scroll) box.scrollIntoView?.({ block: 'nearest' });
  const stop = new AbortController();
  const openerId = opener?.id;
  let closed = false;
  const gone = () => handle.close(false);
  const handle = {
    box,
    close(refocus = true) {
      if (closed) return;
      closed = true;
      stop.abort();
      box.remove();
      if (asks.get(scope) === handle) asks.delete(scope);
      if (scope !== PAGE) scope.removeEventListener(MODAL_LEFT, gone);
      // The word that opened it may have been drawn again meanwhile: its twin by id gets the focus.
      const back = opener?.isConnected ? opener
        : (openerId ? within.ownerDocument.getElementById(openerId) : null);
      if (refocus) back?.focus({ preventScroll: true });
      onClose?.();
    },
    /** Escape answers `no`; true when the key was this ask's. */
    key(event) {
      if (closed || event.key !== 'Escape' || event.isComposing) return false;
      event.preventDefault();
      event.stopPropagation();
      finish(no);
      return true;
    },
  };
  asks.set(scope, handle);
  if (scope !== PAGE) scope.addEventListener(MODAL_LEFT, gone);
  signal?.addEventListener('abort', () => handle.close(false), { once: true, signal: stop.signal });
  return handle;
}

/** Turn the words of `box` off while an answer is on its way, back on if it was refused. */
function busy(box, on) {
  for (const el of box.querySelectorAll('button, input')) el.disabled = on;
}

/**
 * Give an ask the keys of its own box, for a caller that owns no keyboard (a theme's page, outside
 * a modal): Escape inside the box answers `no`. The listener goes with the box. Inside a popup
 * do not call it: hand the key from the modal's `onKey` to `ask.key(event)`.
 */
export function listen(ask) {
  ask.box.addEventListener('keydown', ask.key);
}

/**
 * A form for a name inside `within`: a ruled input (`maxlength` 40, required) holding `value`,
 * then `save` or `rename` (`kind`) and `cancel`. The field carries `aria-invalid`: `false` as it
 * opens, `true` once a send found it empty, and at each typing what the browser finds.
 * `submit(text)` sends it and resolves the answer: closed on `ok`, open as it was (the text
 * kept) otherwise. `opener` gets the focus back and `onClose` hears every way it ends.
 * `classes` and `words` dress it (see the top); `between` is the markup between the two buttons
 * (a space by default) and `scroll` brings the form into view.
 * Returns `{ close, key, box }`.
 */
export function askForm({
  within, kind, value = '', submit, signal, opener, onClose,
  classes = FRAME_CLASSES, words, between = ' ', scroll = false,
}) {
  const label = kind === 'rename' ? words.newName : words.presetName;
  const yes = words.answers[kind];
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
    within, signal, onClose, opener, scroll, className: classes.box,
    html: `<form class="${esc(classes.form)}"><input class="${esc(classes.input)}" type="text"`
      + ` maxlength="40" required autocomplete="off" aria-label="${esc(label)}"`
      + ` aria-invalid="false" value="${esc(value)}"> `
      + `<button type="submit" class="${esc(classes.button)}" data-answer="${esc(kind)}">`
      + `${esc(yes)}</button>${between}`
      + `<button type="button" class="${esc(classes.button)}" data-answer="cancel">`
      + `${esc(words.answers.cancel)}</button></form>`,
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
  // The field is said to be invalid from the moment the browser shows it is (a send with no
  // name), not as it opens empty; from the first typing on it says what the browser finds.
  input.addEventListener('invalid', () => input.setAttribute('aria-invalid', 'true'));
  input.addEventListener('input', () => {
    input.setAttribute('aria-invalid', String(!input.validity.valid));
  });
  input.focus({ preventScroll: true });
  input.select();
  return handle;
}

/**
 * A question inside `within`: `text`, then the answer `yes` (`delete`, `save`, ...) and `no`
 * (`keep`). `go()` is called only when `yes` is given, and resolves the answer: closed on `ok`,
 * open otherwise. `no` and Escape close it and send nothing. `no` has the focus, and both
 * answers are described by the question (`aria-describedby`), so a screen reader reads the
 * question with the answer the focus lands on. A `yes` of
 * `delete` or `remove` carries `classes.danger`. `classes`, `words`, `between` and `scroll` as for
 * the form. Returns `{ close, key, box }`.
 */
export function askQuestion({
  within, text, yes, no = 'keep', go, signal, opener, onClose,
  classes = FRAME_CLASSES, words, between = ' ', scroll = false,
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
  const danger = classes.danger && (yes === 'delete' || yes === 'remove')
    ? ` ${classes.danger}` : '';
  const id = `ask-question-${++questions}`;
  const described = attr('aria-describedby', id);
  handle = open({
    within, signal, onClose, opener, scroll, className: classes.box,
    html: `<p class="${esc(classes.question)}"${attr('id', id)}>${esc(text)}</p>`
      + `<p${attr('class', classes.set)}>`
      + `<button type="button" class="${esc(classes.button + danger)}" `
      + `data-answer="${esc(yes)}"${described}>${esc(words.answers[yes])}</button>${between}`
      + `<button type="button" class="${esc(classes.button)}" data-answer="${esc(no)}"`
      + `${described}>${esc(words.answers[no])}</button></p>`,
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
  return handle;
}

/** Close the form or question that is open on the page, if any, without giving the focus back
 * (the place changed under it); with `within`, the one open in the modal `within` sits in. */
export function closeAsk(within = null) {
  asks.get(scopeOf(within))?.close(false);
}

/** How many scopes have an ask open: none on a page at rest. */
export const openAsks = () => asks.size;
