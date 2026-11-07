/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The computer's side of the page, drawn in Signal's own parts: choosing the computer's hop on the
// line is the flip, and the area below it is the DAC's usual screen acting on the computer: the
// volume in percent, the tone curve with a dot on each point, the mode (Off | EQ), the points'
// table, the presets and, last, the short facts of the route that are not controls (the sound card
// and its format, the other streams, the verdict). Nothing below the line names the computer.
// Pure: the core's view of the computer (`core.computer.volumeOf` and `toneOf`) in, markup out.
// The elements carry their own `data-act` names (`cvol-*`, `cmute`, `cmode`, `cpreset`, `cpoint`,
// `cadd`, `cremove`), which only computer-wire.js acts on.

import {
  COMPUTER, POINTS, SHELF, WORDS, rateWords,
} from '../words.js';
import { chip } from './choices.js';
import { esc } from './esc.js';
import { VIEW_HEIGHT, VIEW_WIDTH, plot } from './tone.js';
import { button } from './word.js';

/** The rows of the points' table in view before it scrolls. */
export const ROWS_SHOWN = 6;

const clamp = (value, low, high) => Math.min(Math.max(value, low), high);

/** dB with one decimal and a true minus; no sign on zero. */
function dbText(db) {
  const text = Math.abs(db).toFixed(1);
  return db < 0 && text !== '0.0' ? `−${text}` : text;
}

/** A frequency in Hz: whole above 100, one decimal below, no trailing zero. */
export const hzText = (f) => `${Number(f.toFixed(f < 100 ? 1 : 0))} Hz`;
/** A gain with its sign. */
export const gainText = (g) => `${g > 0 && g.toFixed(1) !== '0.0' ? '+' : ''}${dbText(g)} dB`;
/** A Q with two decimals. */
export const qText = (q) => q.toFixed(2);

/** A marker of the core's (`toneOf`, on the summed curve at the point's frequency, in the plot's units) as
 * fractions of the plot (`x`, `y` from 0 to 1, from the top left). */
export const fractionOf = (marker) => ({ n: marker.n, x: marker.x / VIEW_WIDTH, y: marker.y / VIEW_HEIGHT });

/** The attributes of a control the viewer may not use (they are tapped all the same: the core says
 * why nothing happens). */
const lock = (inert) => (inert ? ' aria-disabled="true"' : '');

/** The volume: the level large in percent (`—` until it is read), `−`, `Mute` and `+`, and the
 * track whose knob is a slider. `volume` is `core.computer.volumeOf`. */
export function computerVolume(volume, inert) {
  const dead = inert || volume.inert;
  const fraction = volume.pct === null ? 0 : clamp(volume.pct / 100, 0, 1);
  const at = fraction.toFixed(5);
  const mute = button('cmute', volume.muted ? WORDS.unmute : WORDS.mute, dead,
    ` id="sg-cmute" aria-pressed="${volume.muted}"`, 'sg-btn sg-pill');
  const note = volume.boosted ? `<p class="sg-note" id="sg-cboost">${esc(POINTS.boosted)}</p>` : '';
  return `<span class="sg-lbl">${esc(WORDS.volume)}</span>`
    + `<div id="sg-cbig" class="sg-big${volume.muted ? ' sg-muted' : ''}"><span class="sg-num">${esc(volume.text)}</span>`
    + `<small>${esc(volume.pct === null ? '' : volume.unit)}</small></div>`
    + '<div class="sg-vrow">'
    + `${button('cvol-down', '−', dead, ` id="sg-cvdown" aria-label="${esc(WORDS.quieter)}"`, 'sg-btn')}`
    + `${mute}`
    + `${button('cvol-up', '+', dead, ` id="sg-cvup" aria-label="${esc(WORDS.louder)}"`, 'sg-btn')}`
    + '</div>'
    + `<div id="sg-cvtrack" class="sg-vtrack${dead ? ' quiet' : ''}" role="slider" tabindex="0"`
    + ` aria-label="${esc(WORDS.volume)}" aria-valuemin="0" aria-valuemax="100"`
    + `${volume.pct === null ? '' : ` aria-valuenow="${volume.pct}"`} aria-valuetext="${esc(volume.pct === null ? WORDS.unread : `${volume.pct} %`)}"`
    + `${lock(dead)}><span class="sg-fill" data-at="${at}"></span>`
    + `<span id="sg-cknob" class="sg-vknob" data-at="${at}"></span></div>${note}`;
}

/** The mode: `Off` and `EQ` as chips, as Signal draws a DAC's. */
export function computerMode(tone, inert) {
  const chips = tone.modes.map((m) => chip('cmode', m.label, inert,
    ` id="sg-cm-${esc(m.name)}" data-mode="${esc(m.name)}"`, m.on)).join('');
  return `<div id="sg-cmode" class="sg-mode"><span class="sg-lbl">${esc(WORDS.mode)}</span>`
    + `<div class="sg-set">${chips}</div></div>`;
}

/** The tone graph: the curve the points make, shaded (faint while the EQ is off) and a dot on it for
 * each point. The dot of the point picked in the table is lit. */
