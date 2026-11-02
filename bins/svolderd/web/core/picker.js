/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The picker: a sheet over the page with a card for each theme the page ships, each card's picture
// drawn by its theme (`theme.preview(box)`), the one in use marked. A pick of another theme goes to
// `pick(id)`; Escape, a click outside the sheet or a pick closes it. While it stands it is modal:
// what is under it cannot be reached by the keys or the mouse, and the keys that move in it are
// the sheet's.

/** The words under the title. */
const NOTE = 'Each browser keeps its own choice. Your sections\' order stays with each theme.';

/**
 * The picker of `core`'s themes; `pick(id)` takes a card's theme, `under` lists the elements the
 * sheet covers (they go inert while it is open) and `button` is where the focus goes back to when
 * it was nowhere else. Returns `{ open, close, opened, key }`.
 */
export function createPicker({ document, core, pick, under, button }) {
  let sheet = null;
  let back = null;

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
  const opened = () => sheet !== null;

  function open() {
    if (sheet) return;
    back = document.activeElement;
    const current = core.theme?.id;
    sheet = make('div');
    sheet.id = 'picker';
    sheet.setAttribute('role', 'dialog');
    sheet.setAttribute('aria-modal', 'true');
    sheet.setAttribute('aria-labelledby', 'picker-title');
    const inner = make('div', 'picker-sheet');
    const title = make('h2', '', 'Theme');
    title.id = 'picker-title';
    const grid = make('div', 'picker-grid');
    grid.append(...core.registry.list().map((entry) => card(entry, current)));
    const foot = make('p', 'picker-foot');
    const key = make('kbd', '', 'T');
    const address = make('code', '', '?theme=ember');
    foot.append('Press ', key, ' in any theme to open this or add ', address,
      ' to the address');
    inner.append(title, make('p', 'picker-note', NOTE), grid, foot);
    sheet.append(inner);
    sheet.addEventListener('click', (event) => {
      const chosen = event.target.closest?.('.tcard');
      if (chosen) {
        close();
        if (chosen.dataset.theme !== core.theme?.id) pick(chosen.dataset.theme);
      } else if (event.target === sheet) {
        close();
      }
    });
    document.body.append(sheet);
    for (const el of under) el.inert = true;
    (cards().find((el) => el.getAttribute('aria-current') === 'true') ?? cards()[0])?.focus();
  }

  function close() {
    if (!sheet) return;
    sheet.remove();
    sheet = null;
    for (const el of under) el.inert = false;
    const to = back?.isConnected ? back : button;
    back = null;
    to.focus();
  }

  /** A key pressed while the picker is open: Escape closes, the arrows, Home and End and Tab move
   * between the cards; no other key reaches the page under it. */
  function key(event) {
    if (!sheet) return;
    const list = cards();
    const at = list.indexOf(document.activeElement);
    const step = (by) => list[(at + by + list.length) % list.length];
    const to = {
      ArrowRight: step(1),
      ArrowDown: step(1),
      ArrowLeft: step(-1),
      ArrowUp: step(-1),
      Home: list[0],
      End: list[list.length - 1],
      Tab: step(event.shiftKey ? -1 : 1),
    }[event.key];
    if (event.key === 'Escape') {
      event.preventDefault();
      close();
    } else if (to) {
      event.preventDefault();
      to.focus();
    }
    event.stopPropagation();
  }

  return { open, close, opened, key };
}
