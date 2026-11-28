/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The HUD's words from the model, and the rules of the overlay that are not drawing: which modules
// and rows the page has (Host, DAC, Amp: real readings, a row the model does not carry is left
// out), the sparkline's values, the link's 8 s history, when a module shows (the director's
// moments), the VU row's dBFS, and the audio panel's fade and values. Pure: the page model, the
// clock and the director's events are handed in; no DOM, no timers.
//
//   modulesOf(page, { curve })     `[{ id, title, sub, rows }]`: the modules the page has, each row
//                                  `{ id, label, text, tone, ... }` (`tone`: '', 'on', 'warn' or
//                                  'bad'; `slot` the drawn part of a row: a bar, a sparkline, the
//                                  link's bars)
//   rowText(module, label)         the words a row of `label` shows (case blind), or null
//   ampOf(page)                    the amp the Amp module reads: the shown one, else a followed one
//   sparkOf(response)              the computer EQ's curve as 64 values, 0 to 1 over -12 .. +12 dB
//   dbfsWords(db), vuBar(db)       a VU row's reading (`-18.0 dBFS`) and its bar's fill
//   topLine(page, state)           the line that is always on: `{ left, right, tone }`
//   new Levels()                   the VU rows' RMS dBFS, bar and peak mark from a levels frame
//   new LinkHistory()              the link's dBm over the last 8 s: `push`, `points`
//   new Moments()                  which module shows and how strongly: `on`, `step`, `alpha`,
//                                  `active`
//   FADE                           the share of its strength a muted scene holds the panel to
//   featureWords(key, v)           a panel feature's value in words
//   PHONE_QUERY                    what the theme calls a phone (the panel's and the HUD's wires)

import { changedOf, computerOf, dacOf } from '../../core/path-words.js';
import { PHONE_MAX_PX } from '../../core/phone.js';
import { TITLE, statusOf } from '../../core/title.js';
import { Lamp, byteAmplitude, peakAmplitude } from '../../core/vu-model.js';
import { dbmOf, weakOf } from './cues.js';
import { HUD } from './words.js';

/** What the theme calls a phone: a window the core's phone width or narrower, or a touch screen
 * turned sideways (a coarse pointer under 500 px of height; an upright phone is the width's, a touch
 * laptop or a tablet keeps the desktop layout). style.css's phone blocks say the same in their
 * `@media` lines. */
export const PHONE_QUERY = `(max-width: ${PHONE_MAX_PX}px), (pointer: coarse) and (max-height: 500px)`;

const clamp = (v, a = 0, b = 1) => (v < a ? a : v > b ? b : v);

/** The sparkline's width, in values, and the gain at its top and bottom (the curve is shown from
 * -12 to +12 dB: the computer EQ's hand range is -12 .. +12). */
export const SPARK_POINTS = 64;
const SPARK_DB = 12;

/** The link's history spans this many ms and its strength is drawn from `LINK_DBM[0]` (an empty
 * bar) to `LINK_DBM[1]` (a full one). */
export const LINK_MS = 8000;
const LINK_DBM = [-95, -45];

/** A VU row's bar runs from -60 dBFS (empty) to 0 dBFS (full). */
const VU_FLOOR_DB = -60;
/** A reading at or below this, dBFS, is the floor's: `-inf`. */
const VU_SILENT_DB = -89;

/** The number of bars of a link at `dbm`: the page's mirror of `Signal::bars` in crates/svolder-
 * ipc/src/socket/signal.rs (4 from -55, 3 from -67, 2 from -80, else 1); change both together. */
export function barsOf(dbm) {
  if (!Number.isFinite(dbm)) return 0;
  if (dbm >= -55) return 4;
  if (dbm >= -67) return 3;
  if (dbm >= -80) return 2;
  return 1;
}

/** `text` with U+2212 for a leading hyphen. */
const minus = (text) => String(text).replace(/^-/, '−');

