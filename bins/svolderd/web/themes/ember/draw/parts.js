/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The theme Ember's parts of a device's screen, drawn as strings from the page's model: each is
// `(device, page, { oob }) -> string`, pure, with no DOM. `device` is the device's model and
// `page` the page's; `oob` asks the part's root to swap itself out of band, as the stream's
// parts do. A part's markup is what the page's stylesheet and scripts expect: where a space
// stands between two tags, the browser lays out a space.

import { curve } from './curve.js';
import {
  labels, shapes, ticks,
} from './dial.js';
import { arch } from './meter-arch.js';
import { attr, esc } from './html.js';

/** The part's root asks htmx to swap it out of band. */
const swap = (oob) => attr('hx-swap-oob', oob ? 'true' : null);

/** What a control's press posts, or that it is inert. */
const press = (device, json) => (device.inert ? attr('aria-disabled', 'true') : attr('data-json', json));

/** `on` as the `aria-checked`, `aria-pressed` and `data-live` words. */
const flag = (on) => (on === true ? 'true' : 'false');

/** Whether the model has no value: a `null` or a missing field. */
const none = (value) => value == null;

/** The link: a dot beside the device's name, with the state's word (shown unless connected). */
function link(device, page, { oob } = {}) {
  const { state, word } = device.link;
  return `<span id="part-link" class="link ${esc(state)}" role="img"${attr('aria-label', word)}${swap(oob)}><i aria-hidden="true"></i><span class="lw">${esc(word)}</span></span>`;
}

/** The card above the view: the device's, or the page's when it has no device to show. */
function card(device, page, { oob } = {}) {
  const c = device ? device.card : page.card;
  if (!c) return '';
  const body = c.faults.length === 0
    ? `<p>${c.spans.map((s) => (s.code ? `<code>${esc(s.text)}</code>` : esc(s.text))).join('')}</p>`
    : `<ul>${c.faults.map((f) => `<li><strong>${esc(f.label)}</strong>${f.advice ? ` ${esc(f.advice)}` : ''}</li>`).join('')}</ul>`;
  const countdown = none(c.retry_in) ? ''
    : `<div class="countdown"${attr('data-secs', c.retry_in)}><i></i></div>`;
  const tryNow = c.try_now
    ? '<button type="button" class="btn" id="try-now" hx-preserve="true" data-try>Try now</button>'
    : '';
  return `<div id="part-card" class="card ${esc(c.kind)}" role="status"${swap(oob)}><h2>${esc(c.title)}</h2>${body}${countdown}${tryNow}</div>`;
}

/** One control, by its kind, with a space either side as the controls list sets it. */
function control(c, device) {
  switch (c.kind) {
    case 'choice': return ` ${c.style === 'list' ? list(c, device) : segments(c, device)} `;
    case 'toggle': return ` ${toggle(c, device)} `;
    case 'slider': return ` ${slider(c, device)} `;
    default: return ` ${readout(c)} `;
  }
}

/** A select of up to five values: a track of buttons. */
function segments(c, device) {
  const buttons = c.options.map((o) => `<button type="button" class="opt"${attr('aria-pressed', flag(o.on))}${attr('data-key', c.key)}${attr('data-value', o.label)}${press(device, o.json)}>${esc(o.label)}</button>`);
  return `<div class="row"><span class="lbl">${esc(c.label)}</span><div class="seg" role="group"${attr('aria-label', c.label)}>${buttons.join('')}</div></div>`;
}

/** A select of more values: a list. */
function list(c, device) {
  const disabled = device.inert ? ' disabled aria-disabled="true"' : '';
  const options = c.options.map((o) => `<option${o.on ? ' selected' : ''}>${esc(o.label)}</option>`);
  return `<div class="row"><label class="lbl">${esc(c.label)}<select${attr('data-key', c.key)} name="value"${attr('aria-label', c.label)}${disabled}>${options.join('')}</select></label></div>`;
}

/** A switch. */
function toggle(c, device) {
  return `<div class="sws"><button type="button" class="sw" role="switch"${attr('aria-checked', flag(c.on))}${attr('data-key', c.key)}${press(device, c.json)}><span>${esc(c.label)}</span><i class="tg"></i></button></div>`;
}

