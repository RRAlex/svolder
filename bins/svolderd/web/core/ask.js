/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The question every theme can ask: a card with the words and two buttons, Yes and No, that
// resolves `true` or `false`. It is for a choice the daemon hands back to the person (a preset
// that two sides would play twice). The focus goes to No, the safe answer; Tab moves between the
// buttons, `y` and `n` answer, Escape is No, every other key is stopped before the theme's own
// handlers (the card owns the keyboard), a touch or a click on a button answers. The card is
// a modal dialog: the rest of the page is inert while it is open and the focus goes back to where
// it was when the card closes (or to the element `returnTo` names, found again by id when the
// card closes and again, for a short while, each time the page changes after the answer, for a
// control that is drawn afresh meanwhile). One question at a time: a second one answers the first
// No. The card is dressed from the `--frame-*` tokens (frame.css), so each
// theme's own frame colours reach it.

/** How long (ms) after an answer the focus is given back to a control that is drawn again. */
const RESTORE_MS = 1500;

/** Numbers each card's words, which name the card. */
let counter = 0;

/**
 * The asker over `document`: `ask(question, { yes, no, returnTo })` shows the card and resolves the
 * person's answer; `ask.close()` answers an open question No; `ask.opened()` says whether one is
 * open.
 */
export function createAsk({ document }) {
  let current = null;
  /** Ends the watch for the control to give the focus back to, when one is on. */
  let stopWatching = () => {};

  function finish(answer) {
    const open = current;
    if (!open) return;
    current = null;
    open.listening.abort();
    open.box.remove();
    for (const el of open.inerted) el.inert = false;
    const back = () => (open.returnTo ? document.getElementById(open.returnTo) : null);
    const focusBack = () => {
      try {
        (back() ?? open.before)?.focus?.();
      } catch {
        // the element that had the focus is gone
      }
    };
    focusBack();
    open.resolve(answer);
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
      box.setAttribute('role', 'dialog');
      box.setAttribute('aria-modal', 'true');
      box.setAttribute('aria-labelledby', id);
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
      const yesButton = button(yes, 'yes', true);
      const noButton = button(no, 'no', false);
      row.append(yesButton, noButton);
      box.append(words, row);
      const listening = new AbortController();
      const before = document.activeElement;
      // the page behind the card is out of reach while it is open (the popups stay readable)
      const inerted = [...document.body.children].filter((el) => !el.inert && el.id !== 'toasts');
      for (const el of inerted) el.inert = true;
      current = { box, resolve, listening, before, inerted, returnTo };
      document.body.append(box);
      document.addEventListener('keydown', (event) => {
        // The card owns the keyboard: a key it does not take is stopped here, before the theme's
        // own handlers (a volume or a mute written behind a modal). The browser's own action for
        // it (Enter or Space on a focused button) is left to run.
        if (event.ctrlKey || event.altKey || event.metaKey) {
          event.stopImmediatePropagation();
          return;
        }
        const own = box.contains(event.target);
        if (event.key === 'Escape') {
          event.preventDefault();
          event.stopImmediatePropagation();
          finish(false);
        } else if (own && (event.key === 'y' || event.key === 'Y')) {
          event.preventDefault();
          event.stopImmediatePropagation();
          finish(true);
        } else if (own && (event.key === 'n' || event.key === 'N')) {
          event.preventDefault();
          event.stopImmediatePropagation();
          finish(false);
        } else if (event.key === 'Tab') {
          event.preventDefault();
          event.stopImmediatePropagation();
          const buttons = [yesButton, noButton];
          const at = buttons.indexOf(document.activeElement);
          const step = event.shiftKey ? -1 : 1;
          buttons[(at + step + buttons.length) % buttons.length].focus();
        } else {
          event.stopImmediatePropagation();
        }
      }, { capture: true, signal: listening.signal });
      noButton.focus();
    });
  }

  ask.close = () => finish(false);
  ask.opened = () => current !== null;
  return ask;
}
