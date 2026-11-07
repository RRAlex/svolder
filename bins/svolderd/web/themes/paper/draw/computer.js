/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The computer's side of the sheet: the switch by the title (`showing: the DAC · this computer`,
// the side shown underlined in pencil like every other chosen word), the tone as the DAC's is
// drawn (`TONE · off · EQ`, the curve with a pencil point for each point, a table with `type` and
// `×`, `+ Add point`, the line that counts the points and the preamp), its presets, and the notes
// that belong to it. Below the switch nothing says "computer": the headings are the DAC's. The
// core's `view.computer` holds what is drawn (core/computer.js); this is markup only. Pure.

import {
  POINTS, linePath, logFreqs, responseDb, yOf,
} from '../../../core/tone-curve.js';
import { WORDS } from '../words.js';
import { esc } from './esc.js';
import { viewerQuiet } from './side.js';
import { word } from './word.js';

const VIEW_WIDTH = 1000;
const VIEW_HEIGHT = 260;
/** The rows the table shows before it scrolls. */
export const ROWS_SHOWN = 6;
const MINUS = '−';
/** What the daemon calls a curve no preset holds (the points were set by hand). */
const CUSTOM = 'custom';

/** The switch: the device's kind and the computer, the side shown underlined; or the sentence of
 * why the computer cannot be used, in the switch's own place; nothing where there is no switch. */
export function showing(page, device, view) {
  const sw = view.computer?.switchOf?.(page);
  if (!sw) return '';
  if (sw.unavailable) return `<p class="p-showing unavailable">${esc(sw.unavailable)}</p>`;
  const side = (name, text) => word('side', text, false,
    ` id="p-side-${name}" data-side="${name}" aria-pressed="${sw.side === name}"`, sw.side === name ? 'on' : '');
  return `<p class="p-showing"><i>${esc(WORDS.showing)}</i> `
    + `${side('dac', WORDS.theDevice(device?.family_word))} &middot; ${side('computer', WORDS.thisComputer)}</p>`;
}

/** A frequency as the table writes it: `105 Hz`, `1200 Hz`, one decimal under 100 Hz at most. */
export function hzWords(f) {
  const text = f < 100 ? String(Math.round(f * 10) / 10) : String(Math.round(f));
  return `${text} Hz`;
}

/** A gain as the table writes it: a sign, one decimal, a true minus. */
export function dbWords(g) {
  const text = Math.abs(g).toFixed(1);
  if (text === '0.0') return '0.0 dB';
  return `${g < 0 ? MINUS : '+'}${text} dB`;
}

/** A point's three values as the table writes them. */
export const valueWords = { f: hzWords, g: dbWords, q: (q) => q.toFixed(2) };

/** The words of a point on the graph and in its row. */
const pointWords = (p) => `Point ${p.n}: ${p.type} ${hzWords(p.f)}, ${dbWords(p.g)}, Q ${p.q.toFixed(2)}`;

/** The curve's path in the plot's own box. */
export const curvePath = (response) => linePath(response.map(yOf));

/** One row of the table: its number, the type word that opens the list, the three values to type
 * into or step, and the `×` that removes it (the lone point is set flat). */
function row(quiet, p, selected) {
  const cell = (param) => `<td><input type="text" class="p-cell${quiet ? ' quiet' : ''}"`
    + ` id="p-c${p.n}${param}" data-n="${p.n}" data-param="${param}" inputmode="decimal"`
    + ` autocomplete="off" spellcheck="false" value="${esc(valueWords[param](p[param]))}"`
    + ` aria-label="${esc(WORDS.cellOf(p.n, param))}"${quiet ? ' aria-disabled="true"' : ''}></td>`;
  const remove = word('c-remove', '×', quiet, ` id="p-crm-${p.n}" data-n="${p.n}" aria-label="${esc(WORDS.removePoint(p.n))}"`, 'rm');
  return `<tr data-n="${p.n}"${selected === p.n ? ' class="sel"' : ''}><th scope="row">${p.n}</th>`
    + `<td>${word('c-type', p.type, quiet, ` id="p-ct${p.n}" data-n="${p.n}" aria-haspopup="listbox"`
      + ` aria-label="${esc(WORDS.typeOf(p.n))}"`, 'type')}</td>`
    + `${['f', 'g', 'q'].map(cell).join('')}<td>${remove}</td></tr>`;
}

/** The graph: its zero line, the curve and a pencil point for each point of the EQ. */
function graph(t, quiet, selected) {
  const points = t.markers.map((m) => {
    const p = t.points[m.n - 1];
    return `<button type="button" id="p-pt-${m.n}" class="pt${quiet ? ' quiet' : ''}${selected === m.n ? ' sel' : ''}"`
      + ` data-n="${m.n}" data-x="${(m.x / VIEW_WIDTH).toFixed(5)}" data-y="${(m.y / VIEW_HEIGHT).toFixed(5)}"`
      + ` role="slider" aria-label="${esc(pointWords(p))}" aria-valuemin="-12" aria-valuemax="12"`
      + ` aria-valuenow="${p.g}" aria-valuetext="${esc(dbWords(p.g))}"${quiet ? ' aria-disabled="true"' : ''}></button>`;
  }).join('');
  const on = t.modes.find((m) => m.on)?.name === 'eq';
  return `<div id="p-graph" class="p-graph"><svg viewBox="0 0 ${VIEW_WIDTH} ${VIEW_HEIGHT}"`
    + ' preserveAspectRatio="none" aria-hidden="true">'
    + `<path class="p-zero" d="M0 ${yOf(0)}H${VIEW_WIDTH}"/>`
    + `<path class="p-curve${on ? '' : ' faint'}" d="${curvePath(t.response)}"/></svg>${points}</div>`;
}