/** A level other than the volume. */
function slider(c, device) {
  const off = device.inert ? attr('aria-disabled', 'true') : '';
  const [left, right] = c.ends ?? [];
  const track = '<line class="b-track" x1="0" y1="22" x2="100%" y2="22"/>';
  const fill = none(c.at) ? ''
    : `<line class="b-fill" x1="${esc(c.from)}%" y1="22" x2="${esc(c.at)}%" y2="22"/><circle class="b-thumb" cx="${esc(c.at)}%" cy="22" r="10"/>`;
  const mid = none(c.centre) ? ''
    : `<line class="b-mid" x1="${esc(c.from)}%" y1="15" x2="${esc(c.from)}%" y2="29"/>`;
  const raw = none(c.raw) ? '' : c.raw;
  const centre = none(c.centre) ? '' : c.centre;
  return `<div class="row r-bal"> <sv-slider id="slider-${esc(device.dom)}-${esc(c.key)}" hx-preserve="true"${attr('data-key', c.key)}${attr('data-lo', c.lo)}${attr('data-hi', c.hi)}${attr('data-raw', raw)}${attr('data-centre', centre)}${attr('data-texts', JSON.stringify(c.texts))}> <div class="head"><span class="lbl">${esc(c.label)}</span><button type="button" class="val"${off}>${esc(c.text)}</button></div> <div class="bal">${left === undefined ? '' : `<span>${esc(left)}</span>`}<svg class="bslider" role="slider" tabindex="0" width="100%" height="44"${attr('aria-label', c.label)}${attr('aria-valuemin', c.lo)}${attr('aria-valuemax', c.hi)}${none(c.raw) ? '' : attr('aria-valuenow', c.raw)}${attr('aria-valuetext', c.text)}${off}>${track}${fill}${mid}</svg>${right === undefined ? '' : `<span>${esc(right)}</span>`}</div> </sv-slider> </div>`;
}

/** A readout or a fault: its words and their tint. */
function readout(c) {
  return `<dl class="readout"${attr('data-key', c.key)}><dt>${esc(c.label)}</dt><dd class="${esc(c.tone)}">${esc(c.text)}</dd></dl>`;
}

/** The Controls section. */
function controls(device, page, { oob } = {}) {
  const heading = device.controls.length === 0 ? '' : '<h2>Controls</h2>';
  return `<section id="part-controls" class="controls"${swap(oob)}>${heading}${device.controls.map((c) => control(c, device)).join('')}</section>`;
}

/** The power button and, on an amp, its switches and readouts. */
function power(device, page, { oob } = {}) {
  const p = device.power;
  if (!p) return `<section id="part-power" hidden${swap(oob)}></section>`;
  const toggles = p.toggles.map((c) => control(c, device)).join('');
  const readouts = p.readouts.map((c) => control(c, device)).join('');
  return `<section id="part-power" class="p-power"${swap(oob)}> <div class="amp-top"><button type="button" class="btn power"${attr('aria-pressed', flag(p.on))}${attr('data-key', p.key)}${press(device, p.json)}> Power</button><span class="pstate${p.warming ? ' warm' : ''}">${esc(p.word)}</span></div> ${toggles} ${readouts} </section>`;
}

/** An amp's meters: an arch of segments each, kept in place while they move. */
function meters(device) {
  const m = device.meters;
  if (!m) return '<section id="part-meters" hidden></section>';
  const figures = m.meters.map((a) => {
    const lines = arch(a).map((s) => `<line class="seg ${s.class}${s.on ? ' on' : ''}"${attr('x1', s.x1)}${attr('y1', s.y1)}${attr('x2', s.x2)}${attr('y2', s.y2)}/>`);
    const percent = none(a.fraction) ? '' : '%';
    return ` <figure class="meter"${attr('data-key', a.key)}${attr('data-class-a', a.class_a)}> <svg viewBox="-8 -8 216 148" aria-hidden="true"><g>${lines.join('')}</g></svg> <figcaption class="m-read"><span class="m-pct">${esc(a.text)}</span>${percent}<span class="m-ch">${esc(a.channel)}</span></figcaption> </figure> `;
  });
  return `<section id="part-meters" class="p-meters" hx-preserve="true"> <h2>Meters</h2> <p class="mstate">${m.state ? esc(m.state) : ''}</p> <sv-meters${attr('data-live', flag(m.live))}> <div class="meters">${figures.join('')}</div> </sv-meters> </section>`;
}

/** The part's content: its pieces, as the markup lays them out, one space between two. */
const spaced = (...pieces) => pieces.filter((p) => p !== '').join(' ');