/** A rate in Hz as words: `96 kHz`, `44.1 kHz`, `800 Hz`. */
export function rateWords(hz) {
  if (!Number.isFinite(hz) || hz <= 0) return '';
  return hz >= 1000 ? `${Number((hz / 1000).toFixed(1))} kHz` : `${Math.round(hz)} Hz`;
}

/** The computer EQ's curve (181 values in dB) as `SPARK_POINTS` values from 0 (-12 dB) to 1 (+12
 * dB), or null when there is no curve. */
export function sparkOf(response) {
  if (!Array.isArray(response) || response.length < 2) return null;
  const last = response.length - 1;
  return Array.from({ length: SPARK_POINTS }, (_, i) => {
    const db = Number(response[Math.round((i / (SPARK_POINTS - 1)) * last)]);
    return clamp(0.5 + (Number.isFinite(db) ? db : 0) / (2 * SPARK_DB));
  });
}

/** A VU reading in dBFS as a row's words: `-18.0 dBFS` (U+2212), `−∞ dBFS` at silence. */
export function dbfsWords(db) {
  if (!Number.isFinite(db) || db <= VU_SILENT_DB) return `${HUD.silence} ${HUD.unit}`;
  return minus(`${db < 0 ? '' : '+'}${db.toFixed(1)} ${HUD.unit}`);
}

/** The fill of a VU row's bar for a reading in dBFS. */
export const vuBar = (db) => (Number.isFinite(db) ? clamp((db - VU_FLOOR_DB) / -VU_FLOOR_DB) : 0);

/** How fast an RMS level follows its byte (per second, up and down) and how a peak mark is held and
 * let fall (held 1.2 s, then 0.6 a second). */
const VU_EASE = Object.freeze({ up: 14, down: 6 });
const PEAK_HOLD_S = 1.2;
const PEAK_FALL_PER_S = 0.6;

/** A level's amplitude (1.0 is full scale) as dBFS, -Infinity for silence. */
const dbOf = (amp) => (amp > 0 ? 20 * Math.log10(amp) : -Infinity);

/** 
 * The two channels' levels as the HUD shows them: the RMS bytes of a levels frame (`push`) eased by
 * a time constant to a dBFS reading and a bar from -60 to 0 dBFS, the peak bytes as a held mark and
 * the over lamp (a peak byte of 255). `step(dt, now)` gives `{ L, R }`, each `{ db, text, bar,
 * peak, lamp }` (`now` in ms, the lamp's clock); `clear()` is back to silence.
 */
export class Levels {
  constructor() {
    this.clear();
  }

  clear() {
    this.target = [0, 0];
    this.rms = [0, 0];
    this.mark = [0, 0];
    this.held = [0, 0];
    this.lamps = [new Lamp(), new Lamp()];
  }

  /** The four bytes of a levels frame at page time `now` (ms). */
  push(lv, now) {
    for (let i = 0; i < 2; i++) {
      this.target[i] = byteAmplitude(lv[i]);
      const peak = peakAmplitude(lv[2 + i]);
      const bar = vuBar(dbOf(peak));
      if (bar >= this.mark[i]) {
        this.mark[i] = bar;
        this.held[i] = PEAK_HOLD_S;
      }
      this.lamps[i].push(peak, now);
    }
  }

  step(dt, now) {
    const out = {};
    ['L', 'R'].forEach((name, i) => {
      const rate = this.target[i] > this.rms[i] ? VU_EASE.up : VU_EASE.down;
      this.rms[i] += (this.target[i] - this.rms[i]) * (1 - Math.exp(-dt * rate));
      const db = dbOf(this.rms[i]);
      const bar = vuBar(db);
      if ((this.held[i] -= dt) < 0) {
        this.mark[i] = Math.max(bar, this.mark[i] - dt * PEAK_FALL_PER_S);
      }
      out[name] = {
        db, text: dbfsWords(db), bar, peak: this.mark[i], lamp: this.lamps[i].lit(now),
      };
    });
    return out;
  }
}

// ------------------------------------------------------------------ the modules' rows

