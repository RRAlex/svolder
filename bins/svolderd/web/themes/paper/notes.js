/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The handwritten notes of the sheet: which facts of the model become a note, in whose words, and
// which one is red. A note is `{ id, about, text, red }`; `about` names the section it is written
// next to (`volume tone playing device power presets controls`, or `card`, which Paper draws under
// the title and not here). At most one note is red, the most urgent: the device's card, then a
// red note of the volume (risk, a cap), the EQ's clipping peak, a path that changes the sound, the
// passphrase note. Where a note says "to" between two values its text holds `ARROW`. Pure: the
// model in, text out.

import { elapsedUs, timeWords } from '../../core/clock.js';
import { ARROW, RATE_MATCHES } from './words.js';

/** The notes that may be red, most urgent first. */
export const RED_ORDER = ['card', 'volume', 'peak', 'path', 'passphrase'];

/** A model's own arrow (U+2192) is said as ours. */
export const handed = (text) => String(text).replaceAll('→', ARROW);

const MINUS = '−';
const MIDDLE = ' · ';

/** A dB rounded to half a dB, signed: `+3 dB`, `−1.5 dB`. */
function dbWords(db) {
  const half = Math.round(db * 2) / 2;
  return `${half < 0 ? MINUS : '+'}${Math.abs(half)} dB`;
}

/** A frequency: `60 Hz`, `12 kHz`, `2.5 kHz`. */
function hzWords(hz) {
  const whole = Math.round(hz);
  if (whole < 1000) return `${whole} Hz`;
  const k = Math.round(whole / 100) / 10;
  return `${Number.isInteger(k) ? k : k.toFixed(1)} kHz`;
}

/** The preset the bands are on, from the model's presets: `{ name, edited }`, or `null` when no
 * preset is on or recalled. The model names the recalled preset after the bands were changed
 * too, then with a status tail that says so. */
export function presetOf(presets) {
  const on = presets?.rows?.find((r) => r.on);
  if (on) return { name: on.name, edited: false };
  const name = presets?.status?.name;
  if (name === null || name === undefined || name === '') return null;
  return { name, edited: /\bedited\b/.test(presets.status.tail ?? '') };
}

/** A preset as the tone heading and the presets list name it: `Warm`, or `Warm, edited`. */
export const presetName = (preset) => (preset.edited ? `${preset.name}, edited` : preset.name);

/** What the tone's EQ is doing, in words, by mode; nothing for a mode not known. */
function eqWords(tone, preset) {
  if (tone.mode === 'flat') return 'flat: the bands are bypassed';
  if (tone.mode === 'loudness') {
    return tone.strength === null || tone.strength === undefined
      ? 'loudness' : `loudness, strength ${tone.strength}`;
  }
  if (tone.mode !== 'eq') return null;
  const moved = (tone.markers ?? []).filter((m) => Math.round(m.db * 2) !== 0)
    .sort((a, b) => Math.abs(b.db) - Math.abs(a.db)).slice(0, 2);
  if (moved.length === 0) return 'EQ on, every band at 0 dB';
  const lead = preset ? `${preset.edited ? 'EQ' : preset.name}: ` : '';
  return lead + moved.map((m) => `${dbWords(m.db)} at ${hzWords(m.hz)}`).join(', ');
}

/** The peak in words with what it means: `peak +3.3 dB: above −3.3 dB, loud tracks may clip`. */
function peakWords(peak) {
  const above = `${peak.db > 0 ? MINUS : ''}${Math.abs(peak.db).toFixed(1)}`;
  return `${peak.words}: above ${above} dB, loud tracks may clip`;
}

/** The first rate (`48 kHz`) in a hop's detail, or nothing. */
const rateIn = (detail) => /\d+(?:\.\d+)? kHz/.exec(detail ?? '')?.[0] ?? null;

/** The path note: `{ text, red }`, `red` being the words it takes when it is the red note. */
function pathWords(path) {
  const hops = path.hops ?? [];
  const rates = hops.map((h) => rateIn(h.detail)).filter(Boolean);
  const words = path.words ?? '';
  let text = words;
  if (rates.length > 1) {
    const same = rates.every((r) => r === rates[0]);
    const lead = same ? `${rates[0]}${MIDDLE}${RATE_MATCHES}` : rates.join(` ${ARROW} `);
    text = words ? `${lead}${MIDDLE}${words}` : lead;
  }
  const changing = hops.filter((h) => h.changes);
  let red = null;
  if (path.class === 'changed' && changing.length === 1) {
    red = `${changing[0].label}: ${changing[0].detail}, the only hop that changes the sound`;
  } else if (path.class === 'changed' && changing.length > 1) {
    red = `${changing.length} hops change the sound: ${changing.map((h) => h.label).join(' and ')}`;
  }
  return { text, red };
}

/** The elapsed time and the length of the track and its album, as one line, each piece left out
 * when missing (`elapsed` and `length` in microseconds). */
export function trackWords({ elapsed, length, album }) {
  const known = (us) => us !== null && us !== undefined && us !== 0;
  const has = [elapsed !== null && elapsed !== undefined, known(length)];
  let when = '';
  if (has[0] && has[1]) when = `${timeWords(elapsed)} of ${timeWords(length)}`;
  else if (has[0]) when = timeWords(elapsed);
  else if (has[1]) when = timeWords(length);
  return [when, album].filter(Boolean).join(MIDDLE);
}

