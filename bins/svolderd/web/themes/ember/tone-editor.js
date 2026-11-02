/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The tone control's steppers, which repeat while held, and the band editor: the band picked in
// the table, kept across every swap of the parts, with its three steppers and the value a tap
// turns into a field. A write goes out as the page's other writes do, one in flight at a time.
//
// ember.js calls `syncEditor` when a screen is in place and `installToneEditor` where its
// listeners stand, giving it the function that draws the parts whose swap waited once a hold ends.

import { ember, mine, shown } from './bind.js';
import { grip } from './keep.js';

/** Draws the parts whose swap waited (live.js's), given to [`installToneEditor`]. */
let readWaiting = null;

/** Whether a gesture on `element` is still live: it is on the page and the stream is not lost. */
const live = (element) => element.isConnected && !document.body.classList.contains('lost');

let held = null;

/** The core's write for a stepper's `data-post` path and `data-json`: the loudness, or a band's
 * parameter. */
function writeOf(button, device) {
  const body = JSON.parse(button.dataset.json);
  const { actions } = ember.core;
  if (button.dataset.post === 'eq/loudness') return actions.setLoudness(body, { device });
  const { param, ...rest } = body;
  return actions.setBand(Number(button.dataset.post.split('/').pop()), param, rest, { device });
}

/** One step: write the stepper's data-json through the core. The answer's text goes to the
 * stepper's value, or for the Loudness round buttons to the strength's words on the Loudness
 * button, so they follow a held - or + while the part swap waits. */
async function stepOnce(button, device = shown()) {
  if (!live(button) || button.closest('[inert]') || shown() !== device) return false;
  if (button.getAttribute('aria-disabled') === 'true') return false;
  const { signal } = ember;
  const answer = await writeOf(button, device);
  if (signal.aborted || !answer.ok) return false;
  const value = button.closest('.stp')?.querySelector('.v');
  if (value && answer.text && !value.querySelector('input')) value.textContent = answer.text;
  const words = button.closest('#part-tone')?.querySelector('#loud-strength .n');
  if (words && answer.text) {
    words.textContent = answer.text;
    button.closest('.seg')?.setAttribute('data-strength', answer.text);
  }
  return true;
}

function stopHold() {
  if (!held) return;
  held.stopped = true;
  clearTimeout(held.timer);
  held = null;
  grip(null);
  readWaiting();
}

/** The button a swap of its part put in place of `button` (the same path and body), else
 * `button` itself. */
function twin(button) {
  if (button.isConnected) return button;
  return [...document.querySelectorAll('[data-post]')].find((e) => e.isConnected
    && e.dataset.post === button.dataset.post && e.dataset.json === button.dataset.json) ?? button;
}

/** A press steps once; a hold repeats after 380 ms and then every 70 ms, one write in flight; a
 * swap of the button's part (the Loudness buttons' part is swapped as the strength changes) does
 * not end it. */
function startHold(first) {
  stopHold();
  const hold = { stopped: false, timer: null, device: shown() };
  held = hold;
  let button = first;
  grip(button);
  const again = (delay) => {
    hold.timer = setTimeout(async () => {
      button = twin(button);
      grip(button);
      if (hold.stopped || !live(button)) return;
      if (await stepOnce(button, hold.device) && !hold.stopped) again(70);
    }, delay);
  };
  stepOnce(button, hold.device).then((ok) => {
    if (ok && !hold.stopped) again(380);
  });
}

// The band editor: the band picked in the table, kept across every swap of the parts; its three
// steppers and values are built here with the DOM, never from HTML text.
let band = null;
let bandDevice = null;
const PARAMS = [['f', 'Frequency', 1], ['g', 'Gain', 2], ['q', 'Q', 3]];

function stepper(param, label) {
  const box = document.createElement('div');
  box.className = 'stp';
  box.dataset.param = param;
  const name = document.createElement('span');
  name.className = 'lbl';
  name.textContent = label;
  const button = (dir, text) => {
    const b = document.createElement('button');
    b.type = 'button';
    b.className = 'btn sm';
    b.textContent = text;
    b.dataset.post = `eq/bands/${band}`;
    b.dataset.json = JSON.stringify({ param, step: dir });
    b.setAttribute('aria-label', `${dir < 0 ? 'Lower' : 'Raise'} the ${label.toLowerCase()}`);
    return b;
  };
  const value = document.createElement('button');
  value.type = 'button';
  value.className = 'v';
  value.setAttribute('aria-label', `Type a ${label.toLowerCase()}`);
  box.append(name, button(-1, '−'), value, button(1, '+'));
  return box;
}