const row = (id, label, text, extra = {}) => ({ id, label, text, tone: '', ...extra });

const selected = (control) => (control.options ?? []).find((o) => o.on)?.label ?? null;

/** The words of a control by its kind: a choice's selected option, a toggle's state, a slider's or
 * readout's text; null while it has none. */
function controlWords(control) {
  if (control.kind === 'choice') return selected(control);
  if (control.kind === 'toggle') {
    if (control.on === null || control.on === undefined) return null;
    return control.on ? HUD.on : HUD.off;
  }
  if (control.kind === 'slider' || control.kind === 'readout') return control.text || null;
  return null;
}

/** A row for each control of `controls` that `skip` does not name, named by its label. */
function controlRows(controls, skip = []) {
  return (controls ?? []).filter((c) => !skip.includes(c.key)).flatMap((c) => {
    const text = controlWords(c);
    const tone = c.kind === 'readout' && c.tone === 'good' ? 'on' : '';
    return text ? [row(`control:${c.key}`, c.label ?? c.key, text, { tone })] : [];
  });
}

/** The Host module's rows: output, format, EQ with the computer's curve, crossfeed, virtual
 * speakers, loudness, the two VU rows, and the path. A row whose facts the model does not carry is
 * left out. `curve` is the computer EQ's response (181 dB values) or null. */
function hostRows(page, curve) {
  const host = page?.host;
  if (!host) return [];
  const sink = host.route?.sink ?? null;
  const status = host.track?.status ?? null;
  const rows = [];
  if (sink || status) {
    const state = sink?.muted ? HUD.muted : status;
    rows.push(row('output', HUD.output, [sink?.words, state].filter(Boolean).join(' · '), {
      tone: sink?.muted || (status && status !== 'playing') ? 'warn' : '',
    }));
  }
  const rate = rateWords(sink?.rate_hz);
  if (rate || sink?.format) {
    const from = host.route?.player_rate_hz;
    const resampled = Number.isFinite(from) && Number.isFinite(sink?.rate_hz)
      && from !== sink.rate_hz;
    const was = resampled ? HUD.from(rateWords(from)) : null;
    rows.push(row('format', HUD.format, [rate, sink?.format, was].filter(Boolean).join(' · ')));
  }
  const eq = host.computer?.eq ?? null;
  if (eq) {
    const points = (eq.points ?? []).length;
    const text = eq.on
      ? [HUD.on, eq.preset, points > 0 ? HUD.points(points) : null,
        eq.running_unknown ? HUD.unconfirmed : null]
        .filter(Boolean).join(' · ')
      : HUD.off;
    const spark = eq.on ? sparkOf(curve ?? eq.response) : null;
    const slot = spark ? { slot: { kind: 'spark', id: 'eq', values: spark } } : {};
    rows.push(row('eq', HUD.eq, text, { tone: eq.on ? 'on' : '', ...slot }));
    const cross = eq.crossfeed;
    if (cross) {
      const amount = Number.isFinite(cross.amount_pct) ? `${Math.round(cross.amount_pct)} %` : null;
      const said = [HUD.on, cross.model, amount].filter(Boolean).join(' · ');
      rows.push(row('xfeed', HUD.xfeed, cross.on ? said : HUD.off, { tone: cross.on ? 'on' : '' }));
    }
    const speakers = eq.speakers;
    if (speakers) {
      const set = speakers.label || speakers.set || HUD.on;
      rows.push(row('speakers', HUD.speakers, speakers.on ? set : HUD.off,
        { tone: speakers.on ? 'on' : '' }));
    }
    const loud = eq.loudness;
    if (loud) {
      const depth = HUD.depth(minus(Number(loud.depth_db).toFixed(1)));
      const said = `${Math.round(loud.strength_pct)} % · ${depth}`;
      rows.push(row('loudness', HUD.loudness, loud.on ? said : HUD.off,
        { tone: loud.on ? 'on' : '' }));
    }
  }
  const empty = `${HUD.silence} ${HUD.unit}`;
  rows.push(row('vu-l', HUD.vuL, empty, { slot: { kind: 'bar', id: 'vu-l', value: 0 } }));
  rows.push(row('vu-r', HUD.vuR, empty, { slot: { kind: 'bar', id: 'vu-r', value: 0 } }));
  const path = pathRow(page);
  if (path) rows.push(path);
  return rows;
}