export function computerGraph(tone, { inert, picked = null }) {
  const dots = tone.markers.map(fractionOf);
  const points = tone.points.map((p, i) => `<button type="button" id="sg-cpt-${p.n}" class="sg-pt${p.n === picked ? ' on' : ''}${inert ? ' quiet' : ''}"`
    + ` data-n="${p.n}" data-x="${dots[i].x.toFixed(5)}" data-y="${dots[i].y.toFixed(5)}" role="slider"`
    + ` aria-label="${esc(POINTS.point(p.n, hzText(p.f), gainText(p.g)))}" aria-valuemin="-12" aria-valuemax="12"`
    + ` aria-valuenow="${p.g}" aria-valuetext="${esc(gainText(p.g))}"${lock(inert)}></button>`).join('');
  const on = tone.modes.find((m) => m.name === 'eq')?.on === true;
  return plot({ id: 'sg-cgraph', response: tone.response, faint: !on, points });
}

/** The first screen of the computer side: the volume and the tone curve, with the mode under it on
 * a phone (on a desktop it heads the shelf's presets column). `c` is `{ volume, tone }` from the
 * core; either may be null. */
export function computerOpen(c, { phone, inert, picked }) {
  const volume = c.volume ? `<section id="sg-cvolume" class="sg-vol">${computerVolume(c.volume, inert)}</section>` : '';
  const tone = c.tone
    ? `<section id="sg-ctone" class="sg-tone"><span class="sg-lbl">${esc(WORDS.tone)}</span>`
      + `${computerGraph(c.tone, { inert, picked })}${phone ? computerMode(c.tone, inert) : ''}</section>` : '';
  return volume + tone;
}

/** One row of the points' table: its number, its type, the three values typed in their own fields
 * and the remove (the lone point is set flat). */
function row(point, { picked, inert }) {
  const field = (param, text) => `<input class="sg-pval${inert ? ' quiet' : ''}" id="sg-cf-${point.n}${param}" type="text"`
    + ` inputmode="decimal" autocomplete="off" data-act="cpoint" data-n="${point.n}" data-param="${param}"`
    + ` value="${esc(text)}" aria-label="${esc(POINTS.field(point.n, param))}"${inert ? ' readonly' : ''}${lock(inert)}>`;
  const types = POINTS.types.map((t) => `<option${t === point.type ? ' selected' : ''}>${t}</option>`).join('');
  const remove = `<button type="button" class="sg-prm${inert ? ' quiet' : ''}" id="sg-crm-${point.n}" data-act="cremove" data-n="${point.n}"`
    + ` aria-label="${esc(POINTS.remove(point.n))}"${lock(inert)}>×</button>`;
  return `<div class="sg-prow${picked === point.n ? ' on' : ''}" role="row" data-n="${point.n}">`
    + `<span class="sg-pn" role="cell">${point.n}</span>`
    + `<select class="sg-ptype${inert ? ' quiet' : ''}" id="sg-ct-${point.n}" data-act="ctype" data-n="${point.n}"`
    + ` aria-label="${esc(POINTS.type(point.n))}"${inert ? ' disabled' : ''}${lock(inert)}>${types}</select>`
    + `${field('f', hzText(point.f))}${field('g', gainText(point.g))}${field('q', qText(point.q))}${remove}</div>`;
}

/** The line that counts the points and the preamp; a state the EQ's file only says is marked as not
 * confirmed yet (never drawn as verified). */
export const lineHtml = (tone) => `${esc(tone.line)}${tone.verified ? ''
  : ` <span class="sg-unsure">${esc(POINTS.unconfirmed)}</span>`}`;

/** The points: a table of up to sixteen rows that shows six and scrolls, `+ Add point` and the line
 * that counts the points and the preamp. */
export function computerPoints(tone, { inert, picked = null }) {
  const heads = POINTS.heads.map((h) => `<span role="columnheader">${esc(h)}</span>`).join('');
  const long = tone.points.length > ROWS_SHOWN;
  const rows = tone.points.map((p) => row(p, { picked, inert })).join('');
  const add = button('cadd', POINTS.add, inert || !tone.canAdd, ' id="sg-cadd"', 'sg-btn sg-pill');
  const note = tone.note ? `<p class="sg-note">${esc(tone.note)}</p>` : '';
  return `<div id="sg-points" class="sg-box2"><h2 class="sg-lbl">${esc(POINTS.title)}</h2>`
    + `<div class="sg-pts" role="table" aria-label="${esc(POINTS.title)}">`
    + `<div class="sg-phead" role="row" aria-hidden="true">${heads}<span></span></div>`
    + `<div class="sg-pscroll${long ? ' long' : ''}" role="rowgroup" data-rows="${ROWS_SHOWN}">${rows}</div>`
    + `<p class="sg-pmore" aria-hidden="true">${esc(long ? POINTS.more(tone.points.length - ROWS_SHOWN) : '')}</p></div>`
    + `<div class="sg-set">${add}</div>`
    + `<p class="sg-note" id="sg-cline">${lineHtml(tone)}</p>${note}</div>`;
}

