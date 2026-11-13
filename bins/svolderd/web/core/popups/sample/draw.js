/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The sample popup's draws: model in, string out. The sample is the popup the checks open to
// prove the skeleton every popup fills in (popups/frame.js); the page never registers it, so no
// person reaches it. It has a two-entry side list, a paragraph, a place for an inline ask (a
// question and a form), a chip that opens a list and a live line that names the device it
// follows.

import { esc } from '../../html.js';

export const TITLE = 'Sample popup';

/** The side list. */
export const ENTRIES = [{ id: 'one', label: 'Page one' }, { id: 'two', label: 'Page two' }];

/** The ids the wire finds its places and its words by. */
export const IDS = {
  live: 'popup-sample-live',
  ask: 'popup-sample-ask',
  askOpen: 'popup-sample-ask-open',
  formOpen: 'popup-sample-form-open',
  count: 'popup-sample-count',
  name: 'popup-sample-name',
  listOpen: 'popup-sample-list-open',
  voice: 'popup-sample-voice',
};

/** The choices of the sample's list; the first is the one in use until another is picked. */
export const VOICES = ['Flat', 'Warm', 'Bright'];

/** What a screen reader calls the sample's list. */
export const LIST_LABEL = 'Voice';

/** The words of the sample's inline asks. */
export const WORDS = {
  presetName: 'A name for the sample',
  newName: 'A name for the sample',
  question: 'Count one more?',
  answers: { save: 'Count', keep: 'Leave it', rename: 'Name it', cancel: 'Cancel' },
};

/** The live line: the device it follows by name, then its volume (`device`: `{ url, name }` or
 * none; `volume`: the device's volume part or none). */
export function liveHtml({ device, volume }) {
  if (!device) return 'No device';
  const raw = volume?.raw;
  const reading = raw === null || raw === undefined ? 'no reading yet' : `volume ${esc(raw)}`;
  return `${esc(device.name ?? device.url)}: ${reading}`;
}

/** The page pane at `page` (`null`, the list, shows page one): page one has the paragraph, the
 * two words that open an ask, the chip that opens the list and names the `voice` in use, the
 * ask's place and the live line; page two says the `name` the form gave and how often the
 * question was answered (`count`). */
export function pageHtml({
  page, device, volume, name = '', count = 0, voice = VOICES[0],
}) {
  if (page === 'two') {
    return '<p>The second page of the sample.</p>'
      + `<p>Name: <span id="${IDS.name}">${esc(name)}</span></p>`
      + `<p>Counted: <span id="${IDS.count}">${esc(count)}</span></p>`;
  }
  return '<p>The first page of the sample: every shared piece of a popup, once.</p>'
    + '<p class="fi-set">'
    + `<button type="button" class="fi-btn" id="${IDS.askOpen}">Ask</button> `
    + `<button type="button" class="fi-btn" id="${IDS.formOpen}">Name</button> `
    + `<button type="button" class="fi-btn" id="${IDS.listOpen}" aria-haspopup="listbox"`
    + ` aria-expanded="false">${esc(LIST_LABEL)}: <span id="${IDS.voice}">${esc(voice)}</span>`
    + '</button></p>'
    + `<div id="${IDS.ask}"></div>`
    + `<p>Name: <span id="${IDS.name}">${esc(name)}</span>, counted: `
    + `<span id="${IDS.count}">${esc(count)}</span></p>`
    + `<p class="fp-live" id="${IDS.live}" aria-live="off">${liveHtml({ device, volume })}</p>`;
}