/** The Path row: the hops' labels in order, the computer's stage words after the computer and the
 * DAC's own after the DAC; amber when the path changes the sound (core/path-words.js says), with
 * the PipeWire config check's chip when the daemon sent one. */
function pathRow(page) {
  const path = page?.device?.playing?.path;
  const hops = path?.hops ?? [];
  if (hops.length === 0) return null;
  const computer = computerOf(page);
  const own = dacOf(page);
  const words = hops.flatMap((hop) => {
    if (hop.label === 'computer') return [hop.label, ...computer.stages];
    if (hop.label === 'DAC') return [hop.label, ...own];
    return [hop.label];
  });
  const changes = changedOf(page);
  const config = path.config;
  const chip = config
    ? { text: config.chip, outcome: config.outcome, summary: config.summary } : null;
  return row('path', HUD.path, words.join(' → '), {
    tone: changes ? 'warn' : '',
    ...(changes ? { note: HUD.changes } : {}),
    ...(chip ? { chip } : {}),
  });
}

/** The shown device's volume as words: `-23.5 dB`, `−∞ dB muted`, or `—` before a reading. */
function volumeWords(v) {
  const unit = v.unit || 'dB';
  if (v.unread) return HUD.noReading;
  if (v.muted) return `${HUD.silence} ${unit} ${HUD.muted}`;
  const text = String(v.text ?? '').trim();
  const cut = text.endsWith(unit) ? text.slice(0, -unit.length).trim() : text;
  return `${minus(cut)} ${unit}`;
}

/** The DAC module's rows (the shown device): volume with its bar, input and rate, the profile's own
 * controls by kind, the link with its strength and history, and the state in words. `history` is
 * the link's `LinkHistory` points, or none. */
function dacRows(page, history) {
  const device = page?.device;
  const rows = [];
  const v = device.volume;
  if (v) {
    const span = Number(v.hi) - Number(v.lo);
    const at = span > 0 && Number.isFinite(v.raw) ? clamp((v.raw - v.lo) / span) : 0;
    const slot = { kind: 'bar', id: 'volume', value: v.muted ? 0 : at };
    rows.push(row('volume', HUD.volume, volumeWords(v), { tone: v.muted ? 'warn' : '', slot }));
  }
  const controls = device.controls ?? [];
  const input = controls.find((c) => c.key === 'input' && c.kind === 'choice');
  const rate = controls.find((c) => c.key === 'sample_rate' && c.kind === 'readout');
  const inWords = [input ? selected(input) : null, rate?.text].filter(Boolean).join(' · ');
  if (inWords) rows.push(row('input', HUD.input, inWords));
  rows.push(...controlRows(controls, ['input', 'sample_rate']));
  const link = device.link;
  if (link) {
    const dbm = dbmOf(device.facts?.signal_age?.head);
    const weak = weakOf(page);
    const lost = link.state === 'off';
    const head = device.facts?.signal_age?.head;
    const text = dbm !== null && !lost ? minus(head) : link.word;
    rows.push(row('link', HUD.link, text, {
      tone: lost ? 'bad' : weak ? 'warn' : '',
      slot: {
        kind: 'link',
        id: 'link',
        bars: lost ? 0 : barsOf(dbm),
        warn: weak || lost,
        points: history ?? [],
      },
    }));
    const muted = Boolean(device.volume?.muted);
    const said = [link.word, muted ? HUD.muted : null, weak ? HUD.weak : null]
      .filter(Boolean).join(' · ');
    const tone = lost ? 'bad' : (weak || muted) ? 'warn' : 'on';
    rows.push(row('state', HUD.state, said, { tone }));
  }
  return rows;
}

