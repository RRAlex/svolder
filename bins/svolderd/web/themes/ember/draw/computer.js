/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The theme Ember's computer side, drawn as strings from the core's view of it (`core.computer.view`):
// the switch beside the device's name, the volume as the dial's model, the
// tone control with its points and the shared presets. Pure, with no DOM. Nothing below the
// switch says whose controls these are; its own words are the device's name and `This computer`.
// A control the viewer may not use is drawn inert (`aria-disabled`), so a guest touching it is told
// to sign in; the elements carry `data-comp` and no `data-json`, so only computer.js acts on them.

import { POINT_TYPES, ROWS_SHOWN } from '../../../core/computer.js';
import {
  fixedEven, gainText, hzText, qText,
} from '../../../core/words.js';
import { curve } from './curve.js';
import { attr, esc } from './html.js';

const MINUS = '−';

/** The part's root asks htmx to swap it out of band. */
const swap = (oob) => attr('hx-swap-oob', oob ? 'true' : null);

/**
 * The computer's view with the stages laid over it: `levels` (the chain's stages and the total,
 * `core.computer.levelsOf`) and `bypass` (`core.computer.bypassOf`), read from the page when the
 * drawing asks, so the core's view stays what the core says it is. `view` itself when it is `null`.
 */
export function withStages(core, view) {
  if (!view) return view;
  const page = () => core.model.page();
  return Object.create(view, {
    levels: { get: () => core.computer.levelsOf(page()), enumerable: true },
    bypass: { get: () => core.computer.bypassOf(page()), enumerable: true },
  });
}

/**
 * What the computer's parts say, to tell whether they are to be drawn again: the switch, the tone,
 * the point in hand and the writes, and the words of the strip and the Bypass toggle as drawn. A
 * climb's tick that moves the chain's numbers and none of those words is the same thing.
 */
export function drawnOf(view) {
  if (!view) return null;
  const inert = !view.writes;
  return {
    sw: view.sw,
    tone: view.tone,
    picked: view.picked,
    writes: view.writes,
    strip: stripLine(view.levels, inert),
    toggle: bypassToggle(view.bypass, inert),
  };
}

/** The device as the screen lays it out: a computer side has the DAC page's parts, whatever the
 * device. */
export function effective(device, view) {
  return view?.side === 'computer' && device ? { ...device, layout: 'dac', lone: false } : device;
}

/** The three values of a point as the table and the editor say them. */
export const pointWords = (point) => ({
  f: hzText(point.f), g: gainText(point.g), q: qText(point.q),
});

/**
 * The switch: the device's name and `This computer` in the mode track's style, and where the
 * computer cannot be used the reason, in the switch's own place. Nothing where there is no switch.
 * `sw` is the core's `switchOf`.
 */
export function switchPill(sw, { oob = false } = {}) {
  if (!sw) return `<span id="part-switch" class="sideseg" hidden${swap(oob)}></span>`;
  const option = (side, label) => `<button type="button" class="opt"${attr('aria-pressed', sw.side === side ? 'true' : 'false')}${attr('id', `side-${side}`)}${attr('data-side', side)}>${esc(label)}</button>`;
  const why = sw.unavailable
    ? `<span class="why" role="status">computer controls unavailable: ${esc(sw.unavailable)}</span>` : '';
  return `<span id="part-switch" class="sideseg"${swap(oob)}><span class="seg" role="group" aria-label="Show the controls of">${option('dac', sw.dac)}${option('computer', sw.computer)}</span>${why}</span>`;
}

/** The volume as the dial's model draws it: 0 to 100 %, no zone, no cap, the readout `—` until the
 * first read. A level another program put above 100 % shows as read, the dial at its end. */
export function volumeModel(volume) {
  const { pct } = volume;
  return {
    key: 'computer-volume',
    mute: 'computer',
    lo: 0,
    hi: 100,
    raw: pct === null ? null : Math.min(pct, 100),
    target: null,
    risk: null,
    cap: null,
    silence: false,
    muted: volume.muted,
    readout: volume.text,
    unit: pct === null ? null : '%',
    aria: volume.aria,
    texts: Array.from({ length: 101 }, (_, i) => `${i} %`),
    db: [],
    notes: volume.boosted ? [{ text: 'Set above 100 % elsewhere', red: false }] : [],
    sub: volume.muted ? 'muted' : '',
    room: false,
    marks: [0, 25, 50, 75, 100].map((at) => ({ at, text: String(at) })),
    computer: true,
  };
}

