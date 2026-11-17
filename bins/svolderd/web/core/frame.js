/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The frame: what the page draws over every theme and never hands to one. The menu's button (over
// the slot a theme anchors it to, else at the top right), the menu it opens (menu.js), the picker
// of themes (picker.js), the toasts (toast.js), the badge on readings from before a link dropped
// (old-readings.js) and the keys: every key goes to the top modal of
// the core's stack while one is open (modal.js); else the reserved key `T` opens the picker
// whatever the theme does with its keys, unless a sheet of the theme's own stands over the page
// (`?` opens the Help popup under the same rule, through the router; no key table binds it),
// and Space plays and pauses the computer's player unless something does. The button is dressed
// from the `--frame-*` tokens the theme sets; frame.css keeps it shown and on top.
//
// A theme reaches the frame through `core.frame`:
//   toast(text, action, options)  a toast;   dismissToast(key)  take one away
//   anchor(el | null, { fixed })  put the button over `el` (null: the top right); `fixed` (a
//                      boolean or a function asked at each placing) places it against the window,
//                      for a slot pinned at the top that never moves as the page scrolls
//   realign()          place it again and bring the menu's rows up to date (after a redraw)
//   fade(on)           the button fades after 2 s without a pointer move, touch or key (a theme
//                      that fills the screen); any of those brings it back at once
//   openMenu(), closeMenu(), openPicker(), closePicker()
//   inMenu(node)       whether a node is inside the menu, its button included
//   menuOwns(event)    whether a key press is the menu's own, not the page's: any key while the focus
//                      is inside the open popover, and on the closed button the four that open it
//   keyTable()         the key table of the theme on show (keys.js), from its mounted instance
//   zoneNames()        that theme's words for the zones of its table (`{ '<zone>': 'The dial' }`),
//                      from its mounted instance's `zones`: the headings of the Help popup
//   lastFocus()        the element inside `#theme` that had the focus last (the menu, its button
//                      and a modal do not count), or `null`: also while it is hidden (inside a
//                      closed sheet) and once the focus left it for nothing (a click on empty
//                      ground); `core.keys.zonesOf(keyTable(), el)` says which widgets' keys
//                      act there
//   covered()          whether something stands over the page and owns its keys: a modal of the
//                      core's stack, the open menu, or a sheet of the theme's own that says so
//                      with `aria-modal="true"`. A theme asks this before it acts on a key or a
//                      wheel of the whole page.

import { HELP, TYPING } from './keys.js';
import { createMenu, inMenu, menuOwns } from './menu.js';
import { createOldBadge } from './old-readings.js';
import { createPicker } from './picker.js';
import { dismissToast, toast } from './toast.js';

/** How long the button stays after the last sign of the person (ms). */
const FADE_MS = 2000;
/** What brings a faded button back. */
const WAKE = ['pointermove', 'pointerdown', 'touchstart', 'keydown'];
/** The button's gap to the page's top and right edge when no theme anchors it (px). */
const INSET = 16;

/** The icons a theme can ask for (`menuButton.icon`): the effects' waves, or a plain circle. */
const ICONS = {
  waves: '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M7 21c-2.5-2.2 2.5-4.4 0-6.6s2.5-4.4 0-6.6M12 21c-2.5-2.2 2.5-4.4 0-6.6s2.5-4.4 0-6.6M17 21c-2.5-2.2 2.5-4.4 0-6.6s2.5-4.4 0-6.6"/></svg>',
  ink: '<svg viewBox="0 0 24 24" aria-hidden="true"><circle class="solid" cx="12" cy="12" r="5"/></svg>',
};

/** Whether a key press is one of `keys` alone (or with Shift), the focus not in a field it would
 * type into. */
function alone(event, keys) {
  if (event.ctrlKey || event.altKey || event.metaKey) return false;
  if (!keys.includes(event.key)) return false;
  const target = event.target;
  return !(target?.closest?.(TYPING) ?? target?.matches?.(TYPING));
}

/** Whether a key press is the frame's `T`: the letter alone (or with Shift), the focus not in a
 * field it would type into. */
export const reservedKey = (event) => alone(event, ['t', 'T']);

/** Whether a key press is the frame's `?`, under the rule of `T`. */
export const helpKey = (event) => alone(event, ['?']);

/** What Space already does when it has the focus: it activates it, or moves it. */
const SPACE_OWNS = 'button, a[href], summary, select, [role=button], [role=slider], [role=switch], '
  + '[role=checkbox], [role=tab], [role=option], [role=menuitem], [role=menuitemradio], [role=menuitemcheckbox], [role=link]';

/** Whether a key press is the frame's Space: the bare key, not a repeat, the focus neither in a field
 * it would type into nor on anything Space itself activates. */