/** Show the editor for the picked band, its values read from the band table's row. */
function paintEditor() {
  const box = document.querySelector('#part-bands .bedit');
  const row = band && document.querySelector(`#part-bands .band[data-band="${band}"]`);
  for (const r of document.querySelectorAll('#part-bands .band')) {
    r.setAttribute('aria-pressed', String(r === row));
  }
  if (!box) return;
  if (!row || row.getAttribute('aria-disabled') === 'true') {
    box.hidden = true;
    return;
  }
  box.hidden = false;
  if (box.dataset.band !== String(band)) {
    box.dataset.band = String(band);
    const title = document.createElement('h4');
    title.textContent = `Band ${band}`;
    box.replaceChildren(title, ...PARAMS.map(([param, label]) => stepper(param, label)));
  }
  for (const [param, , column] of PARAMS) {
    const value = box.querySelector(`[data-param="${param}"] .v`);
    if (value && !value.querySelector('input')) {
      value.textContent = row.children[column].textContent;
    }
  }
}

/** Pick band `n` (1-based) as a click on its row does: the editor opens on it. A band whose row
 * is not there or is disabled is not picked. */
export function pickBand(n) {
  const row = document.querySelector(`#part-bands .band[data-band="${n}"]`);
  if (!row || row.getAttribute('aria-disabled') === 'true') return;
  band = n;
  paintEditor();
}

/** A screen is in place: the band picked on another device is forgotten, the editor repainted. */
export function syncEditor() {
  if (shown() !== bandDevice) {
    band = null;
    bandDevice = shown();
  }
  paintEditor();
}

/** Listen for the presses and keys on the steppers and the band table; `given.readWaiting` is
 * called when a hold ends; the listeners go with the mount's signal. */
export function installToneEditor(given) {
  readWaiting = given.readWaiting;
  document.addEventListener('pointerdown', (event) => {
    const button = event.target.closest('[data-post]');
    if (!button || event.button !== 0) return;
    event.preventDefault();
    startHold(button);
  }, mine());
  for (const name of ['pointerup', 'pointercancel', 'lostpointercapture', 'blur']) {
    window.addEventListener(name, stopHold, mine());
  }
  // A button released out of the window is never heard: no button down ends the hold.
  window.addEventListener('pointermove', (event) => {
    if (held && event.buttons === 0) stopHold();
  }, mine());
  document.addEventListener('visibilitychange', stopHold, mine());
  document.addEventListener('click', (event) => {
    const button = event.target.closest('[data-post]');
    if (button && event.detail === 0) stepOnce(button); // Enter or Space on a focused stepper
  }, mine());

  document.addEventListener('click', (event) => {
    const row = event.target.closest('#part-bands .band');
    if (row) pickBand(Number(row.dataset.band));
  }, mine());
  document.body.addEventListener('htmx:oobAfterSwap', (event) => {
    if (['part-tone', 'part-bands'].includes(event.detail.target?.id)) paintEditor();
  }, mine());

  // Tap a value to type it: Enter writes it, read on the server with its unit; Esc puts it back.
  document.addEventListener('click', (event) => {
    const value = event.target.closest('.bedit .v');
    if (!value || value.querySelector('input')) return;
    const param = value.closest('.stp').dataset.param;
    const input = document.createElement('input');
    input.inputMode = 'decimal';
    input.setAttribute('aria-label', value.getAttribute('aria-label'));
    const before = value.textContent;
    input.value = before;
    value.replaceChildren(input);
    input.focus();
    input.select();
    input.addEventListener('keydown', async (key) => {
      if (key.key === 'Escape') {
        value.textContent = before;
        paintEditor();
      }
      if (key.key !== 'Enter' || !live(input)) return;
      const { core, signal } = ember;
      const answer = await core.actions.setBand(band, param, { value: input.value },
        { device: bandDevice });
      if (signal.aborted || !answer.ok) return;
      value.textContent = answer.text ?? '';
    });
  }, mine());
}