/** The words a stage of the strip says: its name and, while it plays, its level. */
function stageWords(stage) {
  if (!stage.on) return `${stage.name} off`;
  const kind = stage.key === 'space' && stage.kind !== 'off' ? ` ${stage.kind}` : '';
  return `${stage.name}${kind} ${stage.text}${stage.key === 'loudness' ? '' : ' dB'}`;
}

/**
 * The line under the mode track: each stage with its level (a button that opens the Sound popup at
 * its tab, a plain word for a viewer who may not open it) and the daemon's words for the total.
 * `levels` is the core's `levelsOf`; nothing is drawn without it.
 */
export function stripLine(levels, inert) {
  if (!levels) return '';
  const parts = levels.stages.map((stage) => {
    const text = esc(stageWords(stage));
    if (inert) return `<span class="stage${stage.on ? ' on' : ''}"${attr('data-stage', stage.key)}>${text}</span>`;
    return `<button type="button" class="stage${stage.on ? ' on' : ''}"${attr('id', `ember-cstage-${stage.key}`)}${attr('data-comp', 'stage')}${attr('data-stage', stage.key)}>${text}</button>`;
  });
  if (levels.words) parts.push(`<span class="total">${esc(levels.words)}</span>`);
  // each item carries the dot after it, so a line breaks at a ` · ` and never starts with one
  const items = parts.map((part, i) => `<span class="it">${part}${i < parts.length - 1 ? '<span class="dot" aria-hidden="true"> ·</span>' : ''}</span>`);
  return `<p class="chain" aria-label="The chain's levels">${items.join(' ')}</p>`;
}

/** The Bypass toggle, drawn like a one-button `.seg`; nothing from a host that has no bypass. It is
 * `aria-disabled` for a viewer who may not write; where nothing plays it is drawn dim (`idle`) and a
 * tap still goes to the daemon, which says why not. */
export function bypassToggle(bypass, inert) {
  if (!bypass) return '';
  const off = inert ? attr('aria-disabled', 'true') : '';
  const idle = !inert && !bypass.can && !bypass.on ? ' idle' : '';
  return `<div class="seg bypassw"><button type="button" class="opt${idle}"${attr('id', 'ember-cbypass')}${attr('data-comp', 'bypass')}${attr('aria-pressed', bypass.on ? 'true' : 'false')}${off}>Bypass</button></div>`;
}

/** One row of the points' table. */
function row(point, { picked, inert }) {
  const off = inert ? attr('aria-disabled', 'true') : '';
  const words = pointWords(point);
  const cell = (param, word) => `<button type="button" class="cell"${attr('data-comp', 'cell')}${attr('data-n', point.n)}${attr('data-param', param)}${attr('aria-label', `${word} of point ${point.n}`)}${off}>${esc(words[param])}</button>`;
  const options = POINT_TYPES.map((t) => `<option${t === point.type ? ' selected' : ''}>${t}</option>`).join('');
  const type = `<select class="ptype"${attr('data-comp', 'type')}${attr('data-n', point.n)}${attr('aria-label', `Type of point ${point.n}`)}${inert ? ' disabled aria-disabled="true"' : ''}>${options}</select>`;
  const remove = `<button type="button" class="px"${attr('data-comp', 'remove')}${attr('data-n', point.n)}${attr('aria-label', `Remove point ${point.n}`)}${off}>×</button>`;
  return `<div class="pt${picked === point.n ? ' on' : ''}" role="group"${attr('aria-label', `Point ${point.n}`)}${attr('data-n', point.n)}><span class="pn">${point.n}</span>${type}${cell('f', 'Frequency')}${cell('g', 'Gain')}${cell('q', 'Q')}${remove}</div>`;
}

const STEPPERS = [['f', 'Frequency'], ['g', 'Gain'], ['q', 'Q']];

/** The editor under the table: the point in hand's three values, each with a - and a + (the DAC
 * table's steppers, `.stp`). The number between them is a plain span here, where the DAC's is a
 * button that types a value (tone-editor.js, scoped to `#part-bands`): on the computer a value is
 * typed in the table's cells. `.stp .v` styles both alike (the span is a grid item). */
