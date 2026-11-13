/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The All readings disclosure of a device's page in the Devices popup: a native `<details>`
// (closed unless the state says open) whose summary reads `All readings`, holding a table of every
// reading the page's device holds: its name, its value in words and how long ago it was heard.
//
// Pure: a model in, a string out. `device` is the popup's device (`{ name, connected, model }`,
// `model` the store's model of that device, whose `facts` part lists the readings), `state` is
// `{ open }`. Nothing inside is drawn while it is closed: a wire puts `readingsBodyHtml` in its
// place (`READINGS_BODY_ID`) when the disclosure opens and at each change while it is open, so
// the `<details>` itself, and with it whether the person opened it, is never drawn again.
//
// A reading's value is the daemon's words: it words each reading through the device's profile
// (`63 (−26.1 dB)`), and a reading names its function, not a control of the model, so the page's
// own wording of a control's raw step (core/words.js `stepWords`) has nothing to take here. `say`
// is the converter, a parameter: a caller that can word a reading through the model hands its
// own, and a reading it has no words for keeps the daemon's.
//
// The ages are written here once, in the core's words (`ageWords`), and each cell carries when
// its reading was heard: `READINGS_AGES` are the selectors `core.clock.tickAges` keeps them
// current with. Every id is the part's own, never made of the device's name, which a rename
// changes.

import { attr, esc } from '../../html.js';
import { ageWords } from '../../words.js';

/** The disclosure, the `<details>` itself. */
export const READINGS_ID = 'popup-devices-readings';
/** The place inside it that a wire puts: the table, or the line that says there is none. */
export const READINGS_BODY_ID = 'popup-devices-readings-body';
/** The table: it carries the daemon's clock (`data-now`) the ages are counted from. */
export const READINGS_TABLE_ID = 'popup-devices-readings-table';

/** What `core.clock.tickAges` is handed to keep the ages current: the element that carries the
 * daemon's clock and the cells that carry when their reading was heard. */
export const READINGS_AGES = Object.freeze({
  stamp: `#${READINGS_TABLE_ID}`,
  ages: `#${READINGS_TABLE_ID} td.fpd-reading-age[data-heard-at]`,
});

export const READING_WORDS = {
  title: 'All readings',
  caption: (name) => `Readings of ${name}`,
  lastKnown: (name) => `Last known readings of ${name} (not connected)`,
  name: 'Reading',
  value: 'Value',
  age: 'Heard',
  ago: (age) => `${age} ago`,
  /** A reading the daemon has no stamp for. */
  unknown: '—',
  none: 'This device has sent no reading yet.',
  noneAway: 'This device is not connected, and no reading of it is known.',
};

/** A reading's value in the daemon's words: the converter when a caller hands none. */
export const wordsOf = (row) => row.value;

/** What a reading is called: the profile's label, its id where the profile names none. */
const nameOf = (row) => (row.label && row.label !== '?' ? row.label : row.id);

/** One reading's row: its name (the row's header), its value and its age. */
function rowHtml(row, facts, model, say, words) {
  const heard = Number(row.heard_at_ms);
  const stamped = row.heard_at_ms !== null && row.heard_at_ms !== undefined
    && Number.isFinite(heard) && Number.isFinite(Number(facts.now_ms));
  const age = stamped
    ? `<td class="fpd-reading-age"${attr('data-heard-at', row.heard_at_ms)}>`
      + `${esc(words.ago(ageWords((Number(facts.now_ms) - heard) / 1000)))}</td>`
    : `<td class="fpd-reading-age">${esc(words.unknown)}</td>`;
  return `<tr${attr('data-reading', row.id)}><th scope="row">${esc(nameOf(row))}</th>`
    + `<td>${esc(say(row, model) ?? wordsOf(row))}</td>${age}</tr>`;
}

/**
 * What the disclosure holds, for its place (`READINGS_BODY_ID`): nothing while it is closed;
 * open, the table of `device`'s readings in the daemon's order, captioned as the last known ones
 * when the device is not connected, or the line that says the device has none.
 */
export function readingsBodyHtml(device, state = null, { say = wordsOf, words = READING_WORDS } = {}) {
  if (!state?.open) return '';
  const facts = device.model?.facts;
  const list = facts?.rows ?? [];
  if (list.length === 0) {
    return `<p class="fp-note">${esc(device.connected ? words.none : words.noneAway)}</p>`;
  }
  const caption = device.connected ? words.caption(device.name) : words.lastKnown(device.name);
  return `<table class="fpd-readings-table"${attr('id', READINGS_TABLE_ID)}`
    + `${attr('data-now', facts.now_ms)}${device.connected ? '' : ' data-last-known'}>`
    + `<caption>${esc(caption)}</caption>`
    + `<thead><tr><th scope="col">${esc(words.name)}</th><th scope="col">${esc(words.value)}</th>`
    + `<th scope="col">${esc(words.age)}</th></tr></thead>`
    + `<tbody>${list.map((row) => rowHtml(row, facts, device.model, say, words)).join('')}</tbody>`
    + '</table>';
}

/** The disclosure of `device`'s readings: closed unless `state.open`. */
export function readingsHtml(device, state = null, options = {}) {
  const words = options.words ?? READING_WORDS;
  return `<details${state?.open ? ' open' : ''} class="fpd-readings"${attr('id', READINGS_ID)}>`
    + `<summary class="fpd-readings-title">${esc(words.title)}</summary>`
    + `<div${attr('id', READINGS_BODY_ID)}>${readingsBodyHtml(device, state, options)}</div>`
    + '</details>';
}
