/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The EQ library popup's draws: a model in, a string out. Pure, with no page and no core.
//
// Three pages, each about the side the popup was opened on (`side`: `{ computer, name }`, the
// device on show by its name, or the computer):
//
//   autoeq  a search field, the headphones that matched (each a disclosure of its measurements),
//           the preview of the chosen measurement for the side (a graph, the numbers the daemon
//           worked out, a chip that lists the other measurements of the headphone to compare with)
//           and the form that saves it as a preset
//   upload  a file input, a name and Save
//   saved   the presets the side lists, each marked with where it lives, and *Overwrite with
//           current…*
//
// The graph is drawn by core.plot from the daemon's numbers; nothing here fits a curve or decides
// where a fit is good enough. Every place a wire puts alone has an id here (`ids`), the words are
// `WORDS`, and a refusal is worded by `refusalWords`.

import { attr, esc } from '../../html.js';
import {
  FREQ_MARKS, frame as plotFrame, gapSpans, gridLines, path, pathThrough, zeroLine,
} from '../../plot.js';
import { PEAK_SHOWN_DB } from '../../tone-curve.js';
import { dbText, fixedEven, hzWords } from '../../words.js';
import { heading, rows, tag } from '../frame.js';

/** The popup's name: its folder's, and the prefix of every id. */
export const NAME = 'eq-library';

/** The popup's three pages, in order. */
export const ENTRIES = Object.freeze([
  { id: 'autoeq', label: 'AutoEq' },
  { id: 'upload', label: 'Upload a file' },
  { id: 'saved', label: 'Saved presets' },
]);

/** The id of `part` of the popup. */
export const eid = (part) => `popup-${NAME}-${part}`;

/** The places a wire puts alone. */
export const IDS = Object.freeze({
  query: eid('query'),
  status: eid('status'),
  results: eid('results'),
  preview: eid('preview'),
  previewWords: eid('preview-words'),
  importPlace: eid('import'),
  importWordsPlace: eid('import-words-place'),
  compareOpen: eid('compare-open'),
  compareClear: eid('compare-clear'),
  importName: eid('import-name'),
  importSave: eid('import-save'),
  importWords: eid('import-words'),
  file: eid('file'),
  upFields: eid('upload-fields'),
  upName: eid('upload-name'),
  upSave: eid('upload-save'),
  upWords: eid('upload-words'),
  upWordsPlace: eid('upload-words-place'),
  saved: eid('saved-list'),
  savedWords: eid('saved-words'),
  savedWordsPlace: eid('saved-words-place'),
  savedNote: eid('saved-note'),
});

/** The most characters a search takes (the daemon refuses more), and the most bytes of a file's
 * text. */
export const QUERY_MAX = 60;
export const FILE_MAX = 64 * 1024;

