/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The Crossfeed tab of the Sound popup as strings (model in, markup out; no DOM, no state). Every
// number, range, step, model word, description and preset is the host's (`EqView.crossfeed`, the
// view the daemon streams): this file lays them out and draws the two level curves of the view's
// `curve` as they come, and works out nothing of the filter. What the viewer has asked and the
// model has not yet answered is laid over the view by `shownView`.
//
// The page, in order: what turned it off (a note), the output it is for, Model and Preset as two
// menus on one row with the model's one-line description, the picture, then Level, Crossover and
// Delay, each a value with a `−` and a `+`. The on/off switch is the popup's header (`switchHtml`).
// Off is dimmed and still editable. Classes are frame.css's `fpx-*` and `fp-*`, which read the
// `--frame-*` tokens alone.

import { attr, esc } from '../../html.js';
import { dbText, fixedEven, hzWords } from '../../words.js';

/** The id of `part` of the Crossfeed tab. */
export const xid = (part) => `popup-sound-xf-${part}`;

/** The places and controls a wire reaches by id. */
export const IDS = Object.freeze({
  switch: xid('switch'),
  page: xid('page'),
  model: xid('model'),
  preset: xid('preset'),
  words: xid('words'),
  amount: xid('amount'),
  crossover: xid('crossover'),
  delay: xid('delay'),
});

/** The fields a stepper moves, each with its `−` and `+` ids. */
export const FIELDS = Object.freeze(['amount_pct', 'crossover_hz', 'delay_ms']);

/** The id of the `−` (`dir` -1) or `+` (1) of `field`. */
export const stepId = (field, dir) => xid(`${field}-${dir < 0 ? 'less' : 'more'}`);

/** The tab's words. */
export const WORDS = {
  on: 'On',
  off: 'Off',
  switchLabel: 'Crossfeed',
  model: 'Model',
  preset: 'Preset',
  custom: 'Custom',
  amount: 'Level',
  crossover: 'Crossover',
  delay: 'Delay',
  delayNone: 'none',
  own: 'own',
  modelLabel: 'Crossfeed model',
  presetLabel: 'Crossfeed preset',
  none: 'Crossfeed is not offered here.',
  noComputer: 'The computer\'s sound is not on, so there is no crossfeed to set.',
  less: (what) => `Less ${what}`,
  more: (what) => `More ${what}`,
  edited: (from) => `Edited from ${from}`,
  forOutput: (name) => `For ${name}`,
  legendSame: 'Same ear',
  legendOther: 'Other ear',
  pictureLabel: 'Level at each ear by frequency, from 20 Hz to 20 kHz',
  caption: ({ amount, db, hz, ms }) => `Cross path: ${amount}${db ? ` (${db} dB)` : ''}, below ${hz}${ms ? `, ${ms} behind` : ''}.`,
};

const FIELD_WORDS = {
  amount_pct: 'level', crossover_hz: 'crossover', delay_ms: 'delay',
};

/** The view `cf` with what has been asked (`asked`: the fields of a write not yet answered) laid
 * over it, so a press shows at once. The level in dB is not asked, so it goes while the level is
 * pending (a number the host has not given); the delay asked above the model's own is not the
 * model's own. */
export function shownView(cf, asked = {}) {
  if (!cf) return cf;
  const out = { ...cf };
  if (typeof asked.on === 'boolean') out.on = asked.on;
  // a model or preset asked shows the preset's own numbers (the view's list; a model's default
  // comes first) until the host answers
  const model = (cf.models ?? []).find((one) => one.word === (asked.model ?? cf.model));
  if (asked.model !== undefined && asked.model !== cf.model) {
    out.model = asked.model;
    out.delay_settable = model?.delay_settable ?? out.delay_settable;
    out.edited_from = null;
  }
  if (asked.model !== undefined || asked.preset !== undefined) {
    const preset = (model?.presets ?? []).find((one) => one.name === asked.preset)
      ?? (asked.preset === undefined && asked.model !== cf.model
        ? model?.presets?.find((one) => one.name === model.default_preset) : undefined);
    if (preset) {
      Object.assign(out, {
        preset: preset.name, amount_pct: preset.amount_pct, crossover_hz: preset.crossover_hz,
        amount_db: null,
      });
    }
  }
  for (const field of FIELDS) {
    if (Number.isFinite(asked[field])) out[field] = asked[field];
  }
  if (Number.isFinite(asked.amount_pct)) out.amount_db = null;
  if (Number.isFinite(asked.delay_ms)) out.delay_own = asked.delay_ms <= (cf.delay_floor_ms ?? 0);
  if (FIELDS.some((f) => Number.isFinite(asked[f]))) {
    // a number asked makes the values Custom, edited from the preset they were
    if (out.preset !== null && out.preset !== undefined) out.edited_from = out.preset;
    out.preset = null;
  }
  return out;
}