/** The numbers an SVG line carries, as its four attributes. */
const ends = ([x1, y1, x2, y2]) => `${attr('x1', x1)}${attr('y1', y1)}${attr('x2', x2)}${attr('y2', y2)}`;

/** The volume's dial: its arcs, ticks, labels, the target and cap ticks and the handle. */
function dial(v) {
  const shape = shapes(v);
  const arcPath = (name, d) => (d === null ? '' : `<path class="${name}"${attr('d', d)}/>`);
  const lines = ticks(v).map((t) => `<line class="tick ${esc(t.class)}"${attr('data-raw', t.raw)}${attr('x1', t.x1)}${attr('y1', t.y1)}${attr('x2', t.x2)}${attr('y2', t.y2)}/>`);
  const words = labels(v).map((l) => {
    const red = l.red ? ' red' : '';
    return `<line class="ltick${red}"${attr('data-at', l.at)}${ends(l.tick)}/><text class="lbl${red}"${attr('data-at', l.at)}${attr('x', l.x)}${attr('y', l.y)}>${esc(l.text)}</text>`;
  });
  const spoke = (name, at) => (at === null ? '' : `<line class="${name}"${ends(at)}/>`);
  const handle = shape.handle === null ? ''
    : `<circle class="handle"${attr('cx', shape.handle[0])}${attr('cy', shape.handle[1])} r="6.5"/>`;
  const gradient = '<defs> <linearGradient id="ramp-h" gradientUnits="userSpaceOnUse" x1="20" y1="0" x2="180" y2="0"> <stop class="g0" offset="0"/><stop class="g1" offset=".25"/><stop class="g2" offset=".5"/><stop class="g3" offset=".75"/><stop class="g4" offset="1"/> </linearGradient> </defs>';
  return spaced(
    gradient,
    arcPath('track', shape.track),
    arcPath('zone', shape.zone),
    arcPath('value', shape.value),
    arcPath('target', shape.target),
    lines.join(''),
    words.join(''),
    spoke('ttick', shape.targetTick),
    spoke('ctick', shape.capTick),
    handle,
  );
}

/** A note under the knob. */
const note = (n) => `<p class="note${n.red ? ' risk hot' : ''}">${esc(n.text)}</p>`;

/** The volume's readout: its number, the minus sign set apart, and its unit. */
function bigNumber(v) {
  const minus = '−';
  // Muted reads `−∞ dB`, as the other themes say it.
  const readout = v.muted ? `${minus}∞` : v.readout;
  const digits = readout.startsWith(minus)
    ? `<span class="neg">${minus}</span>${esc(readout.slice(1))}`
    : esc(readout);
  const unit = v.muted ? 'dB' : v.unit;
  const unitEl = unit == null ? '' : `<span class="unit">${esc(unit)}</span>`;
  return `<p class="big"><span class="num">${digits}</span>${unitEl}</p>`;
}

/** The volume: the dial, its readout, the step and mute buttons and the notes. */
function volume(device) {
  const v = device.volume;
  if (!v) return '<section id="part-volume" hidden></section>';
  const off = device.inert ? attr('aria-disabled', 'true') : '';
  const climb = v.notes.filter((n) => n.popup).map((n) => n.text).join('');
  const svg = `<svg class="dial" viewBox="-30 -16 260 220" role="slider" tabindex="0" aria-label="Volume"${attr('aria-valuemin', v.lo)}${attr('aria-valuemax', v.hi)}${none(v.raw) ? '' : attr('aria-valuenow', v.raw)}${attr('aria-valuetext', v.aria)}${device.inert || none(v.raw) ? attr('aria-disabled', 'true') : ''}> ${dial(v)} </svg>`;
  const buttons = `<div class="vol-btns"> <button type="button" class="step big" data-step="-1" aria-label="Down"${off}>−</button> <button type="button" class="btn mute"${attr('aria-pressed', flag(v.muted))}${off}>Mute</button> <button type="button" class="step big" data-step="1" aria-label="Up"${off}>+</button> </div>`;
  const slot = v.notes.filter((n) => n.eq).map(note).join('');
  const rest = v.notes.filter((n) => !n.eq && !n.popup).map(note).join('');
  const notes = `<div class="notes${v.room ? ' room' : ''}"> <div class="slot">${slot}</div> <div class="rest">${rest}</div> </div>`;
  const sub = `<p class="sub">${v.sub ? esc(v.sub) : ''}</p>`;
  const knob = `<sv-knob${attr('data-key', v.key)}${attr('data-lo', v.lo)}${attr('data-hi', v.hi)}${attr('data-raw', none(v.raw) ? '' : v.raw)}${attr('data-target', none(v.target) ? '' : v.target)}${attr('data-muted', flag(v.muted))}${attr('data-silence', flag(v.silence))}${attr('data-risk', none(v.risk) ? '' : v.risk)}${attr('data-cap', none(v.cap) ? '' : v.cap)}${attr('data-climb', climb)}${attr('data-texts', JSON.stringify(v.texts))}${attr('data-db', JSON.stringify(v.db))}${attr('data-mute', v.mute ?? '')}${device.inert ? attr('data-inert', 'true') : ''}${attr('class', v.muted ? 'muted' : '')}> ${svg} ${bigNumber(v)} ${sub} ${buttons} ${notes} </sv-knob>`;
  return `<section id="part-volume" class="p-vol" hx-preserve="true"> <h2>Volume</h2> ${knob} </section>`;
}