/** The popup's words. */
export const WORDS = {
  title: 'EQ library',
  computer: 'This computer',
  searchLabel: 'Search AutoEq for a headphone',
  searchHint: 'Type a headphone\'s name, such as HD 650.',
  searchGo: 'Search',
  searching: 'Searching…',
  nothing: (query) => `Nothing in AutoEq matches “${query}”.`,
  count: (n) => `${n} ${n === 1 ? 'headphone' : 'headphones'}`,
  more: (n) => `${n} more ${n === 1 ? 'headphone matches' : 'headphones match'}: `
    + 'narrow the search.',
  measurements: (n) => `${n} ${n === 1 ? 'measurement' : 'measurements'}`,
  recommended: 'Recommended',
  index: (n, date) => `AutoEq's index lists ${n.toLocaleString('en-US')} measurements; this copy `
    + `was fetched on ${date}.`,
  choose: 'Choose a measurement to see how well it can be followed.',
  fitting: (words, target) => `Fitting ${words} to ${target}…`,
  correction: (words) => `Reading ${words}…`,
  fit: 'Fit',
  fitValue: (db) => `${db} dB`,
  worst: 'Worst miss',
  worstValue: (db, hz) => `${db} dB at ${hz}`,
  lower: (db) => `Lower the volume by ${db} dB: the fitted bands boost that far.`,
  computerPlays: 'This is the correction as the computer plays it; nothing is fitted.',
  peaks: (db) => `It boosts by ${db} dB at most.`,
  preamp: 'Preamp',
  preampValue: (db) => `${db} dB`,
  gapKey: 'The shaded area is where the fit misses the target by a wide margin.',
  legend: { target: 'Target', fit: 'Fit', gap: 'Gap', compare: 'Compared' },
  compareOpen: 'Compare with…',
  compareLabel: 'Compare with another measurement of this headphone',
  compareOn: (words) => `Compared with ${words}`,
  compareClear: 'Stop comparing',
  compareNumbers: (words) => `Compared: ${words}`,
  plotLabel: 'The correction as a curve against frequency, from 20 Hz to 20 kHz',
  nameLabel: 'Preset name',
  save: 'Save as a preset',
  saving: 'Saving…',
  suggest: (name) => `Use “${name}”`,
  saved: (name, filters, target) => `Saved “${name}” `
    + `(${filters} ${filters === 1 ? 'filter' : 'filters'}). `
    + `It is saved only: it is not on ${target} yet. Recall it where the presets are.`,
  savedLate: (name, words) => `Saved “${name}” for ${words}, the measurement chosen before. `
    + 'It is saved only.',
  skipped: (n) => `${n} ${n === 1 ? 'line' : 'lines'} of the file `
    + `${n === 1 ? 'was' : 'were'} left out.`,
  noName: 'Give the preset a name.',
  noTarget: 'No device is on show, so there is no side to save for.',
  fileLabel: 'A parametric EQ file (Equalizer APO text, as AutoEq writes it), at most 64 KiB',
  fileRead: (name) => `${name} read.`,
  fileReading: 'Reading the file…',
  fileTooLarge: 'This file is larger than 64 KiB, which is more than a parametric EQ file holds.',
  fileUnreadable: 'The file could not be read.',
  uploadSave: 'Save the file as a preset',
  chooseFirst: 'Choose a file first.',
  savedHeading: (target) => `Saved presets for ${target}`,
  inLibrary: 'In the library',
  onNow: (target) => `On ${target} now`,
  builtIn: 'Built in',
  imported: 'Imported',
  overwrite: 'Overwrite with current…',
  overwriteAsk: (name, target) => `Replace “${name}” with the curve that is on ${target} now?`,
  overwritten: (name) => `“${name}” now holds the curve that is on.`,
  answers: { overwrite: 'Overwrite', keep: 'Keep it' },
  noPresets: 'There are no presets to list yet.',
  presetsUnknown: (name) => `${name} is not connected, so its presets are not known yet.`,
  noToneControl: (name) => `${name} has no tone control, so it has no presets of its own.`,
  notConnected: (name) => `${name} is not connected, so there is no curve to save.`,
  notRead: 'The tone control has not been read yet, so there is no curve to save.',
  builtInRefusal: (name) => `${name} is built in: save the curve under a new name.`,
  importedRefusal: (name) => `${name} is imported: save the curve under a new name.`,
  computerRefusal: 'The computer\'s points are saved as a new preset, not over one.',
  notPlayable: (why) => why,
};

/** The phrase of the daemon's words for a configuration file that is gone (it answers those as
 * `read_only` too): they say the step, so the page shows them as they are. */
const FILE_MISSING = ' is missing: the daemon keeps its running configuration; restore the file';

/** What a refusal says at its control. `kind` is the control (`search`, `preview`, `import`,
 * `upload`, `overwrite`), `answer` the call's `{ reason, status, error }`. The daemon's own words
 * stand where they say what is wrong with what was sent (a name, a file, a correction too long for
 * the computer); every other reason has words of the page's. */
export function refusalWords(kind, answer) {
  const own = typeof answer?.error === 'string' ? answer.error : '';
  switch (answer?.reason) {
    case 'busy': return 'AutoEq is busy with other downloads. Try again in a moment.';
    case 'not_listed': return 'AutoEq does not list this measurement any more. Search again.';
    case 'unreachable':
      // The daemon says AutoEq is off for the fake device in words that name the way out.
      if (own.startsWith('AutoEq is off')) return own;
      return 'AutoEq could not be reached, or what it sent is not a parametric '
        + 'EQ. Check the daemon\'s network and try again.';
    case 'taken': return 'A preset with this name exists already.';
    case 'bad_name': return own || 'This is not a name a preset can have.';
    case 'invalid': return own || (kind === 'upload'
      ? 'This is not a parametric EQ file svolder can read.' : 'This cannot be done here.');
    case 'missing': return 'This device is no longer configured.';
    case 'not_found_yet': return 'svolder has not found this device yet, and its profile does not '
      + 'say how many bands it has. Try again once it is connected.';
    case 'not_connected': return 'The device is not connected.';
    case 'read_only': return own.includes(FILE_MISSING) ? own
      : 'The configuration file is read-only, so nothing was saved.';
    case 'file_busy': return 'The configuration file is being written by something else. Try '
      + 'again in a moment.';
    case 'stopping': return 'The daemon is stopping, so nothing was saved. Try again once it runs.';
    case 'internal': return 'The daemon failed on this. Try again, and look at its log if it keeps '
      + 'failing.';
    case 'too_large': return WORDS.fileTooLarge;
    case 'no_answer': return 'The daemon did not answer. Try again.';
    case 'signed_out': return 'You are signed out. Sign in to change this.';
    case 'forbidden': return 'Changes are turned off on this page.';
    case 'no_target': return WORDS.noTarget;
    default: return own || 'This did not work. Try again.';
  }
}

