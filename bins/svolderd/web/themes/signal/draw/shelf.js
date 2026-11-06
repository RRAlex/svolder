/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The rest of a device below the first screen (the shelf): in three columns, the band table with
// the values typed or stepped, then the presets (chips, Save as, Rename, Delete), the controls the
// model lists (toggles as pills, choices as chips, balance and the like as scales, readouts), the
// loudness strength and the device's own slots and settings, then the device's facts folded under
// their title. It draws what the device's model has and nothing of the family it belongs to, so an
// amp's shelf is this one too. Pure: the model in, markup out.

import { SHELF } from '../words.js';
import { choiceControl, choices, chip, drawnAbove, idOf, modeRow } from './choices.js';
import { esc } from './esc.js';
import { facts } from './facts.js';
import { fractionOf } from './volume.js';
import { button } from './word.js';

const PARAMS = ['f', 'g', 'q'];

/** The value of a band's parameter between its steppers: a field that takes a typed value. */
function cell(inert, r, param) {
  const at = `data-band="${r.band}" data-param="${param}"`;
  const id = `sg-b${r.band}${param}`;
  const step = (delta, sign, words) => button('band', sign, inert,
    ` id="${id}${delta < 0 ? 'm' : 'p'}" data-step="${delta}" ${at} aria-label="${esc(words)}"`, 'sg-st');
  const head = SHELF.bandHeads[1 + PARAMS.indexOf(param)];
  return `<td data-head="${esc(head)}"><span class="sg-stepper">${step(-1, SHELF.minus, SHELF.lower(r.band, param))}`
    + `<input id="${id}" class="sg-val${inert ? ' quiet' : ''}" type="text" inputmode="decimal" `
    + `autocomplete="off" data-act="band-type" ${at} value="${esc(r[param])}" `
    + `aria-label="${esc(SHELF.field(r.band, param))}"${inert ? ' readonly aria-disabled="true"' : ''}>`
    + `${step(1, SHELF.plus, SHELF.raise(r.band, param))}</span></td>`;
}

/** The band table: each band's frequency, gain and Q. */
function bandTable(device) {
  const t = device.tone;
  if (!t) return '';
  const inert = Boolean(device.inert);
  const mode = t.modes?.find((m) => m.on)?.label ?? '';
  const heads = SHELF.bandHeads.map((h) => `<th scope="col">${esc(h)}</th>`).join('');
  const rows = (t.bands?.rows ?? []).map((r) => `<tr data-band="${r.band}"><th scope="row">${esc(r.band)}</th>`
    + `${PARAMS.map((p) => cell(inert, r, p)).join('')}</tr>`).join('');
  return `<h2 class="sg-lbl">${esc(SHELF.bands)}${mode ? ` · ${esc(mode)}` : ''}</h2>`
    + `<table id="sg-bands-table" class="sg-bands${t.bands?.faint ? ' faint' : ''}">`
    + `<thead><tr>${heads}</tr></thead><tbody>${rows}</tbody></table>`
    + `<p class="sg-note">${esc(SHELF.bandNote)}</p>`;
}

/** The tone presets: a chip for each, what is on, and the ways to save, rename or delete. */
function presets(device) {
  const p = device.presets;
  if (!p) return '';
  const inert = Boolean(device.inert);
  const quiet = inert || p.recall === false;
  const { status } = p;
  const name = status?.name === null || status?.name === undefined ? '' : `<i>${esc(status.name)}</i>`;
  const chips = (p.rows ?? []).map((r, i) => button('preset', r.name, quiet,
    ` id="sg-pr-${i}" data-name="${esc(r.name)}" aria-pressed="${r.on === true}"`,
    `sg-chip${r.on ? ' on' : ''}${r.built_in ? '' : ' own'}`)).join('');
  const own = (p.rows ?? []).some((r) => !r.built_in);
  const writes = p.editable && !inert
    ? button('preset-save', SHELF.saveAs, false, ' id="sg-preset-save"', 'sg-btn sg-pill')
      + (own ? button('preset-rename', SHELF.rename, false, ' id="sg-preset-rename"', 'sg-btn sg-pill')
        + button('preset-delete', SHELF.remove, false, ' id="sg-preset-delete"', 'sg-btn sg-pill danger') : '')
    : '';
  return `<div id="sg-presets" class="sg-box2"><h2 class="sg-lbl">${esc(SHELF.presets)}</h2>`
    + `<div class="sg-set">${chips}</div>`
    + `<p class="sg-status">${esc(status?.lead ?? '')}${name}${esc(status?.tail ?? '')}</p>`
    + `${writes ? `<div class="sg-set">${writes}</div>` : ''}`
    + `${p.note ? `<p class="sg-note">${esc(p.note)}</p>` : ''}<div class="sg-ask"></div></div>`;
}