/** The tone control's mode track: Flat, EQ and Loudness, with the round - and + while Loudness is on. */
function modes(device, t) {
  const off = device.inert ? attr('aria-disabled', 'true') : '';
  const round = (word, step, label) => `<button type="button" class="rb" data-post="eq/loudness"${attr('data-json', JSON.stringify({ step }))}${attr('aria-label', label)} aria-describedby="loud-strength"${off}>${word}</button>`;
  const track = t.modes.map((m) => {
    const rounds = m.name === 'loudness' && t.loud;
    const press = device.inert ? off
      : ` ${attr('data-json', JSON.stringify({ mode: m.name }))}`;
    const strength = rounds && !none(t.strength)
      ? ` <span class="lv" id="loud-strength">strength <span class="n">${esc(t.strength)}</span></span>` : '';
    const opt = `<button type="button" class="opt"${attr('aria-pressed', flag(m.on))}${attr('data-mode', m.name)}${press}>${esc(m.label)}${strength}</button>`;
    return ` <span class="optw">${rounds ? ` ${round('−', -1, 'Lower the loudness')}` : ''} ${opt}${rounds ? ` ${round('+', 1, 'Raise the loudness')}` : ''} </span>`;
  });
  return `<div class="seg${t.loud ? ' loud' : ''}" role="radiogroup" aria-label="Tone control mode"${none(t.strength) ? '' : attr('data-strength', t.strength)}>${track.join('')} </div>`;
}

/**
 * The grid lines' `d`, copied from the template's literal, newline and indent included: an
 * attribute's value is compared verbatim.
 */
const GRID = 'M132.6 0V260M233 0V260M333.3 0V260M466 0V260M566.3 0V260\n          M666.7 0V260M799.3 0V260M899.7 0V260M0 10H1000M0 70H1000M0 190H1000M0 250H1000';

/** The tone graph: the grid, the ghost, the area, the line, the peak and a point per band. */
function graph(t) {
  const c = curve(t);
  const gradient = '<defs> <linearGradient id="eqfill" gradientUnits="userSpaceOnUse" x1="0" y1="0" x2="0" y2="260"> <stop offset="0" class="eqs0"/><stop offset=".5" class="eqs1"/> <stop offset="1" class="eqs2"/> </linearGradient> </defs>';
  const peak = c.peak === null ? ''
    : `<line class="peak" x1="0"${attr('y1', c.peak.y)} x2="1000"${attr('y2', c.peak.y)}/>`;
  const marks = c.markers.map((m) => `<circle class="mk"${attr('data-band', m.band)}${attr('cx', m.x)}${attr('cy', m.y)} r="4.5"/>`);
  const svg = spaced(
    gradient,
    `<path class="gl"${attr('d', GRID)}/>`,
    '<path class="g0" d="M0 130H1000"/>',
    c.ghost === null ? '' : `<path class="ghost"${attr('d', c.ghost)}/>`,
    `<path class="area"${attr('d', c.area)}/>`,
    `<path class="line"${attr('d', c.line)}${attr('data-y', c.ys)}/>`,
    peak,
    marks.join(''),
  );
  const axes = '<span class="gx x0">20 Hz</span><span class="gx x100">100</span> <span class="gx x1k">1k</span><span class="gx x10k">10k</span> <span class="gy y12">+12</span><span class="gy y6">+6</span><span class="gy y0">0</span> <span class="gy ym6">−6</span><span class="gy ym12">−12</span>';
  const words = c.peak === null ? '' : `<span class="pkt">${esc(c.peak.words)}</span>`;
  return `<figure class="eqg"${attr('data-readings', JSON.stringify(t.readings))}> <div class="plot"> <svg viewBox="0 0 1000 260" preserveAspectRatio="none" aria-hidden="true"> ${svg} </svg> ${axes} </div> ${words} </figure>`;
}