// ---- the search ----

/** The index's date as words: UTC, so the same everywhere. */
export const dateOf = (unix) => new Date(unix * 1000).toISOString().slice(0, 10);

/** The search field, a form with a button: Enter and the button search at once, typing searches
 * after a pause (the wire's). It is drawn once with its page and never put, so what is typed and
 * where the caret stands stay. */
export function searchFormHtml({ query = '' } = {}) {
  return '<form class="fi-form fpe-search" role="search" data-form="search">'
    + `<label class="fpe-label" for="${IDS.query}">${esc(WORDS.searchLabel)}</label>`
    + `<input class="fi-input" type="search" id="${IDS.query}" name="eq-query"`
    + `${attr('value', query)}`
    + ` maxlength="${QUERY_MAX}" autocomplete="off" autocapitalize="off" spellcheck="false"`
    + ` enterkeyhint="search" aria-describedby="${IDS.status}">`
    + `<button type="submit" class="fi-btn">${esc(WORDS.searchGo)}</button></form>`;
}

/** The line under the search field: what it holds now. `search` is `{ status, query?, answer?,
 * words? }` (`idle`, `loading`, `ready`, `failed`). It is a status, so a screen reader hears each
 * change. */
export function statusHtml(search) {
  let said = WORDS.searchHint;
  let meta = '';
  let failed = false;
  if (search.status === 'loading') said = WORDS.searching;
  if (search.status === 'failed') {
    said = search.words;
    failed = true;
  }
  if (search.status === 'ready') {
    const { answer } = search;
    said = answer.groups.length === 0 ? WORDS.nothing(search.query)
      : WORDS.count(answer.groups.length);
    if (answer.more > 0) said += `. ${WORDS.more(answer.more)}`;
    meta = [WORDS.index(answer.measurements, dateOf(answer.fetched_unix)), answer.note]
      .filter(Boolean).map(esc).join(' ');
  }
  return `<p class="fp-note${failed ? ' fp-failed' : ''}">${esc(said)}</p>`
    + `${meta ? `<p class="fp-note fpe-meta">${meta}</p>` : ''}`;
}

/** The id of the button of measurement `v` of headphone `g`. */
export const variantId = (g, v) => eid(`v-${g}-${v}`);

/** The identity of a measurement: what tells it from every other. */
export const keyOf = (variant) => JSON.stringify([variant.name, variant.source, variant.form,
  variant.rig ?? null]);

/** Whether the disclosure of `group` is open when the person has not set it: the answer is short,
 * or the group holds the chosen measurement. */
export const defaultOpen = ({ answer, group, selected }) => answer.groups.length <= 3
  || (selected !== null && group.variants.some((m) => keyOf(m) === keyOf(selected.variant)));

/** The measurements of the search answer, grouped by headphone: a disclosure for each, open by
 * `defaultOpen` unless the person set it (`open`, a map of group key to whether it is open). The
 * first measurement of a headphone is AutoEq's recommended one. */