/** The model of the view `cf` named by its wire word, or `undefined`. */
export const modelOf = (cf) => (cf?.models ?? []).find((one) => one.word === cf.model);

/** The level as words: `37 %`, with the level beside it when it is known: `37 % · −8.6 dB`. */
export function amountWords(cf) {
  const pct = `${Math.round(cf.amount_pct)} %`;
  return Number.isFinite(cf.amount_db) ? `${pct} · ${dbText(cf.amount_db)} dB` : pct;
}

/** The delay as words: `0.26 ms`, or `none` for a model that plays none. */
export const delayWords = (cf) => (cf.delay_settable === false ? WORDS.delayNone
  : `${fixedEven(cf.delay_ms, 2)} ms`);

/** The range of `field` for the model in view, `{ min, max, step }`, or `null`. */
export function rangeOf(cf, field) {
  const model = modelOf(cf);
  if (!model) return null;
  if (field === 'amount_pct') return model.amount_pct ?? null;
  if (field === 'crossover_hz') return model.crossover_hz ?? null;
  if (field === 'delay_ms') {
    if (cf.delay_settable === false || !model.delay_settable) return null;
    return { min: cf.delay_floor_ms ?? 0, max: model.delay_max_ms, step: model.delay_step_ms };
  }
  return null;
}

/** The value a step of `dir` (-1 or 1) from the field's value in view asks for, held to the range
 * (the host rounds it to its step again); `null` when the field cannot move that way. */
export function stepped(cf, field, dir) {
  const range = rangeOf(cf, field);
  if (!range || !(range.step > 0)) return null;
  const now = cf[field];
  if (!Number.isFinite(now)) return null;
  const places = 6;
  const raw = Math.round((now + dir * range.step) * 10 ** places) / 10 ** places;
  const next = Math.min(Math.max(raw, range.min), range.max);
  return next === now ? null : next;
}

/** Whether the controls are dim: the host says crossfeed cannot be changed now. */
const blocked = (cf) => typeof cf?.unavailable === 'string' && cf.unavailable !== '';

/** The header's switch: a button that is a switch, with the state in words beside it. */
export function switchHtml(cf) {
  if (!cf) return '';
  const on = cf.on === true;
  return `<button type="button" class="fpx-switch${on ? ' on' : ''}" role="switch"`
    + `${attr('id', IDS.switch)}${attr('aria-checked', String(on))}`
    + `${attr('aria-label', WORDS.switchLabel)}${blocked(cf) ? ' aria-disabled="true"' : ''}`
    + ` data-act="on"><span class="fpx-switch-track" aria-hidden="true">`
    + '<span class="fpx-switch-knob"></span></span>'
    + `<span class="fpx-switch-words">${esc(on ? WORDS.on : WORDS.off)}</span></button>`;
}

const X0 = 0;
const W = 300;
const H = 120;
const DB_TOP = 3;
const DB_BOTTOM = -30;
const xAt = (hz, lo, hi) => X0 + (W * Math.log(hz / lo)) / Math.log(hi / lo);
const yAt = (db) => ((DB_TOP - Math.min(Math.max(db, DB_BOTTOM), DB_TOP)) / (DB_TOP - DB_BOTTOM)) * H;
/** An axis end in words: `20 Hz`, `20 kHz`. */
const axisWords = (hz) => (hz >= 1000 ? `${Number((hz / 1000).toFixed(1))} kHz` : hzWords(hz));
const num = (value) => fixedEven(value, 1);

