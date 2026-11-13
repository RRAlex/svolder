/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The question every theme can ask: a card with the words and two buttons, Yes and No, that
// resolves `true` or `false`. It is for a choice the daemon hands back to the person (a preset
// that two sides would play twice). The card is a modal of the core's stack (modal.js): the rest
// of the page is inert while it is open, it owns the keyboard (no key reaches a theme's own
// handlers), Tab moves between its buttons and Escape is No. The focus goes to No, the safe
// answer; `y` and `n` answer, a touch or a click on a button answers, and a card closed with the
// stack (a theme switch, a device switch) is answered No. When the card closes the focus goes back
// to where it was (or to the element `returnTo` names, found again by id when the card closes and
// again, for a short while, each time the page changes after the answer, for a control that is
// drawn afresh meanwhile). One question at a time: a second one answers the first No. The card is
// dressed from the `--frame-*` tokens (frame.css), so each theme's own frame colours reach it.

/** How long (ms) after an answer the focus is given back to a control that is drawn again. */
const RESTORE_MS = 1500;

/** Numbers each card's words, which name the card. */
let counter = 0;

/**
 * The asker over `document`, on the stack `modal`: `ask(question, { yes, no, returnTo })` shows the
 * card and resolves the person's answer; `ask.close()` answers an open question No; `ask.opened()`
 * says whether one is open.
 */
export function createAsk({ document, modal }) {
  let current = null;
  /** Ends the watch for the control to give the focus back to, when one is on. */
  let stopWatching = () => {};

  /** Answer the open question: its card leaves the stack, which tells `closed`. */
  function finish(answer) {
    if (!current) return;
    current.answer = answer;
    current.standing.close();
  }

  /** The card `open` has left the stack (the focus is back already): its answer is given. */
  function closed(open) {
    if (current === open) current = null;
    const back = () => (open.returnTo ? document.getElementById(open.returnTo) : null);
    open.resolve(open.answer);
    // The caller's own work follows the answer (a write, the model read again, a control drawn
    // afresh): the focus is given back to the control found again by id for a short while, each
    // time the page changes, until it has landed or the window is over; a focus the person has put
    // somewhere else is left, and a press or a key of their own ends the watch (a click on blank
    // ground leaves the focus on the body on purpose).
    if (open.returnTo) {
      const view = document.defaultView ?? globalThis;
      const lost = () => {
        const at = document.activeElement;
        return !at || at === document.body || at.isConnected === false;
      };
      const again = () => {
        if (!lost()) return;
        const el = back();
        if (!el) return;
        try {
          el.focus?.();
        } catch {
          // the control is gone
        }
        if (document.activeElement === el) stopWatching();
      };
      queueMicrotask(again);
      if (view.MutationObserver) {
        const watch = new view.MutationObserver(again);
        watch.observe(document.body, { childList: true, subtree: true });
        const timer = view.setTimeout(() => stopWatching(), RESTORE_MS);
        const handOff = () => stopWatching();
        document.addEventListener('pointerdown', handOff, true);
        document.addEventListener('keydown', handOff, true);
        stopWatching = () => {
          watch.disconnect();
          view.clearTimeout(timer);
          document.removeEventListener?.('pointerdown', handOff, true);
          document.removeEventListener?.('keydown', handOff, true);
          stopWatching = () => {};
        };
      }
    }
  }

  function ask(question, { yes = 'Yes', no = 'No', returnTo = null } = {}) {
    finish(false);
    stopWatching();
    return new Promise((resolve) => {
      const id = `ask-${++counter}`;
      const box = document.createElement('div');
      box.className = 'ask';
      const words = document.createElement('p');
      words.id = id;
      words.className = 'ask-words';
      words.textContent = question;
      const row = document.createElement('div');
      row.className = 'ask-buttons';
      const button = (label, kind, answer) => {
        const el = document.createElement('button');
        el.type = 'button';
        el.className = `ask-button ask-${kind}`;
        el.textContent = label;
        el.addEventListener('click', () => finish(answer));
        return el;
      };
      const noButton = button(no, 'no', false);
      row.append(button(yes, 'yes', true), noButton);
      box.append(words, row);
      // a card closed any other way (Escape, the stack emptied) is a No
      const open = { resolve, returnTo, answer: false, standing: null };
      current = open;
      open.standing = modal.open({
        el: box,
        labelledBy: id,
        focus: noButton,
        returnTo,
        onClose: () => closed(open),
        // `y` and `n` answer, from inside the card and without a modifier
        onKey(event) {
          if (event.ctrlKey || event.altKey || event.metaKey) return false;
          const letter = event.key.toLowerCase();
          if ((letter !== 'y' && letter !== 'n') || !box.contains(event.target)) return false;
          event.preventDefault();
          finish(letter === 'y');
          return true;
        },
      });
    });
  }

  ask.close = () => finish(false);
  ask.opened = () => current !== null;
  return ask;
}
