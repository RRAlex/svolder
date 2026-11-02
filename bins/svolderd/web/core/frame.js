/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The frame: what the page draws over every theme and never hands to one. The menu's button (over
// the slot a theme anchors it to, else at the top right), the menu it opens (menu.js), the picker
// of themes (picker.js), the popups (toast.js) and the reserved key `T`, which opens the picker
// whatever the theme does with its keys. The button is dressed from the `--frame-*` tokens the
// theme sets; frame.css keeps it shown and on top.
//
// A theme reaches the frame through `core.frame`:
//   toast(text, action, options)  a popup;   dismissToast(key)  take one away
//   anchor(el | null)  put the button over `el` (null: the top right);  realign()  place it
//                      again and bring the menu's rows up to date (after the theme redraws)
//   fade(on)           the button fades after 2 s without a pointer move, touch or key (a theme
//                      that fills the screen); any of those brings it back at once
//   openMenu(), closeMenu(), openPicker(), closePicker()

import { createMenu } from './menu.js';
import { createPicker } from './picker.js';
import { dismissToast, toast } from './toast.js';

/** What a key typed into is not the frame's: it types. */
const TYPING = 'input, textarea, select, [contenteditable]';
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

/** Whether a key press is the frame's `T`: the letter alone (or with Shift), the focus not in a
 * field it would type into. */
export function reservedKey(event) {
  if (event.ctrlKey || event.altKey || event.metaKey) return false;
  if (event.key !== 't' && event.key !== 'T') return false;
  const target = event.target;
  return !(target?.closest?.(TYPING) ?? target?.matches?.(TYPING));
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

  // ---- where the button sits ----
  let slot = null;
  /** The observer of the slot's size, or nothing where the browser has none (or it throws). */
  let watching = null;
  try {
    watching = new window.ResizeObserver(() => place());
  } catch {
    // The button is then placed at a resize, at a font's arrival and by `realign`.
  }

  /** Put the button over the slot, in page coordinates, or at the top right. */
  function place() {
    let left;
    let top;
    if (slot?.isConnected) {
      const box = slot.getBoundingClientRect();
      left = box.left + window.scrollX;
      top = box.top + window.scrollY;
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
  const picker = createPicker({
    document, core, pick, button,
    under: [document.getElementById('theme'), root].filter(Boolean),
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

  /** The wait is over: the button fades, unless the menu or the picker is open. */
  function sleep() {
    if (menu.opened() || picker.opened()) {
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

  // ---- the reserved key ----
  window.addEventListener('keydown', (event) => {
    if (reservedKey(event)) {
      event.preventDefault();
      event.stopImmediatePropagation();
      if (event.repeat) return;
      if (picker.opened()) picker.close();
      else openPicker();
      return;
    }
    if (picker.opened()) picker.key(event);
  }, { capture: true });

  const frame = {
    toast,
    dismissToast,
    fade,

    /** Put the button over `el`, or at the top right with none. */
    anchor(el) {
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
  };

  return {
    frame,

    /** `theme` is on show: the button takes its icon and its words. */
    mounted(theme) {
      button.innerHTML = ICONS[theme?.menuButton?.icon] ?? ICONS.ink;
      button.setAttribute('aria-label', theme?.menuButton?.label ?? 'Menu');
      place();
      menu.rebuild();
    },
  };
}