export function resultsHtml({ answer, selected = null, open = new Map() }) {
  if (!answer || answer.groups.length === 0) return '';
  const chosen = selected ? keyOf(selected.variant) : null;
  return answer.groups.map((group, g) => {
    const variants = group.variants.map((m, v) => '<li><button type="button"'
      + ` class="fi-btn fpe-variant"`
      + `${attr('id', variantId(g, v))} data-g="${g}" data-v="${v}"`
      + `${attr('aria-pressed', String(keyOf(m) === chosen))}>${esc(m.words)}`
      + `${v === 0 ? ` ${tag(WORDS.recommended)}` : ''}</button></li>`).join('');
    const shown = open.has(group.key) ? open.get(group.key)
      : defaultOpen({ answer, group, selected });
    return `<details class="fpe-group"${attr('data-key', group.key)}${attr('open', shown)}>`
      + `<summary class="fpe-group-name">${esc(group.name)}`
      + `<span class="fpe-count">${esc(WORDS.measurements(group.variants.length))}</span></summary>`
      + `<ul class="fpe-variants">${variants}</ul></details>`;
  }).join('');
}

// ---- the preview ----

const POINT_MIN = 2;

/** The points `{ hz, db }` of a curve on a preview's grid. */
const points = (freqs, db) => (freqs ?? []).map((hz, i) => ({ hz, db: db?.[i] }))
  .filter((p) => Number.isFinite(p.db));

/** The graph of a preview: the 0 dB line and the grid, the gap between the fit and the target as
 * a shaded area, the target dotted, the fit, and the compared measurement's target in a class of
 * its own. Every shape is the daemon's numbers on its grid. */
export function graphHtml(preview, compare = null) {
  const target = points(preview.freqs, preview.target);
  const fit = preview.fit ? points(preview.freqs, preview.fit.response) : [];
  const gap = preview.fit
    ? gapSpans({ spans: preview.fit.gap, target, fit, cls: 'fp-graph-gap' }) : '';
  const grid = gridLines(FREQ_MARKS.map(([hz]) => hz), 'fp-graph-grid');
  const other = compare ? path(pathThrough(points(compare.freqs, compare.target)),
    'fp-graph-compare') : '';
  const body = `${grid}${zeroLine('fp-graph-grid')}${gap}${other}`
    + `${path(pathThrough(target), 'fp-graph-target')}`
    + `${fit.length >= POINT_MIN ? path(pathThrough(fit), 'fp-graph-series') : ''}`;
  return `<div class="fp-graph fpe-plot" role="img"${attr('aria-label', WORDS.plotLabel)}>`
    + `${plotFrame(body, { cls: 'fpe-svg' })}</div>`;
}

/** What the graph's lines are, as a key: each swatch has a class of its own, so the key is not the
 * colour alone (a line's dash and a shade say it too, in the stylesheet). */
function legendHtml(preview, compare) {
  const item = (cls, words) => `<li class="fpe-key"><span class="fpe-swatch ${cls}"></span>`
    + `${esc(words)}</li>`;
  const list = [item('target', WORDS.legend.target)];
  if (preview.fit) list.push(item('fit', WORDS.legend.fit));
  if (preview.fit?.gap?.length) list.push(item('gap', WORDS.legend.gap));
  if (compare) list.push(item('compare', WORDS.legend.compare));
  return `<ul class="fpe-legend">${list.join('')}</ul>`;
}

/** The numbers of one preview, as rows: the fit and the worst miss for a device, the preamp for
 * the computer, and the line that says how far to lower the volume when the bands boost. */
function numbersHtml(preview, computer) {
  const list = [];
  if (preview.fit) {
    const rms = fixedEven(preview.fit.rms_db, 1);
    list.push({ label: esc(WORDS.fit), value: esc(WORDS.fitValue(rms)) });
    list.push({
      label: esc(WORDS.worst),
      value: esc(WORDS.worstValue(fixedEven(preview.fit.worst_db, 1),
        hzWords(preview.fit.worst_hz))),
    });
  } else if (computer) {
    list.push({
      label: esc(WORDS.preamp), value: esc(WORDS.preampValue(dbText(preview.preamp_db))),
    });
  }
  let out = rows(list);
  if (preview.peak_db >= PEAK_SHOWN_DB) {
    const db = fixedEven(preview.peak_db, 1);
    const line = preview.fit ? WORDS.lower(db) : WORDS.peaks(db);
    out += `<p class="fp-note fpe-lower">${esc(line)}</p>`;
  }
  return out;
}

/** The chip of the compare choice: *Compare with…* while the headphone has another measurement,
 * *Stop comparing* with the words of what is compared once one is chosen. It is drawn in every
 * state of the preview (loading and failed too), so the control that has the focus is still there
 * to take it back when the answer is drawn. */