/** What the mode's words say beyond the mode: the model's own note, and that the EQ's state is
 * only what its file says. Never drawn as running when it is not known to be. */
function notes(t) {
  const lines = [];
  if (!t.verified) lines.push(WORDS.notConfirmed);
  if (t.note) lines.push(t.note);
  return lines.map((text) => `<p class="p-ronote">${esc(text)}</p>`).join('');
}

/** The tone section for the computer; nothing where there is no EQ to draw. */
export function computerTone(page, view) {
  const t = view.computer.toneOf(page);
  if (!t) return '';
  const quiet = viewerQuiet(page);
  const selected = view.point ?? null;
  const modes = t.modes.map((m) => word('c-mode', m.name === 'eq' ? m.label : m.label.toLowerCase(), quiet,
    ` id="p-cm-${m.name}" data-mode="${m.name}" aria-pressed="${m.on}"`, `${m.on ? 'on' : ''}${m.on && !t.verified ? ' unsure' : ''}`.trim()));
  const heads = WORDS.pointHeads.map((h) => `<th scope="col">${esc(h)}</th>`).join('')
    + `<th scope="col"><span class="p-sr">${esc(WORDS.removeHead)}</span></th>`;
  const rows = t.points.map((p) => row(quiet, p, selected)).join('');
  const beyond = t.points.length - ROWS_SHOWN;
  return `<h2 class="p-label">${esc(WORDS.toneLabel)} &middot; ${modes.join(' &middot; ')}</h2>${notes(t)}`
    + graph(t, quiet, selected)
    + `<div id="p-ptsbox" class="p-ptsbox${beyond > 0 ? ' fade' : ''}" data-count="${t.points.length}" data-rows="${ROWS_SHOWN}">`
    + `<table id="p-bands" class="p-bands p-pts${t.modes[1].on ? '' : ' faint'}"><thead><tr>${heads}</tr></thead>`
    + `<tbody>${rows}</tbody></table></div>`
    + `<p id="p-more" class="p-more"${beyond > 0 ? '' : ' hidden'}>${esc(WORDS.moreBelow(Math.max(beyond, 0)))}</p>`
    + `<p class="p-add">${t.canAdd ? word('c-add', WORDS.addPoint, quiet, ' id="p-add"') : ''}</p>`
    + `<p class="p-line-count"><i>${esc(t.line)}</i></p>`;
}

/** The presets section for the computer: what is on, the names to apply (one the computer cannot
 * play is quiet, with its reason under it) and `save as…`. */
export function computerPresets(page, view) {
  const t = view.computer.toneOf(page);
  if (!t) return '';
  const quiet = viewerQuiet(page);
  const names = t.presets.map((r, i) => word('c-preset', r.name, quiet || !r.playable,
    ` id="p-cp-${i}" data-name="${esc(r.name)}"${r.playable ? '' : ' aria-disabled="true"'}`,
    `${r.on ? 'on' : ''}${r.built_in ? '' : ' own'}`.trim()));
  const why = t.presets.filter((r) => !r.playable && r.why)
    .map((r) => `<p class="p-ronote">${esc(`${r.name}: ${r.why}`)}</p>`).join('');
  const custom = t.preset === CUSTOM;
  const lead = t.preset && !custom ? `<i>${esc(t.preset)}</i> ${esc(WORDS.isOn)}`
    : esc(custom ? WORDS.byHand : WORDS.noPresetOn);
  const writes = quiet ? [] : [word('c-save', WORDS.saveAs, false, ' id="p-c-save"')];
  if (!quiet && t.presets.some((r) => !r.built_in)) {
    writes.push(word('c-edit', WORDS.editPresets, false, ' id="p-c-edit"'));
  }
  return `<h2 class="p-label">${esc(WORDS.presetsLabel)}</h2><p class="p-status">${lead}</p>`
    + `<p class="p-names">${names.join(' &middot; ')}</p>${why}`
    + `${writes.length > 0 ? `<p class="p-writes">${writes.join(' &middot; ')}</p>` : ''}`
    + '<div class="p-ask"></div>';
}

/** The curve of a held drag: the path of the points `list` (`{ type, f, g, q }`). */
export function heldCurve(list) {
  const freqs = logFreqs(POINTS);
  return curvePath(responseDb(list.map((p) => ({ f0: p.f, gainDb: p.g, q: p.q, type: p.type })), freqs));
}
