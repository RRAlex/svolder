/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The Scenes popup as strings (model in, markup out; no DOM, no state). The list is the daemon's
// view as the core reads it (core/scenes.js): a row for each scene with a lamp, its name, "On now"
// at the right of the scene that is on, the chips of its parts under it (a part that is not
// connected is a dashed chip, "Hall · not connected"), and a pencil and a bin at the end of the
// row. A tap on the name recalls; the pencil and the bin ask in place (each row has a place for
// its ask). Below the list: the line that says how the last action went, "Save current as…" with
// its field, and the line that says what a save takes. Classes are frame.css's `fp-*`, `fi-*` and
// `sc-*`, which read the `--frame-*` tokens alone.

import { attr, esc } from '../../html.js';
import {
  chipsOf, onNow, outcomeOf, said as asSaid, saveHint,
} from '../../scenes.js';
import { heading } from '../frame.js';

/** The id of `part` of the Scenes popup. */
const sid = (part) => `popup-scenes-${part}`;

/** The places and controls a wire reaches by id. */
export const IDS = Object.freeze({
  page: sid('page'),
  list: sid('list'),
  reading: sid('reading'),
  ask: sid('ask'),
  say: sid('say'),
  field: sid('name'),
  save: sid('save'),
  over: sid('over'),
  hint: sid('hint'),
});

/** The id of the row `i`, of its name button, and of its place for an ask. */
export const rowId = (i) => sid(`row-${i}`);
export const mainId = (i) => sid(`main-${i}`);
export const recallId = (i) => sid(`recall-${i}`);
export const renameId = (i) => sid(`rename-${i}`);
export const deleteId = (i) => sid(`delete-${i}`);
export const askId = (i) => sid(`row-ask-${i}`);

/** The words of the popup. `answers` are the words of the buttons of an ask. */
export const WORDS = Object.freeze({
  title: 'Scenes',
  intro: 'A scene is the sound of the computer and of each device, saved by name.',
  unavailable: 'Scenes are not available here.',
  none: 'No scenes yet. Save the current sound below.',
  reading: 'reading…',
  onNow: 'On now',
  recall: 'Recall',
  renameWord: 'Rename…',
  deleteWord: 'Delete…',
  saveHeading: 'Save current as…',
  fieldLabel: 'Scene name',
  placeholder: 'Name, for example Late night',
  save: 'Save',
  newName: 'New name',
  sceneName: 'Scene name',
  recalling: 'Recalling…',
  saved: (name) => `Saved ${name}.`,
  renamed: (from, to) => `${from} is now ${to}.`,
  deleted: (name) => `Deleted ${name}.`,
  recallAsk: (name) => `Recall ${name}?`,
  renameAsk: (name) => `Rename “${name}” to`,
  deleteAsk: (name) => `Delete “${name}”? The sound now playing does not change.`,
  overAsk: (name) => `Save over “${name}”?`,
  answers: Object.freeze({
    rename: 'Rename', delete: 'Delete', keep: 'Keep', cancel: 'Cancel', overwrite: 'Save over',
    recall: 'Recall anyway', save: 'Save',
  }),
});

/** A pencil and a bin, drawn in the ink of the button's text. */
const ICONS = Object.freeze({
  rename: '<svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true" focusable="false">'
    + '<path d="M4 20h4L19 9l-4-4L4 16z M13.5 6.5l4 4" fill="none" stroke="currentColor" '
    + 'stroke-width="1.8" stroke-linejoin="round" stroke-linecap="round"/></svg>',
  delete: '<svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true" focusable="false">'
    + '<path d="M5 7h14 M10 7V5h4v2 M7 7l1 12h8l1-12 M10.5 11v5 M13.5 11v5" fill="none" '
    + 'stroke="currentColor" stroke-width="1.8" stroke-linejoin="round" stroke-linecap="round"/>'
    + '</svg>',
});

/** One row of the list. A row is lit only when the view says which scene is on (`on`, which is
 * `null` while it is not settled). */