function chipHtml({ selected, compare }) {
  const others = selected.group.variants
    .filter((m) => keyOf(m.variant) !== keyOf(selected.variant));
  if (compare) {
    return '<p class="fpe-chips"><span class="fpe-chip-on">'
      + `${esc(WORDS.compareOn(compare.words))}</span> `
      + `<button type="button" class="fi-btn" id="${IDS.compareClear}">${esc(WORDS.compareClear)}`
      + '</button></p>';
  }
  return others.length > 0
    ? `<p class="fpe-chips"><button type="button" class="fi-btn" id="${IDS.compareOpen}"`
      + ` aria-haspopup="listbox">${esc(WORDS.compareOpen)}</button></p>` : '';
}

/** The line that says a preview is being worked out: a place of its own, a live region that is
 * put in place and never replaced, so a screen reader hears it. */
export function previewWordsHtml({ selected, preview, side }) {
  if (!selected || preview.status !== 'loading') return '';
  return `<p class="fp-note">${esc(WORDS.fitting(selected.words,
    side.computer ? WORDS.computer : side.name))}</p>`;
}

/** The preview place. `selected` is the chosen measurement (`{ variant, words, group }`),
 * `preview` its state (`{ status, answer?, words? }`: `idle`, `loading`, `ready`, `failed`),
 * `compare` the measurement compared with (`{ variant, words }`, or none), `side` the side. */
export function previewHtml({
  selected, preview, compare = null, side,
}) {
  if (!selected) return `<p class="fp-note">${esc(WORDS.choose)}</p>`;
  const title = `${selected.variant.name} · ${selected.words}`;
  const head = heading(title);
  if (preview.status === 'loading') {
    return `${head}${chipHtml({ selected, compare })}`;
  }
  if (preview.status === 'failed') {
    return `${head}${chipHtml({ selected, compare })}`
      + `<p class="fp-note fp-failed" role="alert">${esc(preview.words)}</p>`;
  }
  if (preview.status !== 'ready') return head;
  const { preview: main, compare: second } = preview.answer;
  const chip = chipHtml({ selected, compare });
  const gapKey = main.fit?.gap?.length ? `<p class="fp-note">${esc(WORDS.gapKey)}</p>` : '';
  const computerNote = main.fit ? '' : `<p class="fp-note">${esc(WORDS.computerPlays)}</p>`;
  const compared = second
    ? `${heading(WORDS.compareNumbers(compare?.words ?? ''))}`
      + `${numbersHtml(second, side.computer)}` : '';
  return `${head}${chip}${graphHtml(main, second ?? null)}${legendHtml(main, second)}`
    + `${gapKey}${computerNote}${numbersHtml(main, side.computer)}${compared}`;
}

// ---- the import ----

/** The default name of an import: the headphone's own. */
export const defaultName = (selected) => selected?.variant.name ?? '';

/** The import place: the name field and Save. `state` is `{ name?, status,
 * words?, saved?, suggest? }`: `idle`, `sending`, `refused` (with `words`, and `suggest`, a name to
 * offer when this one is taken) or `saved` (`saved` is the answer `{ name, filters, skipped }`).
 * The field's value is the typed name, else the headphone's own. */
export function importHtml({ selected, state, side }) {
  if (!selected) return '';
  const refused = state.status === 'refused';
  const sending = state.status === 'sending';
  const value = state.name ?? defaultName(selected);
  return '<form class="fi-form fpe-import" data-form="import">'
    + `<label class="fpe-label" for="${IDS.importName}">${esc(WORDS.nameLabel)}</label>`
    + `<input class="fi-input" type="text" id="${IDS.importName}" name="preset-name"`
    + `${attr('value', value)} autocomplete="off" spellcheck="false" enterkeyhint="done"`
    + `${attr('aria-invalid', String(refused))}`
    + `${attr('aria-describedby', refused ? IDS.importWords : null)}${attr('readonly', sending)}>`
    + `<button type="submit" class="fi-btn" id="${IDS.importSave}"`
    + `${attr('aria-disabled', sending ? 'true' : null)}>`
    + `${esc(sending ? WORDS.saving : WORDS.save)}</button></form>`;
}

/** The import's words: what was saved, or the refusal, with the suggestion of a name when this one
 * is taken. They stand in a place of their own, a live region that is put in place and never
 * replaced, so a screen reader hears each change. */