/** How far into `track` it is at `clockNowMs` by the daemon's clock, in microseconds, or `null`
 * when the player gave no position. */
export function elapsedOf(track, clockNowMs) {
  const p = track.progress;
  if (!p) return null;
  return elapsedUs({
    status: track.status,
    position: p.position_us,
    rate: p.rate_milli,
    readAt: p.read_at_ms,
    length: track.length_us ?? null,
  }, clockNowMs);
}

/** The track's line, `0:30 of 20:00 · Album`, at `clockNowMs` by the daemon's clock (no time into
 * the track when the clock is not a number); the album's own arrow, if it has one, is ours. */
export function trackLine(track, clockNowMs) {
  const elapsed = Number.isFinite(clockNowMs) ? elapsedOf(track, clockNowMs) : null;
  return handed(trackWords({ elapsed, length: track.length_us, album: track.album }));
}

/** The time as `HH:MM` on a 24 hour clock. */
const clockWords = (ms) => new Date(ms).toLocaleTimeString('en-GB',
  { hour: '2-digit', minute: '2-digit', hour12: false });

/** The section a change of `field` is written next to. */
function sectionOf(field) {
  if (field === 'volume' || field === 'mute') return 'volume';
  if (field === 'mode') return 'tone';
  if (field === 'preset') return 'presets';
  return 'controls';
}

/** What the last change was, in words, by its field. */
function changeWords(c) {
  if (c.field === 'volume') return `${c.from} ${ARROW} ${c.to}`;
  if (c.field === 'mute') return c.to;
  if (c.field === 'mode') return `tone ${c.from} ${ARROW} ${c.to}`;
  if (c.field === 'preset') return `preset ${c.to}`;
  return typeof c.value === 'boolean' ? `${c.label} ${c.to}` : `${c.label} ${c.from} ${ARROW} ${c.to}`;
}

/**
 * The notes of the sheet for `device` on `page`, in the order they are written: `[{ id, about,
 * text, red }]`. `change` is the last change (changes.js) or `null`; `coarse` says a phone (a touch
 * screen) for `from this phone`; `clockNowMs` is the daemon's clock now for the track's time (the
 * part's own time, else; no time into the track when neither is a number).
 */
export function notesOf(page, device, { change = null, coarse = false, clockNowMs } = {}) {
  const notes = [];
  const add = (id, about, text, redKey = null, redText = null) => {
    if (text !== null && text !== undefined && text !== '') {
      notes.push({ id, about, text: handed(text), redKey, redText: redText && handed(redText) });
    }
  };

  const card = device?.card ?? page?.card;
  if (card) add('card', 'card', card.title, 'card');

  // The volume's own note about the EQ's peak and the tone's peak note say one thing: when the
  // volume's note is red it says it, else the peak note does and carries the threshold.
  const tone = device?.tone;
  const volumeNotes = device?.volume?.notes ?? [];
  const atRisk = volumeNotes.some((n) => !n.popup && n.red);
  const peakSaid = Boolean(tone?.peak) && !atRisk;
  let redVolume = true;
  volumeNotes.forEach((n, i) => {
    if (n.popup || (n.eq && peakSaid)) return;
    add(`volume-${i}`, 'volume', n.text, n.red && redVolume ? 'volume' : null);
    if (n.red) redVolume = false;
  });

  if (peakSaid) add('peak', 'tone', peakWords(tone.peak), 'peak');
  if (tone) add('eq', 'tone', eqWords(tone, presetOf(device.presets)));

  const p = device?.playing ?? page?.host ?? null;
  if (p?.path) {
    const { text, red } = pathWords(p.path);
    add('path', 'playing', text, red ? 'path' : null, red);
  }
  const track = p?.track;
  if (track) {
    add('track', 'playing', trackLine(track, clockNowMs ?? p.now_ms));
  }

  const facts = device?.facts;
  if (facts) {
    const dsp = facts.dsp === null || facts.dsp === undefined ? '' : `${MIDDLE}DSP ${facts.dsp}`;
    if (facts.firmware) add('firmware', 'device', `firmware ${facts.firmware}${dsp}`);
    if (facts.signal) add('link', 'device', `link ${facts.signal}`);
  }

  for (const r of device?.power?.readouts ?? []) {
    add(`power-${r.key}`, 'power', `${String(r.label).toLowerCase()}: ${r.text}`);
  }
  if (device?.presets?.note) add('presets', 'presets', device.presets.note);
  if (facts?.passphrase_note) add('passphrase', 'device', facts.passphrase_note, 'passphrase');

  if (change) {
    const mine = change.mine ? `, from this ${coarse ? 'phone' : 'browser'}` : '';
    add('change', sectionOf(change.field),
      `last change: ${changeWords(change)}, ${clockWords(change.at)}${mine}`);
  }

  const redKey = RED_ORDER.find((key) => notes.some((n) => n.redKey === key));
  const redId = notes.find((n) => n.redKey === redKey)?.id;
  return notes.map((n) => ({
    id: n.id,
    about: n.about,
    text: n.id === redId && n.redText ? n.redText : n.text,
    red: n.id === redId,
  }));
}
