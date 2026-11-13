/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The modal layer: one stack of dialogs over the page. The picker of themes, the question and a
// popup are its clients; the one opened last is the top, and only the top can be reached.
//
//   open({ el, labelledBy, focus, returnTo, onClose, onKey, stays })   put `el` on top; returns
//                    `{ close(), opened() }` for that modal. An `el` that is on the stack already
//                    stays where it is, with the options it was opened with. `stays: true` is a
//                    modal that is not about the device on show: it stands through a change of it
//   close()          close the top
//   closeAll()       close every one, the top first (a theme switch)
//   closeAll({ device: true })   another device is on show: close every one that does not stay,
//                    the top first
//   opened()         whether any is open
//   key(event)       the frame hands every keydown here first (frame.js, in the capture phase on
//                    the window): `true` when a modal is open, and the key is then the top's
//
// While a modal is open:
// - `el` is a child of the body, with `role="dialog"`, `aria-modal="true"` and, with `labelledBy`
//   (the id of its title), `aria-labelledby`; it leaves the page when it closes, and one that
//   something else took out of the page is off the stack the next time the stack is asked or a
//   key is pressed. As it leaves, `el` hears `MODAL_LEFT`: an inline ask or a list open inside it
//   closes (inline-ask.js, list.js keep one open per modal, found with `modalOf`: by the
//   element the stack was given, so also before it is in the page and after it has left);
// - every other child of the body but the toasts (`#toasts`) is `inert`, the modals under the top
//   included;
// - the top owns the keyboard: the key stops at the frame's listener, so no other listener hears
//   it, of a theme, of the frame or inside the modal itself (`T` and `?` do nothing). A part that
//   lives in a modal therefore attaches no key listener of its own: it exports `key(event)` and
//   the modal's `onKey(event)` hands it the key. `onKey` is handed each key first and returns
//   `true` for one it took (an Escape that shuts a list inside the modal, an Enter that sends its
//   form). Of the rest, Escape closes the top and Tab at the top's last control (Shift+Tab at its
//   first, or either from outside it) goes round to the other end. The ends are the first and the
//   last control a person can reach: one that is disabled, in something `hidden` or `inert`, or
//   not rendered (`display: none` in a sheet, as a layout's hidden pane is) is no end;
// - the browser's own behaviour inside the top is never prevented by the stack: a letter typed
//   into a field, the caret's keys, Enter and Space on the button that has the focus, Tab between
//   the top's controls, a native select. A key of a text composition is left whole to the field:
//   neither `onKey` nor Escape nor Tab acts on it;
// - the focus opens on `focus` (an element, or a function that gives one, asked once `el` is in
//   the page), else on the top's first control, else on `el`; when the modal
//   closes it goes to `returnTo` (an element, or an id that is looked up then, so a control drawn
//   again meanwhile is found), else to what had it when the modal opened, or when that has left
//   the page to the element that has its id now (a control a theme drew again while the modal
//   stood); when no `returnTo` was named and the opener is gone with no such twin, or cannot take
//   the focus any more (a row of a menu that has closed), to the layer's `fallback`. A client
//   that names a `returnTo` which is not there yet keeps watch for it itself (ask.js does).
// `onClose()` runs once, however the modal closed, after the focus has moved; one that throws is
// said in the console and stops nothing.

/** The event a modal's element hears as it leaves the stack. What is open inside it and is no
 * modal itself (an inline ask, a list) closes on it. */
export const MODAL_LEFT = 'svolder-modal-left';

/** The elements a stack was given: each is a scope of its own from then on, in the page or out of
 * it (weakly held, never taken out). */
const claimed = new WeakSet();

/** Say ahead that `el` will be a modal of the stack: what is drawn in it before the stack puts it
 * in the page (a popup's `show(page)`) is then filed under it, not under the page. The router does
 * this for every popup it registers; `open` does it for every other modal. */
export function claim(el) {
  claimed.add(el);
}

/** The modal of the stack that `el` sits in, or `null` for the page: the nearest of `el` and its
 * ancestors that a stack was given (`open`) or promised (`claim`), whether it is in the page yet,
 * still, or no more. An element no stack was given is the page's: a theme's section that is not
 * in the page, a sheet a theme marks modal inside its own page. */
export function modalOf(el) {
  for (let at = el ?? null; at; at = at.parentElement) {
    if (claimed.has(at)) return at;
  }
  return null;
}

/** What Tab can reach in a modal. */
export const REACH = 'a[href], button:not([disabled]), input:not([disabled]), '
  + 'select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"]), '
  + 'details > summary:first-of-type';

/** Where the focus goes when a modal closes, or `null` for nowhere: `returnTo` (an element still in
 * the page, or the element that has that id now), else `opener` while it is in the page, else the
 * element that has the id the opener had (`openerId`; by default the id it has now); `fallback`
 * only when no `returnTo` was named. */
export function returnTarget({
  document, returnTo = null, opener = null, openerId = opener?.id ?? '', fallback = null,
}) {
  const live = (el) => (el?.isConnected ? el : null);
  const from = live(opener) ?? (openerId ? document.getElementById(openerId) : null);
  if (returnTo === null) return from ?? live(fallback);
  const named = typeof returnTo === 'string' ? document.getElementById(returnTo) : live(returnTo);
  return named ?? from;
}