export function importWordsHtml({ selected, state, side }) {
  if (!selected) return '';
  const refused = state.status === 'refused';
  let said = '';
  if (refused) said = esc(state.words);
  if (state.status === 'saved') {
    said = esc(WORDS.saved(state.saved.name, state.saved.filters,
      side.computer ? 'the computer' : side.name));
    if (state.saved.skipped > 0) said += ` ${esc(WORDS.skipped(state.saved.skipped))}`;
  }
  const suggest = refused && state.suggest
    ? ' <button type="button" class="fi-btn" data-act="suggest">'
      + `${esc(WORDS.suggest(state.suggest))}`
      + '</button>' : '';
  const late = state.late ? `${said ? ' ' : ''}${esc(state.late)}` : '';
  return `<p class="fp-note${refused ? ' fp-failed' : ''}" id="${IDS.importWords}">`
    + `${said}${late}${suggest}</p>`;
}

// ---- the pages ----

/** The AutoEq page. */
export function autoEqHtml(m) {
  return `${searchFormHtml({ query: m.query })}`
    + `<div id="${IDS.status}" role="status">${statusHtml(m.search)}</div>`
    + `<div id="${IDS.results}" class="fpe-results">${resultsHtml({
      answer: m.search.answer, selected: m.selected, open: m.open,
    })}</div>`
    + `<div id="${IDS.preview}" class="fpe-preview">${previewHtml(m)}</div>`
    + `<div id="${IDS.previewWords}" role="status">${previewWordsHtml(m)}</div>`
    + `<div id="${IDS.importPlace}">${importHtml({
      selected: m.selected, state: m.importing, side: m.side,
    })}</div>`
    + `<div id="${IDS.importWordsPlace}" role="status">${importWordsHtml({
      selected: m.selected, state: m.importing, side: m.side,
    })}</div>`;
}

/** The upload's name field, Save and the line that says what is missing or what happened. `up` is
 * `{ filename, text, name, status, words, saved }`. Save is `aria-disabled`, not `disabled`, while
 * there is no file: it stays a stop of the keyboard and the line under it says why. */
export function uploadFieldsHtml({ up, side }) {
  const refused = up.status === 'refused';
  const sending = up.status === 'sending';
  const missing = up.text === null || up.text === undefined;
  const value = up.name ?? '';
  return `<label class="fpe-label" for="${IDS.upName}">${esc(WORDS.nameLabel)}</label>`
    + `<input class="fi-input" type="text" id="${IDS.upName}" name="upload-name"`
    + `${attr('value', value)} autocomplete="off" spellcheck="false" enterkeyhint="done"`
    + `${attr('aria-invalid', String(refused && up.field === true))}`
    + `${attr('aria-describedby', refused ? IDS.upWords : null)}${attr('readonly', sending)}>`
    + `<button type="submit" class="fi-btn" id="${IDS.upSave}"`
    + `${attr('aria-disabled', missing || sending ? 'true' : null)}`
    + `${attr('aria-describedby', missing ? IDS.upWords : null)}>`
    + `${esc(sending ? WORDS.saving : WORDS.uploadSave)}</button>`;
}

/** The upload's words line. */
export function uploadWordsHtml({ up, side }) {
  let said = '';
  let failed = false;
  if (up.status === 'refused' || up.status === 'unreadable') {
    said = up.words;
    failed = true;
  } else if (up.status === 'saved') {
    said = WORDS.saved(up.saved.name, up.saved.filters, side.computer ? 'the computer' : side.name);
    if (up.saved.skipped > 0) said += ` ${WORDS.skipped(up.saved.skipped)}`;
  } else if (up.status === 'reading') {
    said = WORDS.fileReading;
  } else if (up.text === null || up.text === undefined) {
    said = WORDS.chooseFirst;
  } else {
    said = WORDS.fileRead(up.filename);
  }
  return `<p class="fp-note${failed ? ' fp-failed' : ''}" id="${IDS.upWords}">`
    + `${esc(said)}</p>`;
}

/** The upload page: the file input is drawn once and never put, so the file stays chosen. */
export function uploadHtml(m) {
  return '<form class="fi-form fpe-upload" data-form="upload">'
    + `<label class="fpe-label" for="${IDS.file}">${esc(WORDS.fileLabel)}</label>`
    + `<input class="fi-input" type="file" id="${IDS.file}" name="eq-file"`
    + ' accept=".txt,text/plain">'
    + `<div id="${IDS.upFields}" class="fpe-fields">${uploadFieldsHtml(m)}</div></form>`
    + `<div id="${IDS.upWordsPlace}" role="status">${uploadWordsHtml(m)}</div>`;
}