/** The picture: the same-ear and other-ear curves of the view, the other one's leak shaded, the
 * crossover marked with a dashed line and a dot on the other ear's curve. Nothing without a
 * curve. The axes are fixed (20 Hz to 20 kHz, +3 to -30 dB); a level below the floor sits on it. */
export function pictureHtml(cf) {
  const hz = cf?.curve?.hz ?? [];
  const same = cf?.curve?.same_db ?? [];
  const other = cf?.curve?.other_db ?? [];
  if (hz.length < 2 || same.length !== hz.length || other.length !== hz.length) return '';
  const lo = hz[0];
  const hi = hz.at(-1);
  const line = (db) => hz.map((f, i) => `${i === 0 ? 'M' : 'L'}${num(xAt(f, lo, hi))} ${num(yAt(db[i]))}`)
    .join('');
  const marks = [100, 1000, 10000].filter((f) => f > lo && f < hi)
    .map((f) => `<path class="fpx-grid" d="M${num(xAt(f, lo, hi))} 0V${H}"/>`).join('')
    + [0, -10, -20].map((db) => `<path class="fpx-grid" d="M0 ${num(yAt(db))}H${W}"/>`).join('');
  const leak = `${line(other)}L${W} ${H}L0 ${H}Z`;
  let cross = '';
  const at = cf.crossover_hz;
  if (Number.isFinite(at) && at > lo && at < hi) {
    const i = hz.findIndex((f) => f >= at);
    const a = Math.max(i - 1, 0);
    const b = Math.max(i, 1);
    const t = (Math.log(at / hz[a])) / Math.log(hz[b] / hz[a]);
    const db = other[a] + (other[b] - other[a]) * t;
    cross = `<path class="fpx-cross" d="M${num(xAt(at, lo, hi))} 0V${H}"/>`
      + `<circle class="fpx-dot" cx="${num(xAt(at, lo, hi))}" cy="${num(yAt(db))}" r="3.5"/>`;
  }
  return `<figure class="fpx-picture"><div class="fp-graph fpx-plot" role="img"${attr('aria-label', WORDS.pictureLabel)}>`
    + `<svg class="fpx-svg" viewBox="0 0 ${W} ${H}" aria-hidden="true">${marks}`
    + `<path class="fpx-leak" d="${leak}"/>`
    + `<path class="fpx-other" d="${line(other)}"/><path class="fpx-same" d="${line(same)}"/>`
    + `${cross}</svg></div>`
    + `<div class="fpx-axis" aria-hidden="true"><span>${esc(axisWords(lo))}</span>`
    + `<span>${esc(axisWords(hi))}</span></div>`
    + `<ul class="fpe-legend fpx-legend"><li class="fpe-key"><span class="fpe-swatch fpx-same-key"></span>`
    + `${esc(WORDS.legendSame)}</li><li class="fpe-key"><span class="fpe-swatch fpx-other-key">`
    + `</span>${esc(WORDS.legendOther)}</li></ul></figure>`;
}

/** One row of a value with its steppers. */
function valueRow({
  cf, field, label, id, text, tagWords = '',
}) {
  const word = FIELD_WORDS[field];
  const range = rangeOf(cf, field);
  const grey = range === null;
  const button = (dir, glyph, words) => {
    const off = grey || stepped(cf, field, dir) === null || blocked(cf);
    return `<button type="button" class="fi-btn fpx-step"${attr('id', stepId(field, dir))}`
      + `${attr('data-field', field)}${attr('data-dir', String(dir))}`
      + `${attr('aria-label', words)}${off ? ' aria-disabled="true"' : ''}>${glyph}</button>`;
  };
  return `<div class="fpx-row${grey ? ' fpx-grey' : ''}">`
    + `<span class="fpx-label" id="${id}-label">${esc(label)}</span>`
    + `<span class="fpx-value" id="${id}">${esc(text)}`
    + `${tagWords ? ` <span class="fp-tag">${esc(tagWords)}</span>` : ''}</span>`
    + `<span class="fpx-steps">${button(-1, '−', WORDS.less(word))}`
    + `${button(1, '+', WORDS.more(word))}</span></div>`;
}