/** The band table: a row a band, in words, and the place the band editor opens. */
function table(device, b) {
  const rows = b.rows.map((r) => ` <button type="button" class="band"${attr('data-band', r.band)} aria-pressed="false"${device.inert ? attr('aria-disabled', 'true') : ''}><span>${esc(r.band)}</span> <span>${esc(r.f)}</span><span>${esc(r.g)}</span><span>${esc(r.q)}</span></button>`);
  return `<div id="part-bands" class="bands${b.faint ? ' faint' : ''}"> <div class="bh"><span>Band</span><span>Frequency</span><span>Gain</span><span>Q</span></div>${rows.join('')} <div class="bedit"${attr('id', `band-editor-${device.dom}`)} hx-preserve="true" hidden></div> </div>`;
}

/** The tone control: its modes, its curve and the band table. */
function tone(device, page, { oob } = {}) {
  const t = device.tone;
  if (!t) return '';
  const rows = t.bands ? table(device, t.bands) : '';
  return `<section id="part-tone" class="tone"${attr('data-note', t.note ?? '')}${device.inert ? attr('data-inert', true) : ''}${swap(oob)}> <h2>Tone control</h2> ${modes(device, t)} ${graph(t)} ${rows} </section>`;
}

/** The band table on its own. */
function bands(device) {
  return device.tone && device.tone.bands ? table(device, device.tone.bands) : '';
}

/** A save, rename or delete icon button. */
const icon = (open, label, path, rows) => `<button type="button" class="ico"${open}${attr('aria-label', label)}> <svg viewBox="0 0 24 24" aria-hidden="true">${rows}<path${attr('d', path)}/></svg> </button>`;

/** The tone presets: the status line, a row a preset and, where the viewer may, save, rename and delete. */
function presets(device, page, { oob } = {}) {
  const p = device.presets;
  if (!p) return '';
  const { status } = p;
  const name = status.name == null ? '' : `<strong>${esc(status.name)}</strong>`;
  const rows = p.rows.map((r) => {
    const recall = p.recall
      ? ` ${attr('data-json', r.json)}`
      : attr('aria-disabled', 'true');
    const mine = p.editable && !r.built_in;
    const buttons = mine
      ? ` ${icon(' data-open', `Rename ${r.name}`, 'M4 20h4L19 9l-4-4L4 16zM13 7l4 4', '')} ${icon(' data-open', `Delete ${r.name}`, 'M5 7h14M9 7V4h6v3M7 7l1 13h8l1-13', ' ')}`
      : '';
    const row = `<div class="prow${r.built_in ? '' : ' user'}"><button type="button" class="pre"${attr('aria-pressed', flag(r.on))}${attr('data-name', r.name)}${recall}><i class="lamp"></i><span>${esc(r.name)}</span><small>${r.built_in ? 'built in' : 'yours'}</small> </button>${buttons}</div>`;
    const forms = mine
      ? ` <form class="pform" data-form="rename" hidden${attr('data-json', r.rename_json)}> <input${attr('name', 'to')}${attr('value', r.name)} maxlength="40" required aria-label="New name"> <button class="btn sm primary">Rename</button> <button type="button" class="btn sm" data-close>Cancel</button> </form> <form class="pform" data-form="delete" hidden${attr('data-json', r.json)}> <p>Delete “${esc(r.name)}”? The bands on the device stay as they are.</p> <button class="btn sm danger">Delete</button> <button type="button" class="btn sm" data-close>Keep</button> </form>`
      : '';
    return ` ${row}${forms}`;
  });
  const save = p.editable
    ? ` <div class="psave"> <button type="button" class="btn" data-open>Save as preset</button> <form class="pform" data-form="save" hidden> <input name="name" maxlength="40" required aria-label="Preset name"> <button class="btn sm primary">Save</button> <button type="button" class="btn sm" data-close>Cancel</button> </form> </div>`
    : '';
  const why = p.note == null ? '' : `<p class="ronote">${esc(p.note)}</p>`;
  return `<section id="part-presets" class="presets"${swap(oob)}> <h3>Presets</h3> <p class="pstat">${esc(status.lead)}${name}${esc(status.tail)}</p>${rows.join('')}${save} ${why} </section>`;
}