function rowHtml(scene, i, on) {
  const lit = on !== null && scene.name === on;
  const chips = chipsOf(scene).map((chip) => `<span class="fp-tag${chip.present ? '' : ' sc-off'}"`
    + `${attr('title', chip.present ? null : 'not connected')}>${esc(chip.label)}</span>`).join('');
  return `<li class="sc-row${lit ? ' on' : ''}" id="${rowId(i)}" data-name="${esc(scene.name)}">`
    + '<div class="sc-line">'
    + `<button type="button" class="sc-main" id="${mainId(i)}" data-act="recall" data-i="${i}"`
    + ` aria-label="${esc(`Recall ${scene.name}`)}"${attr('aria-current', lit ? 'true' : null)}>`
    + `<span class="fp-dot${lit ? ' on' : ''}"></span>`
    + `<span class="sc-name">${esc(scene.name)}</span>`
    + `${lit ? `<span class="sc-state">${esc(WORDS.onNow)}</span>` : ''}</button>`
    + `<span class="sc-chips">${chips}</span>`
    + `<button type="button" class="sc-recall" id="${recallId(i)}" data-act="recall" data-i="${i}"`
    + ` aria-hidden="true" tabindex="-1">${WORDS.recall}</button>`
    + `<button type="button" class="sc-ico" id="${renameId(i)}" data-act="rename" data-i="${i}"`
    + ` aria-label="${esc(`Rename ${scene.name}`)}">${ICONS.rename}`
    + `<span class="sc-word">${WORDS.renameWord}</span></button>`
    + `<button type="button" class="sc-ico" id="${deleteId(i)}" data-act="delete" data-i="${i}"`
    + ` aria-label="${esc(`Delete ${scene.name}`)}">${ICONS.delete}`
    + `<span class="sc-word">${WORDS.deleteWord}</span></button>`
    + `</div><div class="sc-ask" id="${askId(i)}"></div></li>`;
}

/** The list: the rows, or the words for none. */
export function listHtml(view) {
  if (!view || view.scenes.length === 0) return `<p class="fp-note">${esc(WORDS.none)}</p>`;
  const on = onNow(view);
  return `<ul class="fpe-rows sc-rows">${view.scenes.map((scene, i) => rowHtml(scene, i, on)).join('')}</ul>`;
}

/** The dim line while the scene that is on is not settled yet. */
export const readingHtml = (view) => (view && !view.known && view.scenes.length > 0
  ? `<p class="fp-note sc-reading" role="status">${esc(WORDS.reading)}</p>` : '');

/** The line that says how the last action went: `said` is `{ headline, lines, failed }`, the
 * headline in bold, the rest in the same ink, the lines that are problems in the warn ink. */
export function sayHtml(said) {
  if (!said) return '';
  const lines = (said.lines ?? []).map((line) => `<span class="sc-skip">${esc(line)}</span>`);
  const set = said.set ? `${esc(said.set)} ` : '';
  return `<p class="fp-note sc-say${said.failed ? ' fp-failed' : ''}">`
    + `<b>${esc(said.headline)}</b> ${set}${lines.join(' ')}</p>`;
}

/** The say line of a recall's answer; `view` tells which parts are away. */
export function recallSaid(name, outcome, view = null) {
  const parts = view?.scenes.find((scene) => scene.name === name)?.parts ?? [];
  const how = outcomeOf(name, outcome, parts.filter((part) => !part.present).map((part) => part.who));
  return { headline: how.headline, set: how.set, lines: how.problems, failed: false };
}

/** The say line of a save's answer: `Saved Late night.`, then the daemon's note for each device
 * that was left out of the scene (a device that is not connected). */
export function savedSaid(name, outcome) {
  const rows = Array.isArray(outcome) ? outcome : [];
  const lines = rows.flatMap((row) => (Array.isArray(row.notes) ? row.notes : []))
    .filter((note) => typeof note === 'string' && note !== '').map(asSaid);
  return { headline: WORDS.saved(name), lines, failed: false };
}

/** The line under the save field: what a save takes. */
export const hintHtml = (page) => `<p class="fp-note sc-hint">${esc(saveHint(page))}</p>`;

/** The save field and its button; `value` is what was typed. */
export const saveFormHtml = (value = '') => `<form class="fi-form sc-save" id="${IDS.save}">`
  + `<input class="fi-input" id="${IDS.field}" type="text" maxlength="40" required `
  + `autocomplete="off" placeholder="${esc(WORDS.placeholder)}" `
  + `aria-label="${esc(WORDS.fieldLabel)}" value="${esc(value)}">`
  + `<button type="submit" class="fi-btn" data-act="save">${esc(WORDS.save)}</button></form>`;

/** The page: the note, the list (and its reading line), the place for a question about a recall,
 * the say line, the save field with its place for a question, and the line of what a save takes.
 * `m` is `{ view, page, said, value }`. */
export function pageHtml({
  view, page, said = null, value = '',
}) {
  if (!view) return `<div id="${IDS.page}"><p class="fp-note">${esc(WORDS.unavailable)}</p></div>`;
  return `<div id="${IDS.page}"><p class="fp-note">${esc(WORDS.intro)}</p>`
    + `<div id="${IDS.reading}">${readingHtml(view)}</div>`
    + `<div id="${IDS.list}">${listHtml(view)}</div>`
    + `<div class="sc-ask" id="${IDS.ask}"></div>`
    + `<div id="${IDS.say}" role="status">${sayHtml(said)}</div>`
    + `${heading(WORDS.saveHeading)}${saveFormHtml(value)}`
    + `<div class="sc-ask" id="${IDS.over}"></div>`
    + `<div id="${IDS.hint}">${hintHtml(page)}</div></div>`;
}