/** The presets of the computer's EQ: a chip for each (one it cannot play is quiet, with its reason as
 * the chip's title), what is on, and the ways to save the curve, rename or delete one of the person's
 * own. */
export function computerPresets(tone, { inert }) {
  const chips = tone.presets.map((p, i) => {
    const dead = inert || !p.playable;
    const on = p.on === true || p.name === tone.preset;
    return button('cpreset', p.name, dead,
      ` id="sg-cpr-${i}" data-name="${esc(p.name)}" aria-pressed="${on}"${p.playable ? '' : ` title="${esc(POINTS.presetWhy(p.why))}"`}`,
      `sg-chip${on ? ' on' : ''}${p.built_in ? '' : ' own'}`);
  }).join('');
  const own = tone.presets.some((p) => !p.built_in);
  const writes = inert ? ''
    : button('cpreset-save', SHELF.saveAs, false, ' id="sg-cpreset-save"', 'sg-btn sg-pill')
      + (own ? button('cpreset-rename', SHELF.rename, false, ' id="sg-cpreset-rename"', 'sg-btn sg-pill')
        + button('cpreset-delete', SHELF.remove, false, ' id="sg-cpreset-delete"', 'sg-btn sg-pill danger') : '');
  const status = tone.preset === null || tone.preset === undefined
    ? esc(POINTS.noPreset) : `<i>${esc(tone.preset)}</i>${esc(POINTS.presetOn)}`;
  return `<div id="sg-cpresets" class="sg-box2"><h2 class="sg-lbl">${esc(SHELF.presets)}</h2>`
    + `<div class="sg-set">${chips}</div><p class="sg-status">${status}</p>`
    + `${writes ? `<div class="sg-set">${writes}</div>` : ''}<div class="sg-ask"></div></div>`;
}

/** The row `label`: `text`, in the class `cls` when given. */
const fact = (label, text, cls = '') => `<dt>${esc(label)}</dt><dd${cls ? ` class="${cls}"` : ''}>${esc(text)}</dd>`;

/** The sink's rate and format in words: `96 kHz S32LE`. */
const formatWords = (sink) => [rateWords(sink.rate_hz), sink.format].filter(Boolean).join(' ');

/** The daemon's words for a stretch of the way, with the computer named by its audio server (nothing
 * below the line says `computer`): `PipeWire volume 54 %`, `PipeWire muted`. */
export const plain = (words) => words.replace(/\bthe computer's\b/gi, "PipeWire's").replace(/\bthe computer\b/gi, 'PipeWire')
  .replace(/\bcomputer\b/gi, 'PipeWire');

/** The path verdict, from the path the shown device reads (its hops' `computer` entry and the
 * words), else nothing. */
function verdict(path) {
  if (!path?.words) return '';
  return fact(COMPUTER.path, plain(path.words), path.class === 'ok' ? 'sg-ok' : 'sg-warn');
}

/** The facts of the route that are not controls: the sound card and its format, the other streams
 * and the verdict on the sound's way, or `route not read`. */
export function computerFacts(page) {
  const route = page.host?.route;
  const path = page.device?.playing?.path ?? page.host?.path ?? null;
  const head = `<h2 class="sg-lbl">${esc(COMPUTER.title)}</h2>`;
  if (!route) return `${head}<p class="sg-detail">${esc(COMPUTER.unread)}</p>`;
  const sink = route.sink;
  const rows = sink
    ? [fact(COMPUTER.sink, sink.words), fact(COMPUTER.format, formatWords(sink) || '—')]
    : [fact(COMPUTER.sink, COMPUTER.noSink)];
  rows.push(fact(COMPUTER.others, route.others_mixing > 0 ? COMPUTER.mixing(route.others_mixing) : COMPUTER.alone));
  return `${head}<dl class="sg-facts">${rows.join('')}${verdict(path)}</dl><p class="sg-note">${esc(COMPUTER.note)}</p>`;
}

/** What the computer's hop opens while its controls are not drawn (not served, or unavailable): the
 * facts alone. */
export function computerScreen(page) {
  return `<section id="sg-route" class="sg-plain">${computerFacts(page)}</section>`;
}

/**
 * The shelf of the computer side: two columns, the points' table, then the mode with the presets;
 * the facts across the bottom. On a phone the presets come first, then the table (the mode ended the
 * first screen).
 */
export function computerShelf(page, c, { phone, inert, picked }) {
  const tone = c.tone;
  const facts = `<section id="sg-cfacts" class="sg-col">${computerFacts(page)}</section></div>`;
  if (!tone) return `<div class="sg-cols">${facts}`;
  const points = computerPoints(tone, { inert, picked });
  const presets = computerPresets(tone, { inert });
  if (phone) {
    return `<div class="sg-cols"><section id="sg-cup" class="sg-col">${presets}</section>`
      + `<section id="sg-cbands" class="sg-col">${points}</section>${facts}`;
  }
  return `<div class="sg-cols"><section id="sg-cbands" class="sg-col">${points}</section>`
    + `<section id="sg-cmid" class="sg-col">${computerMode(tone, inert)}${presets}</section>${facts}`;
}