/**
 * The stack of modals over `document`; `fallback()` gives the element that takes the focus when a
 * modal's opener is gone (the frame's button). Returns `{ open, close, closeAll, opened, key }`.
 */
export function createModals({ document, fallback = () => null }) {
  /** The open modals, the top last: `{ el, opener, openerId, returnTo, onClose, onKey, stays }`. */
  const stack = [];
  /** The children of the body this layer made inert. */
  const covered = new Set();

  /** The controls of `el` that Tab reaches, in the page's order: those that are rendered (where
   * the browser cannot say, a control is taken to be). */
  const reach = (el) => [...el.querySelectorAll(REACH)]
    .filter((one) => !one.closest('[hidden], [inert]')
      && (one.checkVisibility?.({ visibilityProperty: true }) ?? true));

  /** Give `el` the focus; `false` when it did not take it. */
  function give(el) {
    try {
      el?.focus?.();
    } catch {
      // it is gone
    }
    return document.activeElement === el;
  }

  /** Bring `inert` in step with the stack: everything in the body but the top and the toasts. */
  function cover() {
    const top = stack.at(-1)?.el ?? null;
    for (const el of [...document.body.children]) {
      const under = top !== null && el !== top && el.id !== 'toasts';
      if (under && !el.inert) {
        el.inert = true;
        covered.add(el);
      } else if (!under && covered.has(el)) {
        el.inert = false;
        covered.delete(el);
      }
    }
  }

  /** The modal `entry` leaves the stack, wherever it stands in it. */
  function leave(entry) {
    const at = stack.indexOf(entry);
    if (at < 0) return;
    const above = stack[at + 1];
    stack.splice(at, 1);
    entry.el.dispatchEvent?.(new (document.defaultView ?? globalThis).Event(MODAL_LEFT));
    entry.el.remove();
    if (covered.delete(entry.el)) entry.el.inert = false;
    cover();
    const to = returnTarget({
      document,
      returnTo: entry.returnTo,
      opener: entry.opener,
      openerId: entry.openerId,
      fallback: fallback(),
    });
    if (above) {
      // the modal above was opened from this one: it now returns to where this one would have
      if (entry.el.contains(above.opener)) {
        above.opener = to;
        above.openerId = to?.id ?? '';
      }
    } else if (!give(to) && entry.returnTo === null) {
      // the opener is in the page but takes no focus any more
      give(fallback());
    }
    try {
      entry.onClose?.();
    } catch (error) {
      console.warn('svolder: a modal\'s onClose threw', error);
    }
  }

  /** Take off the stack every modal that something else took out of the page. */
  function prune() {
    for (const entry of stack.filter((one) => !one.el.isConnected)) leave(entry);
  }

  /** Tab in the modal `el`: between its controls it is the browser's own; from its last control
   * (with Shift, its first), or from outside it, the focus goes round to the other end, and stays
   * on `el` when nothing in it takes it. */
  function tab(event, el) {
    const list = reach(el);
    const back = event.shiftKey;
    const at = list.indexOf(document.activeElement);
    if (at >= 0 && at !== (back ? 0 : list.length - 1)) return;
    event.preventDefault();
    for (const next of back ? list.reverse() : list) {
      if (give(next)) return;
    }
    give(el);
  }

  function open({
    el, labelledBy = null, focus = null, returnTo = null, onClose = null, onKey = null,
    stays = false,
  }) {
    const handle = (entry) => ({
      close: () => leave(entry),
      opened: () => {
        prune();
        return stack.includes(entry);
      },
    });
    prune();
    const already = stack.find((one) => one.el === el);
    if (already) return handle(already);
    const opener = document.activeElement;
    // the id it has now: a theme may draw the control again while the modal stands
    const entry = {
      el, opener, openerId: opener?.id ?? '', returnTo, onClose, onKey, stays: stays === true,
    };
    claim(el);
    el.setAttribute('role', 'dialog');
    el.setAttribute('aria-modal', 'true');
    if (labelledBy) el.setAttribute('aria-labelledby', labelledBy);
    document.body.append(el);
    stack.push(entry);
    cover();
    // asked now that `el` is in the page, so it can tell what is rendered
    const first = typeof focus === 'function' ? focus() : focus;
    if (!give(first ?? reach(el)[0])) {
      if (!el.hasAttribute('tabindex')) el.tabIndex = -1;
      give(el);
    }
    return handle(entry);
  }

  function key(event) {
    prune();
    const top = stack.at(-1);
    if (!top) return false;
    event.stopImmediatePropagation();
    // a key of a text composition is the field's alone
    if (event.isComposing) return true;
    if (top.onKey?.(event) === true) return true;
    if (event.ctrlKey || event.altKey || event.metaKey) return true;
    if (event.key === 'Escape') {
      event.preventDefault();
      leave(top);
    } else if (event.key === 'Tab') {
      tab(event, top.el);
    }
    return true;
  }

  return {
    open,
    close: () => leave(stack.at(-1)),
    closeAll({ device = false } = {}) {
      for (const entry of [...stack].reverse()) {
        if (!(device && entry.stays)) leave(entry);
      }
    },
    opened() {
      prune();
      return stack.length > 0;
    },
    key,
  };
}
