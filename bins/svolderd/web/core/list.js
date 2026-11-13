/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The list: the options of a chip or a written word that opens one (a preset, a long choice) drawn
// as a small panel next to the opener. One list is open at a time in each scope: the modal of the
// stack its opener sits in, else the page (a list opened in a popup leaves the page's open and the
// other way round; a modal that closes closes the list inside it). The arrows, Home and End move
// through it, Enter or a tap chooses, Escape closes it and gives the focus back to the opener, and
// a press outside closes it. Its place is set through the style object (the page's content policy
// allows no `style` attribute).
//
// The look is the caller's: `classes` names the panel and its options (a theme passes its own; the
// defaults, `FRAME_LIST_CLASSES`, are dressed by the `--frame-*` tokens in frame.css). `place` is
// `'below'` (under the opener) or `'auto'` (over it when it stands in the lower half of the box,
// else under it). The panel goes into `within`, or the opener's nearest `root` selector, or the
// body, and is kept inside it. A list opened inside a modal needs its panel inside that modal:
// the stack makes everything else in the body inert, so `openList` throws rather than draw a
// panel nobody can reach. With no `within` and no `root`, the panel of a list in a modal goes
// into the nearest place for lists the opener sits in (`LIST_PLACE`).
//
// A place for lists (`data-list-place`; a popup's page pane is one, core/popups/frame.js) is a
// positioned box that may scroll. The panel is placed in what the place scrolls, so it goes with
// its opener when the place is scrolled, and where the place shows it whole when it opens: under
// the opener, else over it, else against the bottom of what the place shows (`place` is not
// read there). The place's sheet keeps the panel no taller than the place. The panel is
// measured at its box's top left corner, not shown, and shown once it is placed.
//
// Keys: the part attaches no key listener of its own. `key(event)` on the returned handle takes
// Escape, the arrows, Home and End and returns true when the key was the list's; whoever owns the
// keyboard hands it in: a popup's modal from its `onKey`, a theme's page through `listen(list)`.
// The module reaches no service of the core and may be imported directly; the one thing it keeps
// is which list is open in each scope.

import { MODAL_LEFT, modalOf } from './modal.js';

/** The classes the popups draw with; each one is dressed by frame.css. */
export const FRAME_LIST_CLASSES = { list: 'fi-list', option: 'fi-opt' };

/** A place for lists: a positioned box, which may scroll, that the panel of a list is put in. */
export const LIST_PLACE = '[data-list-place]';

/** The scope of everything outside a modal of the stack. */
const PAGE = 'page';

/** The list open in each scope that has one, by the modal's element or `PAGE`. */
const lists = new Map();

/** The scope `el` sits in. */
const scopeOf = (el) => modalOf(el) ?? PAGE;

/** How far from the opener the list starts, in px. */
const GAP = 4;

/** Where a panel of `size` (`{ w, h }`) goes in the place for lists `sheet`, next to an opener
 * whose box is `at`: `{ left, top }` in px, in what the place scrolls. */
function placeIn(sheet, at, size) {
  const box = sheet.getBoundingClientRect();
  // the inner edge of the place, and what it shows inside its border and its scrollbar
  const x0 = box.left + sheet.clientLeft;
  const y0 = box.top + sheet.clientTop;
  const room = { w: sheet.clientWidth, h: sheet.clientHeight };
  const x = Math.max(0, Math.min(at.left - x0, room.w - size.w));
  const under = at.bottom - y0 + GAP;
  const over = at.top - y0 - GAP - size.h;
  let y = Math.max(0, room.h - size.h);
  if (under + size.h <= room.h) y = Math.max(0, under);
  else if (over >= 0) y = over;
  return { left: x + sheet.scrollLeft, top: y + sheet.scrollTop };
}

/**
 * Open a list next to `opener` (a button): `options` is `[{ value, text, on }]`, `choose(value)`
 * hears the choice (the list closes first), `label` names the list for a screen reader, `signal`
 * closes it when the page goes and `onClose()` hears every way it ends. `classes`, `place`,
 * `within` and `root` are above. Returns `{ close(refocus), key(event), list, opener }`.
 */
