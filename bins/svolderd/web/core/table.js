/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The points' table of the computer's EQ, for every theme: the rows beyond the ones in view
// counted under it, a row scrolled into view (the table scrolls, not the page), the row in hand
// lit, the scroll kept across a draw, and the flow of a value typed in a cell. The elements are the
// theme's own, found by the functions it gives; nothing here names a class of a theme's page except
// the two the table's markup shares (`long`, `end`, which a theme may rename).

import { ROWS_SHOWN } from './computer.js';

/** Whether a table of `count` points is longer than the rows shown (it scrolls, and counts the
 * rest). */
export const isLong = (count) => count > ROWS_SHOWN;

/**
 * The table over the page's elements: `box()` is the scrolling element (its children are the
 * rows), `note()` the words under it, `row(n)` the row of point `n` (1-based), `lit(n)` lights
 * that row and whatever else shows the point in hand, `moreWords(hidden)` says how many rows are
 * out of view. `rowTop(row, box)` is a row's top within the box. `computer` keeps the point in
 * hand.
 *
 * Where the rows are not the box's children (a `table > tbody > tr`), `rows(box)` lists them.
 * The rows out of view are counted from the box's height and the mean row height; with `exact`
 * they are the rows whose bottom lies below the view, whatever their heights. `fadeClass` is put
 * on the box while any row lies below (the opposite sense of `endClass`), and `onMore(hidden)`
 * hears the count at each update (a theme that shows or hides its own words with it).
 */
export function createTable({
  computer, box, note, row, lit = () => {}, moreWords, rowTop = (el) => el.offsetTop,
  longClass = 'long', endClass = 'end', rows = (area) => area.children, exact = false,
  fadeClass = null, onMore = () => {},
}) {
  let kept = 0;

  /** The words under the table: the rows beyond the ones in view. */
  function updateMore() {
    const area = box();
    const more = note();
    if (!area) return;
    const listed = [...rows(area)];
    const edge = area.scrollTop + area.clientHeight + 1;
    let hidden;
    if (exact) {
      hidden = listed.filter((el) => rowTop(el, area) + el.offsetHeight > edge).length;
    } else {
      const rowHeight = area.scrollHeight / Math.max(listed.length, 1);
      hidden = Math.ceil((area.scrollHeight - area.scrollTop - area.clientHeight - 1) / rowHeight);
    }
    if (more) {
      more.textContent = area.classList.contains(longClass) && hidden > 0 ? moreWords(hidden) : '';
    }
    area.classList.toggle(endClass, hidden <= 0);
    if (fadeClass) area.classList.toggle(fadeClass, hidden > 0);
    onMore(hidden);
  }

  /** Scroll the table, not the page, so that row `n` is in view. */
  function reveal(n) {
    const area = box();
    const el = area ? row(n) : null;
    if (!area || !el) return;
    const top = rowTop(el, area);
    if (top < area.scrollTop) area.scrollTop = top;
    else if (top + el.offsetHeight > area.scrollTop + area.clientHeight) {
      area.scrollTop = top + el.offsetHeight - area.clientHeight;
    }
    updateMore();
  }

  return {
    updateMore,
    reveal,
    /** Put row `n` in hand: lit, and scrolled into view unless `scroll` is false. */
    pick(n, { scroll = true } = {}) {
      computer.pick(n);
      lit(n);
      if (scroll) reveal(n);
    },
    /** The table scrolled: its place is kept for the next draw. */
    scrolled() {
      const area = box();
      if (area) kept = area.scrollTop;
      updateMore();
    },
    /** The scroll as it was, to be called as the part's swap begins (a draw is about to replace
     * the table). */
    keep() {
      kept = box()?.scrollTop ?? 0;
    },
    /** The table is drawn again: it keeps its scroll and shows the point in hand. */
    restore() {
      const area = box();
      if (!area) return;
      area.scrollTop = kept;
      const n = computer.picked();
      if (n !== null) reveal(n);
      updateMore();
    },
    /** The side was flipped or another device shown: the scroll starts over. */
    reset() {
      kept = 0;
    },
  };
}

/**
 * The flow of a value typed in a cell's `input` (point `n`'s `param`; `shown` is the cell's text
 * before it was edited): Enter writes it (`cell(n, param, typed, shown)` is
 * `actions.computer.cell`, which says what is wrong with it and writes nothing for no change),
 * Escape or leaving the field puts the old text back. `restore()` puts the text back and `after()`
 * runs once the flow is over, whichever way it ended (a theme draws what waited). Returns
 * `{ close() }`, which ends the flow the way Escape does.
 */
export function editCell(input, { n, param, shown, cell, restore = () => {}, after = () => {} }) {
  let over = false;
  const close = () => {
    if (over) return;
    over = true;
    restore();
    after();
  };
  input.addEventListener('keydown', async (event) => {
    if (event.key === 'Escape') {
      event.stopPropagation?.();
      close();
      return;
    }
    if (event.key !== 'Enter' || over) return;
    event.preventDefault?.();
    over = true;
    // read before `restore()`: a theme's restore may put the old text back in the field
    const typed = input.value;
    restore();
    try {
      await cell(n, param, typed, shown);
    } finally {
      after();
    }
  });
  input.addEventListener('blur', close);
  return { close };
}
