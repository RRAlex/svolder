/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The question every theme can ask: a card with the words and two buttons, Yes and No, that
// resolves `true` or `false`. It is for a choice the daemon hands back to the person (a preset
// that two sides would play twice). The focus goes to No, the safe answer; Tab moves between the
// buttons, `y` and `n` answer, Escape is No, a touch or a click on a button answers. The card is
// a modal dialog: the rest of the page is inert while it is open and the focus goes back to where
// it was when the card closes. One question at
// a time: a second one answers the first No. The card is dressed from the `--frame-*` tokens
// (frame.css), so each theme's own frame colours reach it.

/** Numbers each card's words, which name the card. */
let counter = 0;

/**
 * The asker over `document`: `ask(question, { yes, no })` shows the card and resolves the
 * person's answer; `ask.close()` answers an open question No; `ask.opened()` says whether one is
 * open.
 */
export function createAsk({ document }) {
  let current = null;

  function finish(answer) {
    const open = current;
    if (!open) return;
    current = null;
    open.listening.abort();
    open.box.remove();
    for (const el of open.inerted) el.inert = false;
    try {
      open.before?.focus?.();
    } catch {
      // the element that had the focus is gone
    }
    open.resolve(answer);
  }

  function ask(question, { yes = 'Yes', no = 'No' } = {}) {
    finish(false);
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
      current = { box, resolve, listening, before, inerted };
      document.body.append(box);
      document.addEventListener('keydown', (event) => {
        if (event.ctrlKey || event.altKey || event.metaKey) return;
        const own = box.contains(event.target);
        if (event.key === 'Escape') {
          event.preventDefault();
          event.stopImmediatePropagation();
          finish(false);
        } else if (own && (event.key === 'y' || event.key === 'Y')) {
          event.preventDefault();
          finish(true);
        } else if (own && (event.key === 'n' || event.key === 'N')) {
          event.preventDefault();
          finish(false);
        } else if (event.key === 'Tab') {
          event.preventDefault();
          const buttons = [yesButton, noButton];
          const at = buttons.indexOf(document.activeElement);
          const step = event.shiftKey ? -1 : 1;
          buttons[(at + step + buttons.length) % buttons.length].focus();
        }
      }, { capture: true, signal: listening.signal });
      noButton.focus();
    });
  }

  ask.close = () => finish(false);
  ask.opened = () => current !== null;
  return ask;
}