export function editorBody(point, inert) {
  const words = pointWords(point);
  const off = inert ? attr('aria-disabled', 'true') : '';
  const button = (param, label, dir) => `<button type="button" class="btn sm"${attr('data-comp', 'step')}${attr('data-param', param)}${attr('data-dir', dir)}${attr('aria-label', `${dir < 0 ? 'Lower' : 'Raise'} the ${label.toLowerCase()}`)}${off}>${dir < 0 ? MINUS : '+'}</button>`;
  const stepper = ([param, label]) => `<div class="stp"${attr('data-param', param)}><span class="lbl">${label}</span>${button(param, label, -1)}<span class="v">${esc(words[param])}</span>${button(param, label, 1)}</div>`;
  return `<h4>Point ${point.n}</h4>${STEPPERS.map(stepper).join('')}`;
}

/** The editor's box: hidden and empty while no point is in hand. */
const editor = (point, inert) => (point
  ? `<div class="bedit"${attr('data-n', point.n)}>${editorBody(point, inert)}</div>`
  : '<div class="bedit" hidden></div>');

/** What the table says of the rows beyond the ones in view. */
export const moreWords = (hidden) => (hidden > 0 ? `${hidden} more ↓ scroll` : '');

/** The curve's paths: the line, the area and the ghost (the EQ's curve while the EQ is off). */
export const curveOf = (tone) => curve({
  mode: tone.modes[1].on ? 'eq' : 'flat', response: tone.response, markers: [], peak: null,
});

/** The tone section's curve: the grid, the EQ's curve (a ghost while the EQ is off) and a point per
 * row. */
function graph(tone) {
  const c = curveOf(tone);
  const gradient = '<defs> <linearGradient id="eqfill" gradientUnits="userSpaceOnUse" x1="0" y1="0" x2="0" y2="260"> <stop offset="0" class="eqs0"/><stop offset=".5" class="eqs1"/> <stop offset="1" class="eqs2"/> </linearGradient> </defs>';
  const grid = 'M132.6 0V260M233 0V260M333.3 0V260M466 0V260M566.3 0V260M666.7 0V260M799.3 0V260M899.7 0V260M0 10H1000M0 70H1000M0 190H1000M0 250H1000';
  const marks = tone.markers.map((m) => `<circle class="mk"${attr('data-n', m.n)}${attr('cx', fixedEven(m.x, 1))}${attr('cy', fixedEven(m.y, 1))} r="4.5"/>`).join('');
  const ghost = c.ghost === null ? '' : `<path class="ghost"${attr('d', c.ghost)}/>`;
  const svg = `${gradient} <path class="gl"${attr('d', grid)}/> <path class="g0" d="M0 130H1000"/> ${ghost} <path class="area"${attr('d', c.area)}/> <path class="line"${attr('d', c.line)}${attr('data-y', c.ys)}/> ${marks}`;
  const axes = '<span class="gx x0">20 Hz</span><span class="gx x100">100</span> <span class="gx x1k">1k</span><span class="gx x10k">10k</span> <span class="gy y12">+12</span><span class="gy y6">+6</span><span class="gy y0">0</span> <span class="gy ym6">−6</span><span class="gy ym12">−12</span>';
  return `<figure class="eqg pts"> <div class="plot"> <svg viewBox="0 0 1000 260" preserveAspectRatio="none" aria-hidden="true"> ${svg} </svg> ${axes} </div> </figure>`;
}

/**
 * The tone control: the mode track (Off, EQ), the curve, the points' table that shows six rows and
 * scrolls, the editor of the point in hand, `+ Add point` and the line that counts the points and the
 * preamp. `view` is `core.computer.view`.
 */