/** A device's own presets: its slots, to recall and to save over, and the built-in settings. */
function devicePresets(device, page, { oob } = {}) {
  const d = device.device_presets;
  if (!d) return '';
  const slots = d.slots.map((r) => {
    const recall = d.recall
      ? ` ${attr('data-json', r.recall_json)}`
      : attr('aria-disabled', 'true');
    const save = d.save
      ? ` ${icon(' data-open="save"', `Save the bands to ${r.name}`, 'M5 4h11l3 3v13H5zM8 4v5h7V4M8 20v-6h8v6', ' ')}`
      : '';
    const row = `<div class="prow slot"><button type="button" class="pre"${attr('data-name', r.name)}${recall}><i class="lamp"></i><span>${esc(r.name)}</span><small>on the device</small></button>${save}</div>`;
    const form = d.save
      ? ` <form class="pform" data-form="save" hidden${attr('data-json', r.save_json)}> <p>Save the bands to ${esc(r.name)}? What the device holds there is replaced.</p> <button class="btn sm danger">Save</button> <button type="button" class="btn sm" data-close>Keep</button> </form>`
      : '';
    return ` ${row}${form}`;
  });
  const builtins = d.builtins.map((r) => {
    const apply = d.recall
      ? ` ${attr('data-json', r.json)}`
      : attr('aria-disabled', 'true');
    return ` <div class="prow"><button type="button" class="pre"${attr('aria-pressed', flag(r.on))}${attr('data-name', r.name)}${apply}><i class="lamp"></i><span>${esc(r.name)}</span></button></div>`;
  });
  return `<section id="part-device-presets" class="presets"${swap(oob)}> <h3>Presets</h3>${slots.join('')} <h4>Built in</h4>${builtins.join('')} </section>`;
}

/** One of the Device fold-out's readouts. */
const factRow = (label, value, extra = '') => `<dl class="readout"><dt>${label}</dt><dd${extra}>${value}</dd></dl>`;

/** The Device fold-out: the unit's facts, the table of every reading it holds and Identify. */
function facts(device, page, { oob } = {}) {
  const f = device.facts;
  const product = f.product == null ? `product ${esc(f.product_id)}` : `${esc(f.product)} (product ${esc(f.product_id)})`;
  const age = f.signal_age;
  const signalAge = age
    ? `${attr('data-head', age.head)}${attr('data-heard-at', age.heard_at_ms)}${attr('data-live', age.live)}`
    : '';
  // A space stands between the readouts, as the markup breaks its lines there.
  const list = [
    factRow('Product', product),
    f.unit ? factRow('Unit', esc(f.unit), ' class="mono"') : '',
    factRow('Firmware', esc(f.firmware)),
    factRow('DSP', f.dsp == null ? '—' : esc(f.dsp)),
    factRow('Profile', esc(f.profile)),
    factRow('Link', esc(f.link)),
    factRow('Signal', esc(f.signal), ` data-signal${signalAge}`),
    f.passphrase_note == null ? '' : factRow('Passphrase', esc(f.passphrase_note), ' class="warn"'),
  ].filter((x) => x !== '').join(' ');
  const rows = f.rows.map((r) => {
    const heard = r.heard_at_ms == null ? '>—</td>' : `${attr('data-heard-at', r.heard_at_ms)}></td>`;
    return `<tr><td class="id">${esc(r.id)}</td><td>${esc(r.label)}</td><td class="val">${esc(r.value)}</td><td class="age"${heard}</tr>`;
  });
  const identify = f.identify_offered
    ? ` <div class="identify"><button type="button" class="btn" data-identify${attr('data-gap', f.identify_gap_secs)}${f.identify ? '' : attr('aria-disabled', 'true')}>Identify</button> <span class="note">Asks the unit to show itself, so you can tell it from the others.</span> </div>`
    : '';
  return `<section id="part-facts" class="p-facts"${attr('data-now', f.now_ms)}${swap(oob)}> <details class="device-facts"><summary>Device</summary> <div class="facts"> ${list} </div> <h3>All readings</h3> <div class="readings-wrap"><table class="readings" aria-label="All readings"> <thead><tr><th scope="col">Id</th><th scope="col">Reading</th><th scope="col">Value</th> <th scope="col">Heard</th></tr></thead> <tbody>${rows.join('')}</tbody> </table></div>${identify} </details> </section>`;
}