// ---- the saved presets ----

/** Why *Overwrite with current…* is refused for `row` on `side`, or `null`. `live` is why there is
 * no curve to save (`null` when there is one). The reasons are the TUI's `u`, in the same words. */
export function overwriteWhy(row, { side, live }) {
  if (side.computer) return WORDS.computerRefusal;
  if (live) return live;
  if (row.built_in) return WORDS.builtInRefusal(row.name);
  if (row.imported) return WORDS.importedRefusal(row.name);
  return null;
}

/** The id of the row `i` of the saved list's elements. */
export const rowId = (i) => eid(`row-${i}`);
export const overwriteId = (i) => eid(`overwrite-${i}`);
export const askId = (i) => eid(`ask-${i}`);

/** The saved presets list. `list` is `{ rows, note? }`: each row `{ name, built_in, imported, on,
 * playable?, why? }`; `note` is what stands in place of the list (a device with no tone
 * control). `live` is why there is no curve to save, or `null`. */
export function savedHtml({ list, side, live }) {
  const target = side.computer ? WORDS.computer : side.name;
  if (list.note) return `<p class="fp-note">${esc(list.note)}</p>`;
  if (list.rows.length === 0) return `<p class="fp-note">${esc(WORDS.noPresets)}</p>`;
  const general = side.computer ? WORDS.computerRefusal : live;
  const items = list.rows.map((row, i) => {
    const why = overwriteWhy(row, { side, live });
    const own = why !== null && why !== general ? why : null;
    const where = row.on ? WORDS.onNow(side.computer ? 'the computer' : side.name)
      : WORDS.inLibrary;
    const tags = (row.built_in ? tag(WORDS.builtIn) : '')
      + (row.imported ? tag(WORDS.imported) : '');
    const ids = [general ? IDS.savedNote : null, own ? `${rowId(i)}-why` : null].filter(Boolean);
    return `<li class="fpe-row${row.on ? ' on' : ''}" id="${rowId(i)}"`
      + ` data-name="${esc(row.name)}">`
      + `<div class="fpe-row-head"><span class="fpe-row-name">${esc(row.name)}</span>${tags}`
      + `<span class="fpe-where">${esc(where)}</span>`
      + `<button type="button" class="fi-btn" id="${overwriteId(i)}" data-act="overwrite"`
      + ` data-i="${i}"${attr('aria-disabled', why !== null ? 'true' : null)}`
      + `${attr('aria-describedby', why !== null ? ids.join(' ') : null)}>`
      + `${esc(WORDS.overwrite)}</button></div>`
      + (row.playable === false && row.why
        ? `<p class="fp-note fpe-why">${esc(row.why)}</p>` : '')
      + `${own ? `<p class="fp-note fpe-why" id="${rowId(i)}-why">${esc(own)}</p>` : ''}`
      + `<div class="fpe-ask" id="${askId(i)}"></div></li>`;
  }).join('');
  return `${heading(WORDS.savedHeading(target))}`
    + `${general ? `<p class="fp-note" id="${IDS.savedNote}">${esc(general)}</p>` : ''}`
    + `<ul class="fpe-rows">${items}</ul>`;
}

/** The line that words what an overwrite did: a refusal in the failed ink. */
export function savedWordsHtml({ words = '', failed = false }) {
  return `<p class="fp-note${failed ? ' fp-failed' : ''} fpe-saved-words" id="${IDS.savedWords}">`
    + `${esc(words)}</p>`;
}

/** The saved page: the list, and the line that words an overwrite. */
export function savedPageHtml(m) {
  return `<div id="${IDS.saved}">${savedHtml(m)}</div>`
    + `<div id="${IDS.savedWordsPlace}" role="status">${savedWordsHtml(m)}</div>`;
}

/** The side list's entries. */
export const entriesOf = () => ENTRIES.map((entry) => ({ ...entry }));

/** The page `page` as markup (the first entry's at the list). */
export function pageHtml(page, m) {
  const at = page ?? ENTRIES[0].id;
  if (at === 'upload') return uploadHtml(m);
  if (at === 'saved') return savedPageHtml(m);
  return autoEqHtml(m);
}

/** The title for `side`: `EQ library · Kara`, `EQ library · This computer`. */
export function titleOf(side) {
  const name = side.computer ? WORDS.computer : side.name;
  return name ? `${WORDS.title} · ${name}` : WORDS.title;
}
