/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// What a press on the theme Ember's controls does: a button of a control, a power switch, a tone
// mode, a preset or a slot of a device's own, the forms that rename, delete and save, and a
// choice in a list. Each is the core's action for it: the element carries what to write
// (`data-json`, read when it is pressed) and the core writes it, says why a refusal was one and
// reads the device again, and what the answer changed comes back as the parts of the model.
//
// Also here: the presets' forms, which open and close one at a time; the tap on a control that a
// guest cannot use, which says so; Sign out; and Try now.

import { ember, mine } from './bind.js';
import { request } from './keep.js';

/** The JSON a control's `data-json` holds, or `{}`. */
function bodyOf(element) {
  try {
    return JSON.parse(element.dataset.json ?? '{}');
  } catch {
    return {};
  }
}

/** Write what a press says to the device of the page; a request of a device's own presets is
 * waited for, so the swap of its part waits too (`part-device-presets`), and is drawn from the
 * newest copy once it is answered. */
async function send(call, part = null) {
  const { core, signal } = ember;
  if (part) request(part, true);
  try {
    return await call(core.actions);
  } finally {
    if (part && !signal.aborted) {
      request(part, false);
      ready();
    }
  }
}

/** The form `form` is being sent: its part's swap does not wait for it. A refused form stays
 * open, with what was typed in it. */
async function sending(form, call, part) {
  form.dataset.sending = '';
  try {
    const answer = await send(call, part);
    if (!answer.ok) delete form.dataset.sending;
  } catch (error) {
    delete form.dataset.sending;
    throw error;
  }
}

/** Draws the parts whose swap waited (live.js's), given to [`installPress`]. */
let ready = () => {};

/** Open and close the presets' forms in place, one at a time. */
function forms(event) {
  const opener = event.target.closest('[data-open]');
  if (opener) {
    const row = opener.closest('.prow') ?? opener.parentElement;
    const kind = opener.matches('[aria-label^="Rename"]') ? 'rename'
      : opener.matches('[aria-label^="Delete"]') ? 'delete'
        : opener.dataset.open || null;
    let form = kind ? row.nextElementSibling : opener.nextElementSibling;
    while (form && kind && form.dataset.form !== kind) form = form.nextElementSibling;
    if (form) {
      // One form at a time: opening one closes the others.
      for (const other of opener.closest('.presets').querySelectorAll('.pform')) {
        other.hidden = other !== form;
      }
      form.hidden = false;
      form.querySelector('input')?.focus();
    }
    return;
  }
  const closer = event.target.closest('[data-close]');
  if (closer) {
    closer.closest('form').hidden = true;
    ready();
  }
}

/** A press on the control, switch, mode, preset or slot `event` names. */
function pressed(event) {
  const element = event.target.closest('[data-json]');
  if (!element || element.matches('form') || event.defaultPrevented) return;
  const section = element.closest('#part-controls, #part-power, #part-tone, #part-presets,'
    + ' #part-device-presets');
  if (!section) return;
  const body = bodyOf(element);
  switch (section.id) {
    case 'part-controls':
    case 'part-power':
      if (element.dataset.key) send((a) => a.setControl(element.dataset.key, body));
      break;
    case 'part-tone':
      if (element.dataset.mode) {
        if (element.getAttribute('aria-pressed') !== 'true') {
          ember.core.toneNote.clicked(element.dataset.mode);
        }
        send((a) => a.setMode(element.dataset.mode));
      }
      break;
    case 'part-presets':
      if (element.matches('.pre')) send((a) => a.recallPreset(body.name));
      break;
    default:
      if (!element.matches('.pre')) break;
      if (element.closest('.slot')) {
        send((a) => a.recallSlot(body.slot), 'part-device-presets');
      } else send((a) => a.applySetting(body.name), 'part-device-presets');
  }
}

/** A form of the presets' lists is sent. */
function submitted(event) {
  const form = event.target.closest?.('form.pform');
  if (!form || form.dataset.comp) return;
  event.preventDefault();
  const body = bodyOf(form);
  const value = (name) => form.elements[name]?.value ?? '';
  if (form.closest('#part-device-presets')) {
    sending(form, (a) => a.saveSlot(body.slot, true), 'part-device-presets');
    return;
  }
  switch (form.dataset.form) {
    case 'rename':
      sending(form, (a) => a.renamePreset(body.from, value('to')));
      break;
    case 'delete':
      sending(form, (a) => a.deletePreset(body.name));
      break;
    default:
      sending(form, (a) => a.savePreset(value('name')));
  }
}

/** A choice made in a list of a control's values. */
function chosen(event) {
  const select = event.target.closest?.('#part-controls select[data-key], '
    + '#part-power select[data-key]');
  if (select) send((a) => a.setControl(select.dataset.key, { value: select.value }));
}

/** Listen for the presses, `given.readWaiting` drawing the parts whose swap waited when a use
 * ends; they go when the mount's signal does. */
export function installPress(given) {
  ready = given.readWaiting;
  document.addEventListener('click', forms, mine());
  document.addEventListener('click', pressed, mine());
  document.addEventListener('submit', submitted, mine());
  document.addEventListener('change', chosen, mine());

  // A touch on a control a guest cannot use says so, with the way to sign in.
  document.addEventListener('click', (event) => {
    const inert = event.target.closest('[aria-disabled="true"]');
    if (!inert || !document.body.classList.contains('guest')) return;
    event.preventDefault();
    ember.core.actions.signInPrompt();
  }, mine({ capture: true }));

  // The footer's Sign out ends this browser's sign-in and loads the page again as a visitor.
  document.addEventListener('click', (event) => {
    if (event.target.closest('#sign-out')) ember.core.actions.signOut();
  }, mine());

  // Try now on the lost-link card.
  document.addEventListener('click', (event) => {
    if (event.target.closest('[data-try]')) ember.core.actions.connect();
  }, mine());
}