export function toneSection(view, { oob = false } = {}) {
  const tone = view.tone;
  if (!tone) return '<section id="part-tone" hidden></section>';
  const inert = !view.writes;
  const off = inert ? attr('aria-disabled', 'true') : '';
  const track = tone.modes.map((m) => ` <span class="optw"><button type="button" class="opt"${attr('id', `ember-cm-${m.name}`)}${attr('aria-pressed', m.on ? 'true' : 'false')}${attr('data-comp', 'mode')}${attr('data-mode', m.name)}${off}>${esc(m.label)}</button></span>`).join('');
  const rows = tone.points.map((p) => row(p, { picked: view.picked, inert })).join('');
  const head = '<div class="ph" aria-hidden="true"><span>#</span><span>Type</span><span>Frequency</span><span>Gain</span><span>Q</span><span></span></div>';
  const add = `<button type="button" class="btn sm"${attr('data-comp', 'add')}${tone.canAdd && !inert ? '' : attr('aria-disabled', 'true')}>+ Add point</button>`;
  const strip = stripLine(view.levels, inert);
  const toggle = bypassToggle(view.bypass, inert);
  const stages = strip || toggle ? ` <div class="stages">${strip}${toggle}</div>` : '';
  const unverified = tone.verified ? '' : ' <span class="pnote">not confirmed yet</span>';
  return `<section id="part-tone" class="tone comp"${inert ? attr('data-inert', true) : ''}${swap(oob)}> <h2>Tone control</h2> <div class="seg" role="group" aria-label="Tone control mode">${track} </div>${stages} ${graph(tone)} <div class="pts-table"> ${head} <div class="pscroll${tone.points.length > ROWS_SHOWN ? ' long' : ''}"${attr('data-rows', ROWS_SHOWN)}>${rows}</div> <p class="more">${esc(moreWords(tone.points.length - ROWS_SHOWN))}</p> ${editor(tone.points[view.picked - 1], inert)} <div class="pfoot">${add}<p class="pline">${esc(tone.line)}${unverified}</p></div> </div> </section>`;
}

/** A rename or delete icon button. */
const icon = (label, path, rows) => `<button type="button" class="ico" data-open${attr('aria-label', label)}> <svg viewBox="0 0 24 24" aria-hidden="true">${rows}<path${attr('d', path)}/></svg> </button>`;

/**
 * The shared presets: the status line, a row a preset (one the EQ cannot play is disabled with its
 * reason in place of its label) and, where the viewer may, rename and delete for their own and
 * `Save as preset` for the points on show. The forms are the DAC side's, marked `data-comp` so only
 * computer.js sends them.
 */
export function presetsSection(view, { oob = false } = {}) {
  const tone = view.tone;
  if (!tone) return '<section id="part-presets" hidden></section>';
  const name = tone.preset === null ? '' : `<strong>${esc(tone.preset)}</strong>`;
  const status = tone.preset === null ? 'No preset is on.' : ' is on.';
  const inert = !view.writes;
  const rows = tone.presets.map((p, index) => {
    const disabled = inert || !p.playable;
    const label = p.playable ? (p.built_in ? 'built in' : 'yours') : (p.why ?? 'cannot play');
    const on = p.on === true || p.name === tone.preset;
    const mine = !inert && !p.built_in;
    const buttons = mine
      ? ` ${icon(`Rename ${p.name}`, 'M4 20h4L19 9l-4-4L4 16zM13 7l4 4', '')} ${icon(`Delete ${p.name}`, 'M5 7h14M9 7V4h6v3M7 7l1 13h8l1-13', ' ')}`
      : '';
    const row = ` <div class="prow${p.built_in ? '' : ' user'}"><button type="button" class="pre"${attr('id', `pre-${index}`)}${attr('aria-pressed', on ? 'true' : 'false')}${attr('data-comp', 'preset')}${attr('data-name', p.name)}${disabled ? attr('aria-disabled', 'true') : ''}${p.playable ? '' : attr('title', label)}><i class="lamp"></i><span>${esc(p.name)}</span><small>${esc(label)}</small> </button>${buttons}</div>`;
    const forms = mine
      ? ` <form class="pform" data-form="rename" data-comp="form" hidden${attr('data-from', p.name)}> <input name="to"${attr('value', p.name)} maxlength="40" required aria-label="New name"> <button class="btn sm primary">Rename</button> <button type="button" class="btn sm" data-close>Cancel</button> </form> <form class="pform" data-form="delete" data-comp="form" hidden${attr('data-from', p.name)}> <p>Delete “${esc(p.name)}”? The points on the EQ stay as they are.</p> <button class="btn sm danger">Delete</button> <button type="button" class="btn sm" data-close>Keep</button> </form>`
      : '';
    return `${row}${forms}`;
  });
  const save = inert ? ''
    : ' <div class="psave"> <button type="button" class="btn" data-open>Save as preset</button> <form class="pform" data-form="save" data-comp="form" hidden> <input name="name" maxlength="40" required aria-label="Preset name"> <button class="btn sm primary">Save</button> <button type="button" class="btn sm" data-close>Cancel</button> </form> </div>';
  return `<section id="part-presets" class="presets comp"${swap(oob)}> <h3>Presets</h3> <p class="pstat">${name}${esc(status)}</p>${rows.join('')}${save} </section>`;
}
