/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The list: the options of a chip that opens one (a preset, a long choice) drawn as a
// small panel over or under the chip that opened it. One list is open at a time. The arrows, Home
// and End move through it, Enter or a tap chooses, Escape closes it and gives the focus back to the
// chip, and a press outside closes it. Its place is set through the style object (the page's
// content policy allows no `style` attribute).

/** The list open now, or nothing. */
let current = null;

/** How far under the chip the list starts, in px. */
const GAP = 4;

/**
 * Open a list under `opener` (a button): `options` is `[{ value, text, on }]`, `choose(value)` hears
 * the choice (the list closes first), `label` names the list for a screen reader, `signal` closes
 * it when the page goes and `onClose()` hears every way it ends. Returns `{ close() }`.
 */
export function openList({
  opener, label, options, choose, signal, onClose = () => {},
}) {
  if (current) current.close();
  const doc = opener.ownerDocument;
  const sheet = opener.closest('.nd') ?? doc.body;
  const list = doc.createElement('div');
  list.className = 'nd-list';
  list.setAttribute('role', 'listbox');
  list.setAttribute('aria-label', label);
  const buttons = options.map((option) => {
    const item = doc.createElement('button');
    item.type = 'button';
    item.className = `nd-opt${option.on ? ' on' : ''}`;
    item.setAttribute('role', 'option');
    item.setAttribute('aria-selected', option.on ? 'true' : 'false');
    item.dataset.value = String(option.value);
    item.textContent = option.text;
    list.append(item);
    return item;
  });
  sheet.append(list);

  // Over the chip when it stands in the lower half of the page (the dash), else under it; kept
  // inside the page.
  const box = sheet.getBoundingClientRect();
  const at = opener.getBoundingClientRect();
  const left = Math.max(0, Math.min(at.left - box.left, box.width - list.offsetWidth));
  const above = at.top + at.height / 2 > box.top + box.height / 2;
  const top = above ? at.top - box.top - list.offsetHeight - GAP : at.bottom - box.top + GAP;
  list.style.left = `${left.toFixed(1)}px`;
  list.style.top = `${Math.max(0, top).toFixed(1)}px`;
  opener.setAttribute('aria-expanded', 'true');

  const stop = new AbortController();
  let closed = false;
  const handle = {
    opener,
    close(refocus = true) {
      if (closed) return;
      closed = true;
      stop.abort();
      list.remove();
      opener.setAttribute('aria-expanded', 'false');
      if (current === handle) current = null;
      if (refocus && opener.isConnected) opener.focus({ preventScroll: true });
      onClose();
    },
  };
  current = handle;

  const moveTo = (i) => buttons[(i + buttons.length) % buttons.length]?.focus();
  list.addEventListener('keydown', (event) => {
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
    }
  }, { signal: stop.signal });
  list.addEventListener('click', (event) => {
    const item = event.target.closest?.('.nd-opt');
    if (!item) return;
    const value = options[buttons.indexOf(item)]?.value;
    handle.close();
    choose(value);
  }, { signal: stop.signal });
  // A press anywhere but on the list or its chip closes it; the chip itself toggles it.
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

/** Close the list of `opener` if it is the one open: says whether it was (the chip toggles). */
export function closeOf(opener) {
  if (current?.opener !== opener) return false;
  current.close();
  return true;
}

/** Close the list that is open, if any, without giving the focus back (the page changed under it). */
export function closeList() {
  current?.close(false);
}