/** The meters of `device` as rows (one bar each), and the words of why it reads none. */
const read = (m) => typeof m.fraction === 'number';

function meterRows(device) {
  const m = device?.meters;
  if (!m) return [];
  const rows = (m.meters ?? []).map((x) => row(`meter:${x.key}`, x.channel, x.text, {
    slot: {
      kind: 'bar', id: `meter:${x.key}`, value: read(x) ? clamp(x.fraction) : 0, off: !read(x),
    },
  }));
  if (m.state) rows.push(row('meters-state', HUD.noReading, m.state, { tone: 'warn' }));
  return rows;
}

/** The family of the shown device: the fleet entry's, else the device model's. */
const familyOf = (page) => (page?.fleet?.devices ?? []).find((d) => d.url === page?.shown)?.family
  ?? page?.device?.family;

/** The amp the Amp module reads: the shown device when it is an amp, else the amp a live hop of the
 * fleet follows the shown device with (when the page holds its model), else null. */
export function ampOf(page) {
  const shown = page?.device;
  const family = familyOf(page);
  if (shown && family === 'amp') return shown;
  const hop = (page?.fleet?.hops ?? [])
    .find((h) => h.live && h.after === page?.shown && (h.kind ?? 'amp') === 'amp');
  return hop ? page?.followed?.[hop.device] ?? null : null;
}

/** The Amp module's rows: power, its other controls (gain and the like), the tone control, loudness
 * and its meters. */
function ampRows(amp) {
  const rows = [];
  if (amp.power) {
    const p = amp.power;
    const state = p.on === true ? HUD.powerOn : p.on === false ? HUD.standby : HUD.noReading;
    const word = p.word ?? state;
    rows.push(row('power', HUD.power, word, { tone: p.on === true && !p.warming ? 'on' : 'warn' }));
    const more = [...(p.toggles ?? []), ...(p.readouts ?? [])]
      .filter((c) => c.key !== 'warming_up');
    rows.push(...controlRows(more));
  }
  rows.push(...controlRows(amp.controls, ['input', 'sample_rate']));
  const tone = amp.tone;
  if (tone?.mode) {
    const gains = (tone.bands?.rows ?? [])
      .map((r) => minus(String(r.g).replace(/ dB$/, ''))).join(' ');
    const said = tone.mode === 'eq' ? `eq${gains ? ` · ${gains} dB` : ''}` : tone.mode;
    rows.push(row('tone', HUD.tone, said, { tone: tone.mode === 'flat' ? '' : 'on' }));
    if (tone.mode === 'loudness') {
      const strength = tone.strength ? ` · ${HUD.strength(tone.strength)}` : '';
      rows.push(row('amp-loudness', HUD.loudness, `${HUD.on}${strength}`, { tone: 'on' }));
    }
  }
  rows.push(...meterRows(amp));
  return rows;
}

/** 
 * The modules the page has, in the order Host, DAC, Amp: `{ id, title, sub, rows }`. `curve` is the
 * computer EQ's response where the core has one (else the model's own), `history` the link's
 * `LinkHistory` points.
 */
export function modulesOf(page, { curve = null, history = [] } = {}) {
  const out = [];
  const host = hostRows(page, curve);
  if (host.length > 0) out.push({ id: 'host', title: HUD.host, sub: '', rows: host });
  const shown = page?.device;
  const family = familyOf(page);
  if (shown && family !== 'amp') {
    const rows = dacRows(page, history);
    if (rows.length > 0) out.push({ id: 'dac', title: HUD.dac, sub: shown.name ?? '', rows });
  }
  const amp = ampOf(page);
  if (amp) {
    const rows = ampRows(amp);
    if (rows.length > 0) out.push({ id: 'amp', title: HUD.amp, sub: amp.name ?? '', rows });
  }
  return out;
}

/** The words a row of `label` (case blind) shows in `modules`, its note after them; null for none.
 */