/** The previous, play or pause, and next buttons. */
function transport(t) {
  const playing = t.status === 'playing';
  const glyph = playing ? '<path d="M6 4h4v16H6zM14 4h4v16h-4z"/>' : '<path d="M7 4v16l13-8z"/>';
  return `<span class="np-trans"><button type="button" class="np-btn prev" data-press="previous" aria-label="Previous"><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M11 5v14l-8-7zM20 5v14l-8-7z"/></svg></button><button type="button" class="np-btn go"${attr('data-press', playing ? 'pause' : 'play')}${attr('aria-label', playing ? 'Pause' : 'Play')}><svg viewBox="0 0 24 24" aria-hidden="true">${glyph}</svg></button><button type="button" class="np-btn next" data-press="next" aria-label="Next"><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M13 5v14l8-7zM4 5v14l8-7z"/></svg></button></span>`;
}

/** The Now playing card: the track, its transport and the path the sound takes to the device. */
function playing(device, page, { oob } = {}) {
  const p = device.playing;
  if (!p) return '';
  const t = p.track;
  const root = `<section id="part-playing" class="p-playing${t ? '' : ' idle'}" aria-label="Now playing"${attr('data-now', p.now_ms)}${attr('data-sink-label', p.sink_label)}${swap(oob)}>`;
  if (!t) return `${root} <p class="np-none">No player</p> </section>`;
  const g = t.progress;
  const np = `<div class="np"${attr('data-status', t.status)}${none(t.length_us) ? '' : attr('data-length', t.length_us)}${g ? `${attr('data-position', g.position_us)}${attr('data-rate', g.rate_milli)}${attr('data-read-at', g.read_at_ms)}` : ''}>`;
  const cover = t.cover == null ? ''
    : `<img${attr('src', `api/v1/host/cover?v=${t.cover}`)} alt="" width="96" height="96">`;
  const artist = t.artist ? `<span class="np-artist">${esc(t.artist)}</span>` : '';
  const source = t.source ? `<span class="np-source">${esc(t.source)}</span>` : '';
  const who = `<span class="np-who"><b class="np-title"><span>${esc(t.headline)}</span></b>${artist}${source}</span>`;
  const fold = '<button type="button" class="np-fold" aria-expanded="false" aria-label="Show the whole card"><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M6 9l6 6 6-6"/></svg></button>';
  const head = `<div class="np-head"> <span class="np-cover" aria-hidden="true">${cover}</span> ${who} ${p.buttons ? `${transport(t)} ` : ''}${fold} </div>`;
  const prog = '<div class="np-prog"><span class="np-t np-el"></span><span class="np-bar"><i></i></span><span class="np-t np-len"></span></div>';
  const path = p.path;
  const hops = path ? path.hops.map((h) => `<span class="np-hop${h.changes ? ' changes' : ''}"><b>${esc(h.label)}</b> ${esc(h.detail)}</span>`).join('') : '';
  const words = path ? `<span class="np-verdict">${esc(path.words)}</span>${hops}` : '';
  return `${root} ${np} ${head} ${prog} <p class="np-path${path ? ` ${esc(path.class)}` : ''}">${words}</p> </div> </section>`;
}

/**
 * The visualizer card: the canvas the page's script draws into and what sits on it. It has no
 * part of its own, and its state is the script's.
 */
function visualizer() {
  return '<div class="p-viz" role="group" aria-label="Visualizer" data-look="lumibars-soft" data-graph="off" data-state="idle"> <button type="button" class="viz-face" aria-label="Visualizer. Press to change the look"><canvas aria-hidden="true"></canvas></button> <span class="viz-chip" aria-hidden="true"></span> <button type="button" class="viz-graph" aria-pressed="false">Graph</button> <p class="viz-note" hidden></p> <p class="viz-sink" hidden></p> </div>';
}

export const PARTS = {
  link,
  card,
  controls,
  control: (device, page, { control: c }) => control(c, device),
  power,
  meters,
  volume,
  tone,
  bands,
  presets,
  'device-presets': devicePresets,
  facts,
  playing,
  visualizer,
};