/** A switch: its label and a pill that reads `On`, `Off` or a dash where the device has not said. */
function toggle(c, inert) {
  const said = c.on === true ? SHELF.on : c.on === false ? SHELF.off : SHELF.unknown;
  return `<div class="sg-ctl"><span class="sg-lbl">${esc(c.label)}</span><div class="sg-set">`
    + `${chip('toggle', said, inert, ` id="${idOf('sg-t', c.key)}" data-key="${esc(c.key)}"`, c.on === true)}`
    + '</div></div>';
}

/** A level: its label and value, a scale with a knob where the device reads and a tick at its
 * centre. A tap on the value writes the centre where the level has one. */
function slider(c, inert) {
  const read = typeof c.raw === 'number';
  const [left, right] = c.ends ?? [];
  const lo = { lo: c.lo, hi: c.hi };
  const knob = read ? `<span class="sg-sknob" data-at="${fractionOf(lo, c.raw).toFixed(5)}"></span>` : '';
  const centre = typeof c.centre === 'number'
    ? `<span class="sg-sctr" data-at="${fractionOf(lo, c.centre).toFixed(5)}"></span>` : '';
  const value = typeof c.centre === 'number'
    ? button('centre', c.text, inert, ` id="${idOf('sg-ce', c.key)}" data-key="${esc(c.key)}"`, 'sg-link sg-sval')
    : `<span class="sg-sval">${esc(c.text)}</span>`;
  const end = (text) => (text === undefined ? '' : `<span class="sg-end">${esc(text)}</span>`);
  return `<div class="sg-level"><div class="sg-lrow"><span class="sg-lbl">${esc(c.label)}</span>${value}</div>`
    + `<div class="sg-sbox">${end(left)}`
    + `<div id="${idOf('sg-s', c.key)}" class="sg-scale${inert ? ' quiet' : ''}" role="slider" tabindex="0" `
    + `data-key="${esc(c.key)}" aria-label="${esc(c.label)}" aria-valuemin="${esc(c.lo)}" `
    + `aria-valuemax="${esc(c.hi)}"${read ? ` aria-valuenow="${esc(c.raw)}"` : ''} `
    + `aria-valuetext="${esc(c.text)}"${inert ? ' aria-disabled="true"' : ''}>${centre}${knob}</div>`
    + `${end(right)}</div></div>`;
}

/** A readout: its label and its words, faint where the device says it is of no account. */
function readout(c) {
  return `<div class="sg-ctl sg-read"><span class="sg-lbl">${esc(c.label)}</span>`
    + `<span data-readout="${esc(c.key)}" class="sg-rv${c.tone === 'faint' ? ' faint' : ''}">${esc(c.text)}</span></div>`;
}

/** One control, by its kind. */
export function control(c, inert) {
  switch (c.kind) {
    case 'choice': return choiceControl(c, inert);
    case 'toggle': return toggle(c, inert);
    case 'slider': return slider(c, inert);
    default: return readout(c);
  }
}

/** The device's power: the switch with the word the device says (`On`, `Standby`, `Warming up`), then
 * its switches as pills and its readouts. Nothing for a device without power. */
export function power(device) {
  const p = device.power;
  if (!p) return '';
  const inert = Boolean(device.inert);
  const said = p.word ?? (p.on === true ? SHELF.on : p.on === false ? SHELF.off : SHELF.unknown);
  const main = `<div class="sg-ctl"><span class="sg-lbl">${esc(SHELF.power)}</span><div class="sg-set">`
    + `${chip('power', said, inert, ` id="sg-power" data-key="${esc(p.key)}"`, p.on === true)}</div></div>`;
  const toggles = (p.toggles ?? []).map((c) => `<div class="sg-ctl"><span class="sg-lbl">${esc(c.label)}</span>`
    + `<div class="sg-set">${chip('power-toggle', c.on === true ? SHELF.on : c.on === false ? SHELF.off : SHELF.unknown,
      inert, ` id="${idOf('sg-pt', c.key)}" data-key="${esc(c.key)}"`, c.on === true)}</div></div>`).join('');
  return `<div id="sg-power-box" class="sg-box2">${main}${toggles}${(p.readouts ?? []).map(readout).join('')}</div>`;
}