export function spaceKey(event) {
  if (event.key !== ' ' || event.ctrlKey || event.altKey || event.metaKey || event.shiftKey || event.repeat) return false;
  const target = event.target;
  if (target?.closest?.(TYPING) ?? target?.matches?.(TYPING)) return false;
  return !(target?.closest?.(SPACE_OWNS) ?? target?.matches?.(SPACE_OWNS));
}

/** The transport button Space presses for `page`: `pause` while its player plays, else `play`; `null`
 * where there is no player with buttons and a track. */
export function playPauseOf(page) {
  const p = page?.device?.playing ?? page?.host ?? null;
  if (!p?.buttons || !p.track) return null;
  return p.track.status === 'playing' ? 'pause' : 'play';
}

/**
 * The frame over `window`'s page for `core`; `pick(id)` is what a card of the picker asks for.
 * Returns `{ frame, mounted }`: `frame` is what `core.frame` is, `mounted(theme)` tells the frame a
 * theme is on show (its button's look).
 */
export function createFrame({ window, core, pick }) {
  const document = window.document;

  const root = document.createElement('div');
  root.id = 'frame';
  const wrap = document.createElement('span');
  wrap.className = 'fxmenu';
  const button = document.createElement('button');
  button.type = 'button';
  button.className = 'fxbtn';
  button.id = 'fx-button';
  button.setAttribute('aria-label', 'Menu');
  button.setAttribute('aria-haspopup', 'menu');
  button.setAttribute('aria-expanded', 'false');
  button.setAttribute('aria-controls', 'fx-menu');
  button.innerHTML = ICONS.waves;
  const pop = document.createElement('span');
  pop.className = 'fxpop';
  pop.id = 'fx-menu';
  pop.setAttribute('role', 'menu');
  pop.setAttribute('aria-label', 'Menu');
  pop.hidden = true;
  wrap.append(button, pop);
  root.append(wrap);
  // First in the page, so the button is the first stop of the Tab key; its layer is above the
  // theme's by z-index, not by order.
  document.body.prepend(root);
  // The mark on readings from before a link dropped, which every theme shows alike.
  createOldBadge({ document, root, store: core.model });

  // ---- where the button sits ----
  let slot = null;
  /** Whether the button is placed against the window rather than the page (see `anchor`). */
  let fixed = false;
  /** The observer of the slot's size, or nothing where the browser has none (or it throws). */
  let watching = null;
  try {
    watching = new window.ResizeObserver(() => place());
  } catch {
    // The button is then placed at a resize, at a font's arrival and by `realign`.
  }

  /** Put the button over the slot, in page coordinates (in the window's while the slot is fixed),
   * or at the top right. */
  function place() {
    let left;
    let top;
    const still = Boolean(slot?.isConnected && (typeof fixed === 'function' ? fixed() : fixed));
    root.classList.toggle('fixed', still);
    if (slot?.isConnected) {
      const box = slot.getBoundingClientRect();
      left = box.left + (still ? 0 : window.scrollX);
      top = box.top + (still ? 0 : window.scrollY);
    } else {
      left = document.documentElement.clientWidth - button.offsetWidth - INSET;
      top = INSET;
    }
    wrap.style.left = `${left.toFixed(1)}px`;
    wrap.style.top = `${top.toFixed(1)}px`;
  }

  window.addEventListener('resize', () => place(), { passive: true });
  watching?.observe(document.documentElement);
  document.fonts?.ready?.then(() => place());
  place();

  // ---- the menu and the picker ----
  const modal = core.modal;
  const picker = createPicker({
    document, core, pick, modal, toggles: reservedKey,
  });
  const menu = createMenu({
    document, window, core, wrap, button, pop,
    openPicker: () => openPicker(),
  });

  function openPicker() {
    menu.close(false);
    picker.open();
    wake();
  }

  // ---- the fade ----
  let fading = false;
  let timer = 0;
  let listening = null;

  /** Bring the button back, and start its wait again while a theme asks for the fade. */
  function wake() {
    clearTimeout(timer);
    button.classList.remove('faded');
    if (fading) timer = setTimeout(sleep, FADE_MS);
  }

  /** Whether a modal sheet stands over the page: one of the stack, or one a theme draws itself
   * and marks `aria-modal="true"`. */
  const sheet = () => modal.opened() || document.querySelector('[aria-modal="true"]') !== null;
  /** Whether something stands over the page (see `covered` in the header). */
  const covered = () => menu.opened() || sheet();

  // ---- the last focus ----
  /** Whether `el` is in the page and drawn: not hidden, nor inside anything hidden (a closed
   * sheet). The browser says (`checkVisibility`); a page that cannot is read by the `hidden`
   * attribute and an inline `display: none` on the element and what holds it. */
  const drawn = (el) => {
    if (!el?.isConnected) return false;
    if (typeof el.checkVisibility === 'function') {
      return el.checkVisibility({ visibilityProperty: true });
    }
    for (let at = el; at; at = at.parentElement) {
      if (at.hidden || at.style?.display === 'none') return false;
    }
    return true;
  };
  /** The element of the theme that had the focus last. */
  let focused = null;
  // The focus moving into the menu, its button or a modal is not the theme's: it is left as it is.
  document.addEventListener('focusin', (event) => {
    if (document.getElementById('theme')?.contains(event.target)) focused = event.target;
  }, { capture: true });
  // The focus leaving that element for nothing (a click on empty ground) is the person leaving
  // the widget: nothing of the theme has the focus then. A focus that goes somewhere (another
  // control, the menu, a modal) names where, and a window that goes behind another tells the
  // element so while it keeps the focus: neither clears it.
  document.addEventListener('focusout', (event) => {
    // where it goes is an element, or nothing (`null`; the document itself counts as nothing)
    const nowhere = event.relatedTarget?.nodeType !== 1;
    if (event.target === focused && nowhere && document.activeElement !== focused) focused = null;
  }, { capture: true });

  /** The wait is over: the button fades, unless the menu or a modal is open. */
  function sleep() {
    if (menu.opened() || modal.opened()) {
      timer = setTimeout(sleep, FADE_MS);
      return;
    }
    button.classList.add('faded');
  }

  function fade(on) {
    fading = Boolean(on);
    listening?.abort();
    listening = null;
    if (fading) {
      listening = new AbortController();
      for (const kind of WAKE) {
        window.addEventListener(kind, wake, { capture: true, passive: true, signal: listening.signal });
      }
    }
    wake();
  }

  // ---- the keys ----
  // In the capture phase on the window: before any listener of a theme.
  window.addEventListener('keydown', (event) => {
    // the top modal owns every key: nothing opens or acts behind it
    if (modal.key(event)) return;
    // `T` opens the picker, with the menu open too (the menu closes as the picker takes the
    // focus); under a theme's own sheet the key is the sheet's
    if (reservedKey(event) && !sheet()) {
      event.preventDefault();
      event.stopImmediatePropagation();
      if (!event.repeat) openPicker();
      return;
    }
    // `?` opens the Help popup under the same rule, through the router: the key is the person's
    // gesture, so Back leaves the popup. The menu closes as the popup takes the focus. A page
    // with no Help popup lets the key through
    if (helpKey(event) && !sheet() && core.popups?.names().includes(HELP)) {
      event.preventDefault();
      event.stopImmediatePropagation();
      if (!event.repeat) {
        menu.close(false);
        core.router.open(HELP);
        wake();
      }
      return;
    }
    // Space plays and pauses the computer's player, unless something stands over the page
    if (spaceKey(event) && !covered()) {
      const button = playPauseOf(core.model.page());
      if (button) {
        event.preventDefault();
        core.actions.press(button).catch(() => {});
      }
    }
  }, { capture: true });

  const frame = {
    toast,
    dismissToast,
    fade,

    /** Put the button over `el`, or at the top right with none; `fixed` as the header says. */
    anchor(el, { fixed: still = false } = {}) {
      fixed = still;
      if (slot) watching?.unobserve(slot);
      if (slot?.parentElement) watching?.unobserve(slot.parentElement);
      slot = el ?? null;
      if (slot) watching?.observe(slot);
      if (slot?.parentElement) watching?.observe(slot.parentElement);
      place();
    },

    /** Place the button again, and bring the menu's rows up to date. */
    realign() {
      place();
      menu.sync();
    },

    openMenu: () => menu.open(),
    closeMenu: (back = true) => menu.close(back),
    openPicker,
    closePicker: () => picker.close(),
    inMenu,
    menuOwns,
    covered,

    /** The element inside the theme that had the focus last, while it is in the page; `null`
     * before any had it, once it has left the page, once the focus left it for nothing and
     * after a theme switch. */
    lastFocus: () => (drawn(focused) ? focused : null),

    /** The key table of the theme on show: the mounted instance's `keys`, else the theme
     * object's, else an empty one. */
    keyTable: () => core.mounted?.keys ?? core.theme?.keys ?? [],

    /** The words of the theme on show for the zones of its key table, `{ '<zone>': words }`:
     * the mounted instance's `zones`, else the theme object's, else none. */
    zoneNames: () => core.mounted?.zones ?? core.theme?.zones ?? {},
  };

  return {
    frame,

    /** `theme` is on show: the button takes its icon and its words, and what had the focus in
     * the theme before it is forgotten. */
    mounted(theme) {
      focused = null;
      button.innerHTML = ICONS[theme?.menuButton?.icon] ?? ICONS.ink;
      button.setAttribute('aria-label', theme?.menuButton?.label ?? 'Menu');
      place();
      menu.rebuild();
    },
  };
}
