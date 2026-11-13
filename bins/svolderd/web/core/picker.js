/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The picker: a sheet over the page with a card for each theme the page ships, each card's picture
// drawn by its theme (`theme.preview(box)`), the one in use marked. A pick of another theme goes to
// `pick(id)`; Escape, the key that opened it, a click outside the sheet, the close button at its
// top right or a pick closes it. It is a modal of the core's stack (modal.js): what is under it
// cannot be reached by the keys or the mouse, and the keys that move in it are the sheet's.

/** The words under the title. */
const NOTE = 'Each browser keeps its own choice. Your sections\' order stays with each theme.';

/**
 * The picker of `core`'s themes on the stack `modal`; `pick(id)` takes a card's theme and
 * `toggles(event)` says whether a key is the one that opens the picker (it closes it too).
 * Returns `{ open, close, opened }`.
 */
export function createPicker({ document, core, pick, modal, toggles }) {
  let sheet = null;
  /** The sheet's place on the stack while it is open. */
  let standing = null;

  /** An element of `tag` with `name` as its class. */
  function make(tag, name, text) {
    const el = document.createElement(tag);
    if (name) el.className = name;
    if (text !== undefined) el.textContent = text;
    return el;
  }

  /** The theme's picture in `box`, if it has one; a theme that fails to draw leaves the box empty. */
  function paint(theme, box) {
    try {
      theme.preview?.(box);
    } catch (error) {
      console.warn(`svolder: the preview of ${theme.id} threw`, error);
    }
  }

  /** The card for the listed theme `entry`. */
  function card(entry, current) {
    const el = make('button', 'tcard');
    el.type = 'button';
    el.dataset.theme = entry.id;
    if (entry.id === current) el.setAttribute('aria-current', 'true');
    const box = make('span', 'tprev');
    el.append(box, make('b', '', entry.label), make('small', '', entry.blurb ?? ''));
    const loaded = core.registry.loaded(entry.id);
    if (loaded) paint(loaded, box);
    else {
      core.registry.load(entry.id).then((theme) => paint(theme, box)).catch(() => {});
    }
    return el;
  }

  const cards = () => (sheet ? [...sheet.querySelectorAll('.tcard')] : []);
  /** What the keys move over: the cards, then the close button. */
  const reach = () => (sheet ? [...cards(), sheet.querySelector('.pclose')] : []);
  const opened = () => sheet !== null;

  function open() {
    if (sheet) return;
    const current = core.theme?.id;
    sheet = make('div');
    sheet.id = 'picker';
    const inner = make('div', 'picker-sheet');
    const title = make('h2', '', 'Theme');
    title.id = 'picker-title';
    const grid = make('div', 'picker-grid');
    const x = make('button', 'pclose');
    x.type = 'button';
    x.setAttribute('aria-label', 'Close');
    x.innerHTML = '<svg viewBox="0 0 16 16" width="16" height="16" aria-hidden="true" focusable="false">'
      + '<path d="M3.5 3.5l9 9M12.5 3.5l-9 9" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/></svg>';
    grid.append(...core.registry.list().map((entry) => card(entry, current)));
    const foot = make('p', 'picker-foot');
    const letter = make('kbd', '', 'T');
    const address = make('code', '', '?theme=ember');
    foot.append('Press ', letter, ' in any theme to open this or add ', address,
      ' to the address');
    inner.append(x, title, make('p', 'picker-note', NOTE), grid, foot);
    sheet.append(inner);
    sheet.addEventListener('click', (event) => {
      const chosen = event.target.closest?.('.tcard');
      if (event.target.closest?.('.pclose')) {
        close();
      } else if (chosen) {
        close();
        if (chosen.dataset.theme !== core.theme?.id) pick(chosen.dataset.theme);
      } else if (event.target === sheet) {
        close();
      }
    });
    standing = modal.open({
      el: sheet,
      labelledBy: 'picker-title',
      focus: cards().find((el) => el.getAttribute('aria-current') === 'true') ?? cards()[0],
      onKey: key,
      onClose: () => {
        sheet = null;
        standing = null;
      },
    });
  }

  function close() {
    standing?.close();
  }

  /** A key pressed while the picker is open, `true` when it is the picker's: the key that opened it
   * closes it, the arrows, Home and End and Tab move between the cards. Escape is the stack's. */
  function key(event) {
    if (toggles(event)) {
      event.preventDefault();
      // the key still held from the press that opened the picker is not a second press
      if (!event.repeat) close();
      return true;
    }
    const list = reach();
    const at = list.indexOf(document.activeElement);
    const step = (by) => list[(at + by + list.length) % list.length];
    const to = {
      ArrowRight: step(1),
      ArrowDown: step(1),
      ArrowLeft: step(-1),
      ArrowUp: step(-1),
      Home: list[0],
      End: list[list.length - 2],
      Tab: step(event.shiftKey ? -1 : 1),
    }[event.key];
    if (!to) return false;
    event.preventDefault();
    to.focus();
    return true;
  }

  return { open, close, opened };
}