export function openList({
  opener, label, options, choose, signal, onClose = () => {},
  classes = FRAME_LIST_CLASSES, place = 'below', within = null, root = null,
}) {
  const scope = scopeOf(opener);
  const doc = opener.ownerDocument;
  const sheet = within ?? (root ? opener.closest(root) : null)
    ?? (scope === PAGE ? null : opener.closest(LIST_PLACE)) ?? doc.body;
  if (scope !== PAGE && !scope.contains(sheet)) {
    throw new Error('svolder: a list opened inside a modal needs its panel inside that modal: '
      + 'pass `within` or `root` naming an element of the modal, or open it from inside a place '
      + 'for lists (a panel in the body is inert)');
  }
  lists.get(scope)?.close();
  const list = doc.createElement('div');
  list.className = classes.list;
  list.setAttribute('role', 'listbox');
  list.setAttribute('aria-label', label);
  const buttons = options.map((option) => {
    const item = doc.createElement('button');
    item.type = 'button';
    item.className = `${classes.option}${option.on ? ' on' : ''}`;
    item.setAttribute('role', 'option');
    item.setAttribute('aria-selected', option.on ? 'true' : 'false');
    item.dataset.value = String(option.value);
    item.textContent = option.text;
    list.append(item);
    return item;
  });
  // Measured at the box's corner and out of sight: left where the flow puts it (after the box's
  // last child, under its bottom) the panel would give a box that did not scroll a scrollbar
  // for this one layout, and the opener and the box's width would be read with it there.
  list.style.left = '0px';
  list.style.top = '0px';
  list.style.visibility = 'hidden';
  sheet.append(list);

  // Next to the opener, kept inside the box.
  const at = opener.getBoundingClientRect();
  if (sheet.matches(LIST_PLACE)) {
    const to = placeIn(sheet, at, { w: list.offsetWidth, h: list.offsetHeight });
    list.style.left = `${to.left.toFixed(1)}px`;
    list.style.top = `${to.top.toFixed(1)}px`;
  } else {
    const box = sheet.getBoundingClientRect();
    const left = Math.max(0, Math.min(at.left - box.left, box.width - list.offsetWidth));
    list.style.left = `${left.toFixed(1)}px`;
    const above = place === 'auto' && at.top + at.height / 2 > box.top + box.height / 2;
    const top = above ? at.top - box.top - list.offsetHeight - GAP : at.bottom - box.top + GAP;
    list.style.top = `${(place === 'auto' ? Math.max(0, top) : top).toFixed(1)}px`;
  }
  list.style.visibility = '';
  opener.setAttribute('aria-expanded', 'true');

  const stop = new AbortController();
  let closed = false;
  const moveTo = (i) => buttons[(i + buttons.length) % buttons.length]?.focus();
  const gone = () => handle.close(false);
  const handle = {
    opener,
    list,
    close(refocus = true) {
      if (closed) return;
      closed = true;
      stop.abort();
      list.remove();
      opener.setAttribute('aria-expanded', 'false');
      if (lists.get(scope) === handle) lists.delete(scope);
      if (scope !== PAGE) scope.removeEventListener(MODAL_LEFT, gone);
      if (refocus && opener.isConnected) opener.focus({ preventScroll: true });
      onClose();
    },
    /** Escape closes the list; the arrows, Home and End move; true when the key was the list's. */
    key(event) {
      if (closed || event.isComposing) return false;
      const at = buttons.indexOf(doc.activeElement);
      if (event.key === 'Escape') {
        event.preventDefault();
        event.stopPropagation();
        handle.close();
      } else if (event.key === 'ArrowDown') {
        event.preventDefault();
        moveTo(at + 1);
      } else if (event.key === 'ArrowUp') {
        event.preventDefault();
        moveTo(at < 0 ? -1 : at - 1);
      } else if (event.key === 'Home') {
        event.preventDefault();
        moveTo(0);
      } else if (event.key === 'End') {
        event.preventDefault();
        moveTo(buttons.length - 1);
      } else {
        return false;
      }
      return true;
    },
  };
  lists.set(scope, handle);
  if (scope !== PAGE) scope.addEventListener(MODAL_LEFT, gone);

  list.addEventListener('click', (event) => {
    const item = event.target.closest?.(`.${classes.option.split(' ')[0]}`);
    if (!item) return;
    const value = options[buttons.indexOf(item)]?.value;
    handle.close();
    choose(value);
  }, { signal: stop.signal });
  // A press anywhere but on the list or its opener closes it; the opener itself toggles it.
  doc.addEventListener('pointerdown', (event) => {
    if (list.contains(event.target) || opener.contains(event.target)) return;
    handle.close(false);
  }, { capture: true, signal: stop.signal });
  // The focus leaving for another control closes it too.
  list.addEventListener('focusout', (event) => {
    const to = event.relatedTarget;
    if (to && (list.contains(to) || opener.contains(to))) return;
    if (to) handle.close(false);
  }, { signal: stop.signal });
  signal?.addEventListener('abort', () => handle.close(false), { once: true, signal: stop.signal });
  (buttons.find((b) => b.classList.contains('on')) ?? buttons[0])?.focus({ preventScroll: true });
  return handle;
}

/**
 * Give a list the keys of its own panel, for a caller that owns no keyboard (a theme's page,
 * outside a modal). The listener goes with the panel. Inside a popup do not call it: hand the key
 * from the modal's `onKey` to `list.key(event)`.
 */
export function listen(list) {
  list.list.addEventListener('keydown', list.key);
}

/** Close the list of `opener` if it is the one open: says whether it was (the opener toggles). */
export function closeOf(opener) {
  const open = lists.get(scopeOf(opener));
  if (open?.opener !== opener) return false;
  open.close();
  return true;
}

/** Close the list that is open on the page, if any, without giving the focus back (the page
 * changed under it); with `within`, the one open in the modal `within` sits in. */
export function closeList(within = null) {
  lists.get(scopeOf(within))?.close(false);
}

/** How many scopes have a list open: none on a page at rest. */
export const openLists = () => lists.size;