export function rowText(modules, label) {
  const want = String(label).toLowerCase();
  for (const m of modules) {
    const r = m.rows.find((one) => one.label.toLowerCase() === want);
    if (r) return r.note ? `${r.text} · ${r.note}` : r.text;
  }
  return null;
}

// ------------------------------------------------------------------ the top line

/** What the computer does to the sound and the DAC's own stage, as the verdict's words and whether
 * they are worth the amber (core/path-words.js says what the path names). */
function verdictOf(page) {
  const { eq, changes, stages } = computerOf(page);
  const parts = [];
  if (eq) parts.push(stages.length ? stages.join(' \u00b7 ') : HUD.hostEq);
  if (changes) parts.push(HUD.changes);
  const own = dacOf(page);
  if (own.length > 0) parts.push(`${HUD.dac} ${own.join(' \u00b7 ')}`);
  return { words: parts.join(' \u00b7 '), tone: changes || own.length > 0 ? 'warn' : '' };
}

/** 
 * The top line of `page` while the stream is in `state` (`open`, `lost`, `waiting`, `resting`):
 * `left` is `<device> \u00b7 <link word> \u00b7 <input> \u00b7 <rate>` (the link word only while
 * the link is not on, each left out when absent), `right` the verdict, replaced by the title's
 * status words (`reconnecting`, a card's title, `fault`, `reading again`, `resting`) while the page
 * is in trouble; `tone` is 'warn', 'bad' or ''. `link` is the link word shown, or null.
 */
export function topLine(page, state) {
  const device = page?.device ?? null;
  const entry = (page?.fleet?.devices ?? []).find((d) => d.url === page?.shown);
  const name = device?.name ?? entry?.name ?? '';
  const link = device?.link ?? entry?.link;
  const said = link && link.state !== 'on' ? link.word : null;
  const controls = device?.controls ?? [];
  const input = controls.find((c) => c.key === 'input' && c.kind === 'choice');
  const rate = controls.find((c) => c.key === 'sample_rate' && c.kind === 'readout');
  const left = [name, said, input ? selected(input) : null, rate?.text]
    .filter(Boolean).join(' \u00b7 ');
  const status = statusOf(page, state);
  if (status !== TITLE.connected) {
    const tone = state === 'waiting' || state === 'resting' ? 'warn' : 'bad';
    return { left, right: status, tone, link: said };
  }
  const verdict = verdictOf(page);
  return { left, right: verdict.words, tone: verdict.tone, link: said };
}

// ------------------------------------------------------------------ the link's history

/** The link's strength over the last `LINK_MS`: a sample is kept only when the reading is a new one
 * (its `heardAt` changed). */
export class LinkHistory {
  constructor() {
    this.samples = [];
    this.heard = null;
    this.shown = null;
  }

  /** The reading `dbm` heard at `heardAt` (the daemon's ms) seen at page time `now` (ms); `shown`
   * names the device, a new one starts afresh. */
  push(dbm, heardAt, now, shown = null) {
    if (shown !== this.shown) {
      this.samples = [];
      this.heard = null;
      this.shown = shown;
    }
    if (Number.isFinite(dbm) && heardAt !== this.heard) {
      this.heard = heardAt;
      this.samples.push({ at: now, dbm });
    }
    while (this.samples.length > 0 && now - this.samples[0].at > LINK_MS) this.samples.shift();
  }

  /** The samples at page time `now` as `{ x, y }` (0 to 1 along the 8 s, and the strength). */
  points(now) {
    return this.samples.filter((s) => now - s.at <= LINK_MS).map((s) => ({
      x: clamp(1 - (now - s.at) / LINK_MS),
      y: clamp((s.dbm - LINK_DBM[0]) / (LINK_DBM[1] - LINK_DBM[0])),
    }));
  }
}

// ------------------------------------------------------------------ the director's moments

/** How long a module stays after a moment, seconds, and how fast it comes and goes in. */
export const MOMENT_S = Object.freeze({ section: 4, changed: 5, rate: 4, track: 4, start: 5 });
const COME_S = 0.25;
const GO_S = 0.4;

