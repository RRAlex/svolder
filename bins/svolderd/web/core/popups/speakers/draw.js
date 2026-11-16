/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The Virtual speakers tab of the Sound popup as strings (model in, markup out; no DOM, no
// state). Every number is the daemon's: the computer EQ's `speakers` view (the chosen set, its
// licence, angles, level, curve and timing, and the fetched sets) and the catalogue's list of
// sets (`api/v1/speakers/catalogue`). This file lays them out.
//
// The page, in order: what turned it off or refused (notes), the three views of the chosen set
// (Arrivals, Directions, Decay: side by side on a wide window, one at a time with chips below it),
// the two choices (With room · BRIR, No room · HRTF) with the licence line, the response with its
// facts, the sets as a list (fetch, use, forget), and the person's own file. The on/off switch is
// the popup's header (`switchHtml`). Off is dimmed and still editable. Classes are frame.css's
// `fps-*`, `fpx-*` and `fp-*`, which read the `--frame-*` tokens alone.

import { attr, esc } from '../../html.js';
import { fixedEven } from '../../words.js';
import {
  PICTURE_WORDS, anglesOf, arrivalsSvg, decaySvg, directionsSvg, earDifference, levelWords,
  responseSvg,
} from './pictures.js';

/** The id of `part` of the Virtual speakers tab. */
export const sid = (part) => `popup-sound-sp-${part}`;

/** The places and controls a wire reaches by id. */
export const IDS = Object.freeze({
  switch: sid('switch'),
  page: sid('page'),
  words: sid('words'),
  import: sid('import'),
  label: sid('label'),
  views: sid('views'),
});

/** The id of the row of the set `id`. */
export const rowId = (id) => sid(`set-${id}`);
/** The id of the choice `room` (`with` or `no`). */
export const roomId = (room) => sid(`room-${room}`);
/** The id of the chip `n`. */
export const chipId = (n) => sid(`chip-${n}`);
/** The id of the filter pill `filter` (`all`, `with`, `no`). */
export const pillId = (filter) => sid(`pill-${filter}`);

/** What each choice is, in a line under its name. The names and the technical terms (`With room ·
 * BRIR`) are the daemon's (`room_name` and `term` of a set), never written here. */
export const ABOUT = Object.freeze({
  with: 'The speakers in a measured room',
  no: 'The same speakers, heard in the open',
});

/** The two choices' words as the data gives them: `{ with: { name, term }, no: { name, term } }`,
 * each from a catalogue row, then a set of the view's list, then the set chosen; a choice no data
 * names is left out. */
export function namesOf(sp, cat) {
  const out = {};
  const take = (one) => {
    if (one && (one.room === 'with' || one.room === 'no') && one.room_name && one.term && !out[one.room]) {
      out[one.room] = { name: one.room_name, term: one.term };
    }
  };
  (cat?.sets ?? []).forEach(take);
  (sp?.sets ?? []).forEach(take);
  take(sp);
  return out;
}

/** The three views, in order. */
export const VIEWS = Object.freeze([
  { id: 'arrivals', label: PICTURE_WORDS.arrivals },
  { id: 'directions', label: PICTURE_WORDS.directions },
  { id: 'decay', label: PICTURE_WORDS.decay },
]);

/** The page's words. */
export const WORDS = {
  on: 'On',
  off: 'Off',
  switchLabel: 'Virtual speakers',
  none: 'Virtual speakers are not offered here.',
  noComputer: 'The computer\'s sound is not on, so there are no virtual speakers to set.',
  noSet: 'No set is chosen. Fetch one below, then choose it.',
  choice: 'Virtual speakers',
  pictures: 'Picture',
  sets: 'Sets',
  showSets: 'Show sets',
  all: 'All',
  fetchOff: 'This daemon reaches nothing, so no set can be fetched.',
  noCatalogue: 'The list of sets could not be read.',
  loadingCatalogue: 'Reading the list of sets…',
  inUse: 'in use',
  chosen: 'chosen',
  fetched: 'fetched',
  fetching: 'Fetching…',
  importing: 'Importing…',
  own: 'own',
  fetch: 'Fetch',
  use: 'Use',
  forget: 'Forget',
  cancel: 'Cancel',
  damaged: (why) => `damaged: ${why}`,
  noneOf: (name) => `No ‘${name}’ set is fetched yet. Fetch one below.`,
  exclusive: 'Turning these on turns crossfeed off.',
  use1: (label) => `Use ${label}`,
  forget1: (label) => `Forget ${label}`,
  fetch1: (label) => `Fetch ${label}`,
  cancel1: (label) => `Cancel fetching ${label}`,
  fileTitle: 'Use your own file',
  fileAbout: 'A SOFA free-field HRIR, or a WAV with 4 channels (left and right speaker, each to both ears) or the 14-channel 7.1 layout.',
  fileName: 'Name',
  fileChoose: 'Choose file…',
  fileRoom: 'It is a set',
  fileTooBig: (mib) => `A file over ${mib} MiB imports with svolder host speakers import.`,
  fileRead: 'The file could not be read.',
  licence: 'Licence',
  source: 'source page',
  setLabel: 'Set',
  angles: 'Angles found',
  length: 'Length',
  level: 'Level',
  pictureNone: 'Not measured for this set.',
  caption: {
    arrivals: {
      with: 'Direct sound, then the room\'s reflections and tail, in each ear.',
      no: 'Two direct arrivals; no room, so nothing follows.',
    },
    directions: {
      with: 'The two measured angles; the bar is how much louder the near ear is.',
      no: 'The two measured angles; the bar is how much louder the near ear is.',
    },
    decay: {
      with: 'Each band\'s level; the dashed line is the end.',
      no: 'Each band\'s level; the dashed line is the end.',
    },
  },
};

/** The text of a view's chip or panel `id`. */
const viewLabel = (id) => VIEWS.find((one) => one.id === id).label;

/** The view `sp` with what has been asked (`asked`: `on`, `set`) laid over it, so a press shows at
 * once. A set asked that the view's list holds shows that set's own words until the host
 * answers. */
export function shownView(sp, asked = {}) {
  if (!sp) return sp;
  const out = { ...sp };
  if (typeof asked.on === 'boolean') out.on = asked.on;
  if (typeof asked.set === 'string' && asked.set !== sp.set) {
    const one = (sp.sets ?? []).find((set) => set.id === asked.set);
    if (one) {
      Object.assign(out, {
        set: one.id, room: one.room, room_name: one.room_name, term: one.term, label: one.label,
        licence: one.licence, credit: one.credit, source_url: one.source_url,
      });
    }
  }
  return out;
}

/** Whether the controls are dim: the host says the speakers cannot be changed now. */
const blocked = (sp) => typeof sp?.unavailable === 'string' && sp.unavailable !== '';

/** The header's switch: on only means something with a set chosen. */
export function switchHtml(sp) {
  if (!sp) return '';
  const on = sp.on === true;
  const stuck = blocked(sp) || (!on && !sp.set);
  return `<button type="button" class="fpx-switch${on ? ' on' : ''}" role="switch"`
    + `${attr('id', IDS.switch)}${attr('aria-checked', String(on))}`
    + `${attr('aria-label', WORDS.switchLabel)}${stuck ? ' aria-disabled="true"' : ''}`
    + ` data-act="on"><span class="fpx-switch-track" aria-hidden="true">`
    + '<span class="fpx-switch-knob"></span></span>'
    + `<span class="fpx-switch-words">${esc(on ? WORDS.on : WORDS.off)}</span></button>`;
}

/** `bytes` as `8.4 MB`; nothing for none. */
export const sizeWords = (bytes) => (bytes > 0 ? `${fixedEven(bytes / 1e6, 1)} MB` : '');

/** The room word of a set row: `With room · BRIR`, the terms always in view. */
export function kindHtml(room, names) {
  const one = names?.[room];
  if (!one) return '';
  return `${esc(one.name)} <span class="fps-suf">· ${esc(one.term)}</span>`;
}

/** A link to `url` when it is a web address; the words alone when it is anything else. */
function sourceHtml(url) {
  if (!/^https?:\/\//i.test(url ?? '')) return '';
  return `<a class="fps-link-out" href="${esc(url)}" target="_blank" rel="noopener noreferrer">`
    + `${esc(WORDS.source)}</a>`;
}

/** The licence line of the chosen set: its SPDX words, the credit and the source page. */
export function licenceHtml(sp) {
  if (!sp?.set || !sp.licence) return '';
  const credit = sp.credit ? ` · ${esc(sp.credit)}` : '';
  return `<div class="fps-licence"><span><code>${esc(sp.licence)}</code>${credit}</span>`
    + `${sourceHtml(sp.source_url)}</div>`;
}

/** The two choices, the room of the chosen set marked. */
export function choiceHtml(sp, names) {
  if (!names?.with && !names?.no) return '';
  const at = sp?.room ?? null;
  const card = (room) => {
    if (!names?.[room]) return '';
    return `<button type="button" class="fps-kind${at === room ? ' on' : ''}" role="radio"`
      + `${attr('id', roomId(room))}${attr('aria-checked', String(at === room))}`
      + `${attr('data-act', 'room')}${attr('data-room', room)}${blocked(sp) ? ' aria-disabled="true"' : ''}>`
      + `<b><i class="fps-radio" aria-hidden="true"></i>${kindHtml(room, names)}</b>`
      + `<small>${esc(ABOUT[room])}</small></button>`;
  };
  return `<div><p class="fpx-label">${esc(WORDS.choice)}</p>`
    + `<div class="fps-kinds" role="radiogroup"${attr('aria-label', WORDS.choice)}>${card('with')}${card('no')}</div>`
    + '</div>';
}

/** The facts of the chosen set. */
function factsHtml(sp) {
  const length = sp.cut ? `cut to ${fixedEven(sp.taps_ms / 1000, 1)} s` : `${fixedEven(sp.taps_ms, 1)} ms`;
  const rows = [
    [WORDS.setLabel, `${esc(sp.label)}${sp.on ? ` · ${esc(WORDS.inUse)}` : ''}`],
    sp.angles ? [WORDS.angles, esc(sp.angles)] : null,
    Number.isFinite(sp.taps_ms) && sp.taps_ms > 0 ? [WORDS.length, esc(length)] : null,
    Number.isFinite(sp.loudness_db) ? [WORDS.level, esc(levelWords(sp.loudness_db))] : null,
  ].filter(Boolean);
  return `<dl class="fps-facts">${rows.map(([k, v]) => `<dt>${esc(k)}</dt><dd>${v}</dd>`).join('')}</dl>`;
}

/** The three views, side by side on a wide window and one at a time (the chip `chip`) below it.
 * A panel with no data says so in one line. */
export function viewsHtml({ sp, angles, chip }) {
  const room = sp.room === 'with' ? 'with' : 'no';
  const timing = sp.timing ?? null;
  const svgs = {
    arrivals: arrivalsSvg(timing?.arrivals),
    directions: directionsSvg(angles, earDifference(sp.curve)),
    decay: decaySvg(timing?.decay, sp.cut === true),
  };
  const chips = `<div class="fps-chips" role="tablist"${attr('aria-label', WORDS.pictures)}>`
    + VIEWS.map((one, i) => `<button type="button" class="fps-chip${i === chip ? ' on' : ''}" role="tab"`
      + `${attr('id', chipId(i))}${attr('aria-selected', String(i === chip))}`
      + `${attr('aria-controls', sid(`panel-${one.id}`))}${attr('data-act', 'chip')}`
      + `${attr('data-chip', String(i))}>${esc(one.label)}</button>`).join('')
    + '</div>';
  const panels = VIEWS.map((one, i) => {
    const svg = svgs[one.id];
    const body = svg || `<p class="fps-none">${esc(WORDS.pictureNone)}</p>`;
    const caption = svg ? `<p class="fps-caption">${esc(WORDS.caption[one.id][room])}</p>` : '';
    return `<section class="fps-panel${i === chip ? ' on' : ''}" role="tabpanel"`
      + `${attr('id', sid(`panel-${one.id}`))}${attr('aria-labelledby', chipId(i))}>`
      + `<h4>${esc(viewLabel(one.id))}</h4>${body}${caption}</section>`;
  }).join('');
  return `<div class="fps-views" id="${IDS.views}">${chips}<div class="fps-panels">${panels}</div></div>`;
}

/** One row of the sets list. `row` is a catalogue set; `ui` is `{ chosen, busy, fetching }` (the
 * chosen set, the id of the job running or `null`, whether this daemon fetches). */
function rowHtml(row, ui) {
  const inUse = row.id === ui.chosen;
  const running = ui.busy === row.id;
  const other = ui.busy !== null && !running;
  const button = (act, words, name, extra = '') => `<button type="button" class="fi-btn fps-act${extra}"`
    + `${attr('data-act', act)}${attr('data-id', row.id)}${attr('aria-label', name)}`
    + `${other ? ' aria-disabled="true"' : ''}>${esc(words)}</button>`;
  let state;
  if (running) {
    state = `<span class="fps-state">${esc(ui.kind === 'forget' ? '' : WORDS.fetching)}</span>`
      + (ui.kind === 'fetch' ? button('cancel', WORDS.cancel, WORDS.cancel1(row.label)) : '');
  } else if (row.damaged) {
    state = `<span class="fps-state fps-warn">${esc(WORDS.damaged(row.damaged))}</span>`
      + button('forget', WORDS.forget, WORDS.forget1(row.label));
  } else if (inUse) {
    // The chosen set may be forgotten while it does not play: the daemon refuses it while it does.
    state = `<span class="fps-state fps-chk">✓ ${esc(WORDS.chosen)}</span>`
      + button('forget', WORDS.forget, WORDS.forget1(row.label));
  } else if (row.fetched) {
    state = button('use', WORDS.use, WORDS.use1(row.label), ' fps-primary')
      + button('forget', WORDS.forget, WORDS.forget1(row.label));
  } else {
    const off = ui.fetching === false;
    state = `<button type="button" class="fi-btn fps-act fps-primary"${attr('data-act', 'fetch')}`
      + `${attr('data-id', row.id)}${attr('aria-label', WORDS.fetch1(row.label))}`
      + `${other || off ? ' aria-disabled="true"' : ''}>`
      + `${esc(WORDS.fetch)}${row.bytes > 0 ? ` <span class="fps-suf">${esc(sizeWords(row.bytes))}</span>` : ''}</button>`;
  }
  const sub = [kindHtml(row.room, ui.names), row.own ? esc(WORDS.own) : '',
    row.fetched ? '' : esc(sizeWords(row.bytes)), row.credit ? esc(row.credit) : '']
    .filter(Boolean).join(' · ');
  return `<li class="fps-row${inUse ? ' on' : ''}"${attr('id', rowId(row.id))}${attr('data-set', row.id)}>`
    + `<span class="fps-mark" aria-hidden="true">${inUse ? '●' : ''}</span>`
    + `<span class="fps-name">${esc(row.label)}</span><span class="fps-acts">${state}</span>`
    + `<span class="fps-sub">${sub}</span><span class="fps-lic">${esc(row.licence)}</span></li>`;
}

/** The sets, filtered by `filter` (`all`, `with`, `no`). */
export function setsHtml({
  cat, filter, chosen, busy, kind, names = namesOf(null, cat),
}) {
  const pill = (id, words) => `<button type="button" class="fp-pill${filter === id ? ' on' : ''}" role="radio"`
    + `${attr('id', pillId(id))}${attr('aria-checked', String(filter === id))}${attr('data-act', 'filter')}`
    + `${attr('data-filter', id)}>${esc(words)}</button>`;
  const pills = `<div class="fp-pills" role="radiogroup"${attr('aria-label', WORDS.showSets)}>`
    + `${pill('all', WORDS.all)}${['with', 'no'].filter((id) => names[id]).map((id) => pill(id, names[id].name)).join('')}</div>`;
  let list;
  if (!cat) {
    list = `<p class="fp-note">${esc(WORDS.loadingCatalogue)}</p>`;
  } else {
    const rows = cat.sets.filter((one) => filter === 'all' || one.room === filter);
    list = rows.length === 0 ? '' : `<ul class="fps-list">${rows.map((row) => rowHtml(row, {
      chosen, busy, kind, fetching: cat.fetching, names,
    })).join('')}</ul>`;
  }
  const note = cat && cat.fetching === false
    ? `<p class="fp-note">${esc(cat.note || WORDS.fetchOff)}</p>`
    : (cat?.note ? `<p class="fp-note">${esc(cat.note)}</p>` : '');
  return `<div><p class="fpx-label">${esc(WORDS.sets)}</p>${pills}${list}${note}</div>`;
}

/** The person's own file: a name, the choice it belongs under, and the button that opens the file
 * dialog. */
export function importHtml({ room, importing, names = {} }) {
  const radio = (id) => `<button type="button" class="fp-pill${room === id ? ' on' : ''}" role="radio"`
    + `${attr('aria-checked', String(room === id))}${attr('data-act', 'import-room')}`
    + `${attr('data-room', id)}>${kindHtml(id, names)}</button>`;
  return `<div class="fps-drop"><div class="fps-drop-text"><b>${esc(WORDS.fileTitle)}</b>`
    + `<p>${esc(WORDS.fileAbout)}</p></div>`
    + `<div class="fps-drop-room" role="radiogroup"${attr('aria-label', WORDS.fileRoom)}>`
    + `${['no', 'with'].filter((id) => names[id]).map(radio).join('')}</div>`
    + `<input type="text" class="fi-input fps-label" id="${IDS.label}"${attr('placeholder', WORDS.fileName)}`
    + ` maxlength="80"${attr('aria-label', WORDS.fileName)}>`
    + `<button type="button" class="fi-btn fps-primary" id="${IDS.import}" data-act="import"`
    + `${importing ? ' aria-disabled="true"' : ''}>${esc(importing ? WORDS.importing : WORDS.fileChoose)}</button>`
    + '</div>';
}

/** The whole tab. `m` is `{ sp, angles, chip, cat, filter, busy, kind, refusal, computer,
 * importRoom, importing }`. */
export function pageHtml(m) {
  const {
    sp, cat = null, filter = 'all', chip = 0, busy = null, kind = null, refusal = null,
    computer = true, importRoom = 'no', importing = false, crossfeedOn = false,
  } = m;
  const names = namesOf(sp, cat);
  if (!computer) return `<p class="fp-note" id="${IDS.page}">${esc(WORDS.noComputer)}</p>`;
  if (!sp) return `<p class="fp-note" id="${IDS.page}">${esc(WORDS.none)}</p>`;
  const off = sp.on !== true;
  let out = `<div class="fps${off ? ' fps-off' : ''}" id="${IDS.page}">`;
  if (blocked(sp)) out += `<p class="fp-note fp-failed" role="alert">${esc(sp.unavailable)}</p>`;
  if (refusal) out += `<p class="fp-note fp-failed" role="alert" id="${IDS.words}">${esc(refusal)}</p>`;
  if (off && sp.note) out += `<p class="fp-note fpx-turnedoff" role="status">${esc(sp.note)}</p>`;
  if (off && crossfeedOn) out += `<p class="fp-note fps-exclusive">${esc(WORDS.exclusive)}</p>`;
  if (sp.set) {
    const own = (cat?.sets ?? []).find((one) => one.id === sp.set);
    out += viewsHtml({ sp, angles: anglesOf(own?.angles, sp.angles), chip });
    out += '<div class="fps-mid"><div class="fps-cards">'
      + `${choiceHtml(sp, names)}${licenceHtml(sp)}</div>`
      + `<div class="fps-cards"><div><p class="fpx-label">Response, level at each ear in dB</p>`
      + `<div class="fps-plot">${responseSvg(sp.curve)}</div>`
      + `<ul class="fpe-legend fps-legend"><li class="fpe-key"><span class="fpe-swatch fps-same-key"></span>${esc(PICTURE_WORDS.sameEar)}</li>`
      + `<li class="fpe-key"><span class="fpe-swatch fps-other-key"></span>${esc(PICTURE_WORDS.otherEar)}</li></ul></div>`
      + `${factsHtml(sp)}</div></div>`;
  } else {
    out += `<p class="fp-note">${esc(WORDS.noSet)}</p>${choiceHtml(sp, names)}`;
  }
  out += setsHtml({
    cat, filter, chosen: sp.set, busy, kind, names,
  });
  out += importHtml({ room: importRoom, importing, names });
  return `${out}</div>`;
}
