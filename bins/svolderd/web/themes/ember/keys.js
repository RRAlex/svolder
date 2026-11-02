/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The desktop keys and the key list: `?` opens the list, which closes by itself, and the keys act
// on the knob, the balance slider, the tone mode and the presets, never while a text field has
// the focus, with a modifier, or in the frame's menu, and none that writes while the sections are
// being arranged. `v` changes the visualizer's look and `A` starts Arrange mode.
//
// ember.js calls `installKeys` where its listeners stand; the list's own markup is the shell's
// (draw/shell.js).

import { inMenu, menuOwns } from '../../core/menu.js';
import { ember, mine } from './bind.js';
import { arranging, openArrange } from './arrange.js';
import { cycleLook, hasBand } from './visualizer.js';

// The key list closes by itself 6 s after it opens, and at once on any key, click, scroll, wheel
// or touch (it is a passive list: nothing in it is used, so a click on it closes it too).
const KEYS_MS = 6000;
let keysTimer = 0;
/** Show the key list (or hide it), whose own timer is restarted or cleared. */
function showKeys(list, shown) {
  clearTimeout(keysTimer);
  list.hidden = !shown;
  if (!shown) return;
  // The visualizer's look is listed only where the screen has a card to change.
  for (const row of list.querySelectorAll('.kviz')) row.hidden = !hasBand();
  keysTimer = setTimeout(() => showKeys(list, false), KEYS_MS);
}
/** Hide the key list when it is open. */
function dismissKeys() {
  const list = document.getElementById('keys');
  if (list && !list.hidden) showKeys(list, false);
}

/** Whether a key press is for a text field or carries a modifier: none of the desktop keys act. */
function typingIn(event) {
  return event.target.matches?.('input, textarea, select') || event.ctrlKey
    || event.metaKey || event.altKey;
}
/** Whether a key press is the frame's menu's own (open, or the keys that open it). */
const inEffectsMenu = (event) => menuOwns(event);

export function installKeys() {
  for (const kind of ['click', 'wheel', 'touchstart']) {
    document.addEventListener(kind, (event) => {
      // A key in the frame's menu makes a click of its own (detail 0): that is not a pointer.
      if (kind === 'click' && event.detail === 0 && inMenu(event.target)) return;
      dismissKeys();
    }, mine({ capture: true, passive: true }));
  }
  window.addEventListener('scroll', dismissKeys, mine({ capture: true, passive: true }));

  // Any key but a modifier on its own (it comes before the key it modifies) closes the list; a
  // `?` that the keys below act on toggles it instead. It runs in the capture phase: the dial
  // stops the keys it takes (arrows, Page Up/Down, Home) before they reach the document.
  document.addEventListener('keydown', (event) => {
    const key = event.key;
    if (inEffectsMenu(event)) return;
    if (!['Shift', 'Control', 'Alt', 'Meta', 'AltGraph'].includes(key)
      && (key !== '?' || typingIn(event))) dismissKeys();
  }, mine({ capture: true }));

  // The desktop keys: none while a text field has focus or with a modifier; arrows on a
  // focused control are that control's.
  document.addEventListener('keydown', (event) => {
    const key = event.key;
    if (typingIn(event) || inEffectsMenu(event)) return;
    // A held key repeats only for the arrows: a held m would flap the mute.
    if (event.repeat && key !== 'ArrowLeft' && key !== 'ArrowRight') return;
    const list = document.getElementById('keys');
    if (key === '?') {
      if (list) showKeys(list, list.hidden);
      return;
    }
    if (key === 'Escape') dismissKeys();
    // The look of the visualizer is the page's own, not a write: a guest and a lost link have it.
    if (key === 'v') {
      cycleLook();
      return;
    }
    // Arranging is the page's own too, and nothing below it is for a page being arranged.
    if (key === 'A' || key === 'a') {
      openArrange();
      return;
    }
    if (arranging()) return;
    const body = document.body.classList;
    if (body.contains('lost') || body.contains('guest') || !body.contains('writes')) return;
    const knob = document.querySelector('sv-knob');
    const own = event.target.closest?.('sv-knob, sv-slider, .seg, .presets, .bands, .bedit');
    const flat = document.querySelector('#part-tone .opt[data-mode="flat"]');
    if ((key === 'ArrowLeft' || key === 'ArrowRight') && !own && knob) {
      knob.step(key === 'ArrowRight' ? 1 : -1);
      event.preventDefault();
    } else if (key === 'm' && knob?.dataset.mute) {
      knob.toggleMute();
    } else if (key === '0' && document.querySelector('sv-slider[data-key="balance"]')) {
      ember.core.actions.setControl('balance', { value: 0 });
    } else if (key === 'f' && flat && flat.getAttribute('aria-pressed') !== 'true') {
      ember.core.actions.setMode('flat');
    } else if (key === 'p') {
      (document.querySelector('.pre[aria-pressed="true"]') ?? document.querySelector('.pre'))
        ?.focus();
    }
  }, mine());
}