/** The whole tab. `m` is `{ cf, refusal }`: the view as shown (`shownView`; none when the host
 * offers no crossfeed) and the words of the last refusal, if any. */
export function pageHtml({ cf, refusal = null, computer = true }) {
  if (!computer) return `<p class="fp-note" id="${IDS.page}">${esc(WORDS.noComputer)}</p>`;
  if (!cf) return `<p class="fp-note" id="${IDS.page}">${esc(WORDS.none)}</p>`;
  const model = modelOf(cf);
  const custom = cf.preset === null || cf.preset === undefined;
  const off = cf.on !== true;
  let out = `<div class="fpx${off ? ' fpx-off' : ''}" id="${IDS.page}">`;
  if (blocked(cf)) out += `<p class="fp-note fp-failed" role="alert">${esc(cf.unavailable)}</p>`;
  if (refusal) out += `<p class="fp-note fp-failed" role="alert" id="${IDS.words}">${esc(refusal)}</p>`;
  if (off && cf.note) out += `<p class="fp-note fpx-turnedoff" role="status">${esc(cf.note)}</p>`;
  if (cf.output) out += `<p class="fp-note fpx-output">${esc(WORDS.forOutput(cf.output))}</p>`;
  out += '<div class="fpx-pair">'
    + `<div class="fpx-field"><span class="fpx-label" id="${IDS.model}-label">${esc(WORDS.model)}</span>`
    + `<button type="button" class="fi-btn fpx-menu"${attr('id', IDS.model)} data-act="model"`
    + ` aria-haspopup="listbox"${attr('aria-labelledby', `${IDS.model}-label ${IDS.model}`)}`
    + `${blocked(cf) ? ' aria-disabled="true"' : ''}>${esc(model?.name ?? cf.model)}</button></div>`
    + `<div class="fpx-field"><span class="fpx-label" id="${IDS.preset}-label">${esc(WORDS.preset)}</span>`
    + `<button type="button" class="fi-btn fpx-menu${custom ? ' fpx-custom' : ''}"`
    + `${attr('id', IDS.preset)} data-act="preset" aria-haspopup="listbox"`
    + `${attr('aria-labelledby', `${IDS.preset}-label ${IDS.preset}`)}`
    + `${blocked(cf) ? ' aria-disabled="true"' : ''}>${esc(custom ? WORDS.custom : cf.preset)}</button>`
    + '</div></div>';
  if (custom && cf.edited_from) {
    out += `<p class="fp-note fpx-edited">${esc(WORDS.edited(cf.edited_from))}</p>`;
  }
  if (model?.description) out += `<p class="fp-note fpx-description">${esc(model.description)}</p>`;
  out += pictureHtml(cf);
  const caption = [
    `${Math.round(cf.amount_pct)} %`,
    Number.isFinite(cf.amount_db) ? dbText(cf.amount_db) : '',
  ];
  out += `<p class="fp-note fpx-caption">${esc(WORDS.caption({
    amount: caption[0],
    db: caption[1],
    hz: hzWords(cf.crossover_hz),
    ms: cf.delay_settable === false ? '' : `${fixedEven(cf.delay_ms, 2)} ms`,
  }))}</p>`;
  out += '<div class="fpx-rows">'
    + valueRow({
      cf, field: 'amount_pct', label: WORDS.amount, id: IDS.amount, text: amountWords(cf),
    })
    + valueRow({
      cf, field: 'crossover_hz', label: WORDS.crossover, id: IDS.crossover,
      text: hzWords(cf.crossover_hz),
    })
    + valueRow({
      cf, field: 'delay_ms', label: WORDS.delay, id: IDS.delay, text: delayWords(cf),
      tagWords: cf.delay_settable !== false && cf.delay_own ? WORDS.own : '',
    })
    + '</div></div>';
  return out;
}

/** The options of the Model menu: every model of the view, the current one marked. */
export const modelOptions = (cf) => (cf.models ?? []).map((one) => ({
  value: one.word, text: one.name, on: one.word === cf.model,
}));

/** The options of the Preset menu: the current model's presets, the current one marked. */
export const presetOptions = (cf) => (modelOf(cf)?.presets ?? []).map((one) => ({
  value: one.name, text: one.name, on: one.name === cf.preset,
}));
