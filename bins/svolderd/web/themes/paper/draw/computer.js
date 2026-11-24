/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The computer's side of the sheet: the switch by the title (`showing: the DAC · this computer`,
// the side shown underlined in pencil like every other chosen word), the tone as the DAC's is
// drawn (`TONE · off · EQ`, the curve with a pencil point for each point, a table with `type` and
// `×`, `+ Add point`, the line that counts the points and the preamp), its presets, and the notes
// that belong to it. Below the switch nothing says "computer": the headings are the DAC's. The
// core's `view.computer` holds what is drawn (core/computer.js); this is markup only. Pure.

import {
  POINTS, VIEW_HEIGHT, VIEW_WIDTH, logFreqs, responseDb,
} from '../../../core/tone-curve.js';
import {
  curveD, frame, path, zeroLine,
} from '../../../core/plot.js';
import { ROWS_SHOWN } from '../../../core/computer.js';
import { gainText, hzText, qText } from '../../../core/words.js';
import { WORDS } from '../words.js';
import { esc } from './esc.js';
import { viewerQuiet } from './side.js';
import { word } from './word.js';

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

/** A point's three values as the table writes them. */
export const valueWords = { f: hzText, g: gainText, q: qText };

/** The words of a point on the graph and in its row. */
const pointWords = (p) => `Point ${p.n}: ${p.type} ${hzText(p.f)}, ${gainText(p.g)}, Q ${qText(p.q)}`;

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
      + ` aria-valuenow="${p.g}" aria-valuetext="${esc(gainText(p.g))}"${quiet ? ' aria-disabled="true"' : ''}></button>`;
  }).join('');
  const on = t.modes.find((m) => m.on)?.name === 'eq';
  const drawn = zeroLine('p-zero') + path(curveD(t.response), `p-curve${on ? '' : ' faint'}`);
  return `<div id="p-graph" class="p-graph">${frame(drawn)}${points}</div>`;
}

/** What the mode's words say beyond the mode: the model's own note, and that the EQ's state is
 * only what its file says. Never drawn as running when it is not known to be. */
function notes(t) {
  const lines = [];
  if (!t.verified) lines.push(WORDS.notConfirmed);
  if (t.note) lines.push(t.note);
  return lines.map((text) => `<p class="p-ronote">${esc(text)}</p>`).join('');
}

/** The words of a stage on the chain line: its name and, while it plays, its level. */
function stageText(stage) {
  const name = stage.key === 'eq' ? 'EQ' : stage.name.toLowerCase();
  if (!stage.on) return `${name} off`;
  return `${name}${stage.key === 'space' && stage.kind !== 'off' ? ` ${stage.kind}` : ''} ${stage.text}`;
}

/**
 * The chain line under the tone's heading: `CHAIN · EQ −7.8 · loudness 60 % · space speakers −7.0 ·
 * plays −14.8 dB in all · bypass`. A stage word opens the Sound popup at its tab (plain words for a
 * viewer who may not open it), `bypass` is the toggle (underlined while the bypass is on; dim for a
 * viewer who may not write, and `idle` while nothing plays, where a tap still goes to the daemon),
 * the total is the daemon's own words. Each part carries the dot after it in one unbreakable span,
 * so a wrapped line never starts with a dot. Nothing from a host that sends no levels and no
 * bypass.
 */
export function chainLine(page, view, quiet) {
  const levels = view.computer.levelsOf?.(page);
  const bypass = view.computer.bypassOf?.(page);
  if (!levels && !bypass) return '';
  const parts = [`<span class="p-chain-head">${esc(WORDS.chain)}</span>`];
  for (const stage of levels?.stages ?? []) {
    parts.push(quiet
      ? `<span class="p-stage${stage.on ? ' on' : ''}" data-stage="${stage.key}">${esc(stageText(stage))}</span>`
      : word('c-stage', stageText(stage), false, ` id="p-cs-${stage.key}" data-stage="${stage.key}"`, stage.on ? 'on' : ''));
  }
  if (levels?.words) parts.push(`<i class="p-chain-total">${esc(levels.words)}</i>`);
  const last = bypass ? word('c-bypass', WORDS.bypass, quiet, ` id="p-cbypass" aria-pressed="${bypass.on}"`, bypass.on ? 'on' : (bypass.can || quiet ? '' : 'idle')) : null;
  const items = [...parts, ...(last ? [last] : [])]
    .map((part, i, all) => `<span class="p-it">${part}${i < all.length - 1 ? ' &middot;' : ''}</span>`);
  return `<p id="p-chain" class="p-label p-chain">${items.join(' ')}</p>`;
}

/** The tone section for the computer; nothing where there is no EQ to draw. */
export function computerTone(page, view) {
  const t = view.computer.toneOf(page);
  if (!t) return '';
  const quiet = viewerQuiet(page);
  const selected = view.computer.picked(page);
  const modes = t.modes.map((m) => word('c-mode', m.name === 'eq' ? m.label : m.label.toLowerCase(), quiet,
    ` id="p-cm-${m.name}" data-mode="${m.name}" aria-pressed="${m.on}"`, `${m.on ? 'on' : ''}${m.on && !t.verified ? ' unsure' : ''}`.trim()));
  const heads = WORDS.pointHeads.map((h) => `<th scope="col">${esc(h)}</th>`).join('')
    + `<th scope="col"><span class="p-sr">${esc(WORDS.removeHead)}</span></th>`;
  const rows = t.points.map((p) => row(quiet, p, selected)).join('');
  const beyond = t.points.length - ROWS_SHOWN;
  return `<h2 class="p-label">${esc(WORDS.toneLabel)} &middot; ${modes.join(' &middot; ')}</h2>${chainLine(page, view, quiet)}${notes(t)}`
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
  return curveD(responseDb(list.map((p) => ({ f0: p.f, gainDb: p.g, q: p.q, type: p.type })), freqs));
}
