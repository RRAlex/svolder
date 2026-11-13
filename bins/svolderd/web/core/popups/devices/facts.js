/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The facts of a device's page in the Devices popup and its *Identify* button, as draws: model in,
// string out. Pure: no page, no core, no request.
//
// The facts are one description list: Product, Unit (absent for a guest), Firmware, DSP, Profile,
// Link, Signal (with how long ago it was heard, kept current by the core's ticker) and, when the
// daemon has a note on it, Passphrase. Each value is in a `dd` that carries its label in
// `data-fact`, which is how a check finds it. *Identify* stands under them when the daemon offers
// it for the device's kind; on a device that cannot be asked now (not connected, or the daemon
// says it cannot) it stays, marked `aria-disabled` (it keeps the focus and sends nothing), and the
// words beside it say why.
//
// `device` is the popup's device (`{ name, connected, model }`, `model` the store's model of that
// device, whose `facts` part holds the facts). With no facts yet nothing is drawn. Every id is the
// part's own, never made of the device's name, which a rename changes. The section carries the
// daemon's clock (`data-now`) and the signal's cell its own stamp: `FACTS_AGES` are the selectors
// `core.clock.tickAges` keeps them current with.

import { attr, esc } from '../../html.js';

/** The place of the facts on a device's page: the wire puts the section into it. */
export const FACTS_ID = 'popup-devices-facts';
/** *Identify*, and the words beside it. */
export const IDENTIFY_ID = 'popup-devices-identify';
export const IDENTIFY_NOTE_ID = 'popup-devices-identify-note';

/** What `core.clock.tickAges` is handed besides the readings' own selectors: the element that
 * carries the daemon's clock, and the cell of the signal's strength. */
export const FACTS_AGES = Object.freeze({
  stamp: `#${FACTS_ID} section[data-now]`,
  signal: `#${FACTS_ID} dd[data-signal][data-heard-at]`,
});

export const FACT_WORDS = {
  title: 'Device',
  labels: {
    product: 'Product', unit: 'Unit', firmware: 'Firmware', dsp: 'DSP', profile: 'Profile',
    link: 'Link', signal: 'Signal', passphrase: 'Passphrase',
  },
  /** A fact the device has not told. */
  unknown: '—',
  product: (name, id) => (name ? `${name} (product ${id})` : `product ${id}`),
  identify: 'Identify',
  identifyNote: 'Asks the unit to show itself, so you can tell it from the others.',
  identifyAway: 'The device is not connected, so it cannot be asked to show itself.',
  identifyBusy: 'The device cannot be asked to show itself just now.',
};

/** The labels of the facts, in the order they stand. */
export const FACT_LABELS = Object.freeze(Object.values(FACT_WORDS.labels));

/** One fact: its label and its value, which carries the label for a check to find. */
const fact = (label, value, more = '', cls = '') => `<div class="fp-row fpd-fact">`
  + `<dt class="fp-row-label">${esc(label)}</dt>`
  + `<dd class="fp-row-value${cls}"${attr('data-fact', label)}${more}>${value}</dd></div>`;

/** The facts of `facts` (a device's `facts` part) as the rows of the list. */
function factRows(facts, words) {
  const labels = words.labels;
  const age = facts.signal_age;
  const signal = age
    ? `${attr('data-head', age.head)}${attr('data-heard-at', age.heard_at_ms)}`
      + `${age.live ? ' data-live' : ''}` : '';
  return [
    fact(labels.product, esc(words.product(facts.product, facts.product_id))),
    facts.unit ? fact(labels.unit, esc(facts.unit), '', ' fpd-mono') : '',
    fact(labels.firmware, esc(facts.firmware)),
    fact(labels.dsp, facts.dsp === null || facts.dsp === undefined ? words.unknown
      : esc(facts.dsp)),
    fact(labels.profile, esc(facts.profile)),
    fact(labels.link, esc(facts.link)),
    fact(labels.signal, esc(facts.signal), ` data-signal${signal}`),
    facts.passphrase_note === null || facts.passphrase_note === undefined ? ''
      : fact(labels.passphrase, esc(facts.passphrase_note), '', ' fpd-warn'),
  ].join('');
}

/** What stands beside *Identify*: what it does, or why it cannot be pressed now. */
function identifyHtml(device, facts, words) {
  if (!facts.identify_offered) return '';
  const idle = !device.connected ? words.identifyAway : (facts.identify ? '' : words.identifyBusy);
  return '<p class="fpd-identify">'
    + `<button type="button" class="fi-btn"${attr('id', IDENTIFY_ID)}`
    + `${attr('data-gap', facts.identify_gap_secs)}`
    + `${idle ? ' aria-disabled="true"' : ''}${attr('aria-describedby', IDENTIFY_NOTE_ID)}>`
    + `${esc(words.identify)}</button> `
    + `<span class="fp-note"${attr('id', IDENTIFY_NOTE_ID)}>${esc(idle || words.identifyNote)}</span>`
    + '</p>';
}

/**
 * The facts section of `device`, for the wire's place (`FACTS_ID`): the heading, the list and
 * *Identify*; empty while the device's model has no facts.
 */
export function factsHtml(device, words = FACT_WORDS, { readOnly = false } = {}) {
  const facts = device.model?.facts;
  if (!facts) return '';
  return `<section${attr('data-now', facts.now_ms)}>`
    + `<h4 class="fp-heading">${esc(words.title)}</h4>`
    + `<dl class="fp-rows fpd-facts">${factRows(facts, words)}</dl>`
    + `${readOnly ? '' : identifyHtml(device, facts, words)}`
    + '</section>';
}