/** The controls the model lists but for those the first screen draws. */
function controls(device) {
  const inert = Boolean(device.inert);
  const all = (device.controls ?? []).filter((c) => !drawnAbove(c));
  if (all.length === 0) return '';
  return `<div id="sg-controls" class="sg-box2"><h2 class="sg-lbl">${esc(SHELF.controls)}</h2>`
    + `${all.map((c) => control(c, inert)).join('')}</div>`;
}

/** The loudness strength, with its two steppers, while the tone is on Loudness. */
function loudness(device) {
  const t = device.tone;
  if (!t?.loud) return '';
  const inert = Boolean(device.inert);
  const step = (delta, sign, words) => button('loudness', sign, inert,
    ` id="sg-loud${delta < 0 ? 'm' : 'p'}" data-step="${delta}" aria-label="${esc(words)}"`, 'sg-st');
  return `<div id="sg-loud" class="sg-box2"><div class="sg-ctl"><span class="sg-lbl">${esc(SHELF.loudness)}</span>`
    + `<span class="sg-stepper">${step(-1, SHELF.minus, SHELF.lowerLoudness)}`
    + `<span class="sg-rv">${esc(t.strength ?? SHELF.unknown)}</span>`
    + `${step(1, SHELF.plus, SHELF.raiseLoudness)}</span></div></div>`;
}

/** The device's own presets: its slots to recall or write over, and its built-in settings. */
function devicePresets(device) {
  const d = device.device_presets;
  if (!d) return '';
  const inert = Boolean(device.inert);
  const quiet = inert || d.recall === false;
  const slots = (d.slots ?? []).map((s) => button('slot', s.name, quiet,
    ` id="sg-sl-${esc(s.slot)}" data-slot="${esc(s.slot)}"`, 'sg-chip')).join('');
  const save = d.save && !inert
    ? `<div class="sg-set">${button('slot-save-list', SHELF.saveBands, false, ' id="sg-slot-save"', 'sg-btn sg-pill')}</div>` : '';
  const built = (d.builtins ?? []).map((b, i) => button('setting', b.name, quiet,
    ` id="sg-st-${i}" data-name="${esc(b.name)}" aria-pressed="${b.on === true}"`,
    `sg-chip${b.on ? ' on' : ''}`)).join('');
  return `<div id="sg-dpresets" class="sg-box2"><h2 class="sg-lbl">${esc(SHELF.onDevice)}</h2>`
    + `<div class="sg-set">${slots}</div>${save}`
    + `<h2 class="sg-lbl">${esc(SHELF.builtIn)}</h2><div class="sg-set">${built}</div>`
    + '<div class="sg-ask"></div></div>';
}

/**
 * The shelf of `device` (the model of the device the hop chosen is): two columns, the band table
 * (with a DAC's input under it), then the mode (with the loudness strength), power, presets,
 * controls and the device's own presets; the facts across the bottom. On a phone a DAC's input and
 * presets come first, right under the mode that ends the first screen. Nothing for no device. `own`
 * names what the open area draws for the device itself and the shelf leaves out:
 * `{ power, identify }`; `choices` marks a DAC's shelf, `phone` the phone's order.
 */
export function drawShelf(page, device, view, own = {}) {
  if (!device) return '';
  const input = own.choices ? choices(device) : '';
  const picks = input ? `<section id="sg-choices" class="sg-side">${input}</section>` : '';
  const facts_ = `<section id="sg-facts" class="sg-col">${facts(page, device, view, own)}</section></div>`;
  const power_ = own.power ? '' : power(device);
  if (own.choices && own.phone) {
    return '<div class="sg-cols">'
      + `<section id="sg-up" class="sg-col">${picks}${presets(device)}</section>`
      + `<section id="sg-bands" class="sg-col">${bandTable(device)}</section>`
      + `<section id="sg-mid" class="sg-col">${power_}${controls(device)}${devicePresets(device)}</section>`
      + facts_;
  }
  return '<div class="sg-cols">'
    + `<section id="sg-bands" class="sg-col">${bandTable(device)}${picks}</section>`
    + `<section id="sg-mid" class="sg-col">${own.choices ? modeRow(device) : ''}${power_}${presets(device)}`
    + `${controls(device)}${own.choices ? '' : loudness(device)}${devicePresets(device)}</section>`
    + facts_;
}