/** The screen slot of each module (a new module in a slot sends the old one out); on a narrow
 * screen they share one. */
const SLOTS = Object.freeze({ host: 'L', dac: 'R', amp: 'BR' });
const NARROW = Object.freeze({ host: 'L', dac: 'L', amp: 'L' });

/** 
 * Which module shows and how strongly. A module shows for a time at a moment: a section change
 * (Host; the Amp at a drop), a new track (the DAC), the path starting or ceasing to change the
 * sound and a new rate (Host), and at the start; one module to a slot. `alpha(id)` is 0 to 1
 * (coming in over 0.25 s, going out over 0.4 s); while the panel is spread every module in `has` is
 * at 1.
 */
export class Moments {
  constructor() {
    this.live = {};
    this.changed = null;
    this.rate = null;
    this.started = false;
  }

  /** Show `id` for `dur` seconds; a module in the same slot goes out within 0.4 s. */
  on(id, dur, narrow = false) {
    const slots = narrow ? NARROW : SLOTS;
    for (const [other, m] of Object.entries(this.live)) {
      if (other !== id && slots[other] === slots[id]) m.dur = Math.min(m.dur, m.t + GO_S);
    }
    this.live[id] = { t: 0, dur };
  }

  /** One step of `dt` seconds: `events` the director's, `changed` whether the path changes the
   * sound, `rate` a key of the rates the page shows, `has` the modules the page has, `amp` whether
   * the drop shows the Amp, `narrow` the screen. */
  step(dt, {
    events = [], changed = false, rate = null, has = [], narrow = false,
  } = {}) {
    for (const m of Object.values(this.live)) m.t += dt;
    for (const [id, m] of Object.entries(this.live)) if (m.t > m.dur) delete this.live[id];
    if (!this.started) {
      this.started = true;
      for (const id of has) this.on(id, MOMENT_S.start, narrow);
    }
    for (const e of events) {
      if (e.type === 'section') {
        this.on('host', MOMENT_S.section, narrow);
        if (e.section === 'drop' && has.includes('amp')) this.on('amp', MOMENT_S.section, narrow);
      } else if (e.type === 'reseed' && has.includes('dac')) this.on('dac', MOMENT_S.track, narrow);
    }
    if (this.changed !== null && changed !== this.changed) {
      this.on('host', MOMENT_S.changed, narrow);
    }
    this.changed = changed;
    if (this.rate !== null && rate !== this.rate) this.on('host', MOMENT_S.rate, narrow);
    this.rate = rate;
  }

  /** Whether `id` has a moment on (its strength may still be 0 as it comes in). */
  active(id) {
    return id in this.live;
  }

  /** How strongly `id` shows now. */
  alpha(id) {
    const m = this.live[id];
    if (!m) return 0;
    return clamp(m.t / COME_S) * clamp((m.dur - m.t) / GO_S);
  }
}

// ------------------------------------------------------------------ the audio panel

/** A muted scene holds the panel to this share of its strength (the chrome's fade is the style sheet's). */
export const FADE = Object.freeze({ muted: 0.45 });

const CENT_HZ = [30, 14000];

/** The centroid feature (0 to 1 along the log axis the features read) in Hz. */
export const centroidHz = (v) => CENT_HZ[0] * (CENT_HZ[1] / CENT_HZ[0]) ** clamp(v);

/** The words of a panel feature's value, the way the panel says them: three digits for a percentage, the
 * centroid in Hz or kHz with no space, the flatness to two places and `T` (tonal) or `N` (noisy). */
export function featureWords(key, v) {
  if (key === 'cent') return rateWords(Math.round(centroidHz(v))).replace(' ', '');
  if (key === 'flat') return `${clamp(v).toFixed(2)} ${v > 0.45 ? 'N' : 'T'}`;
  return String(Math.round(clamp(v) * 100)).padStart(3, '0');
}
