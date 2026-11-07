/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The HUD's words, worked out from the model: the top line (the device, its input and rate, and
// the path's verdict, which trouble replaces), the EQ line and the volume. No DOM here; the
// drawing is in draw/hud.js and draw/volume.js.

import { TITLE, statusOf } from '../../core/title.js';
import { HUD } from './words.js';

/** The shown device's entry in the fleet's list, or `null`. */
export const entryOf = (page) => (page?.fleet?.devices ?? []).find((d) => d.url === page.shown) ?? null;

/** The device's input (its chosen option's label) and rate (the readout's text), where it has them. */
function inputAndRate(device) {
  const controls = device?.controls ?? [];
  const input = controls.find((c) => c.key === 'input' && c.kind === 'choice');
  const rate = controls.find((c) => c.key === 'sample_rate' && c.kind === 'readout');
  return [input?.options?.find((o) => o.on)?.label, rate?.text];
}

/** What the computer does to the sound, in words, and whether it is worth the amber: the path's
 * flag for the computer, with the route's host EQ when the path has none. */
function verdictOf(page) {
  const hop = page?.device?.playing?.path?.hops?.find((h) => h.label === 'computer');
  const eq = page?.host?.route?.host_eq === true;
  const changes = hop ? hop.changes === true : eq;
  if (eq) return { words: changes ? `${HUD.hostEq} · ${HUD.changes}` : HUD.hostEq, tone: changes ? 'warn' : null };
  return changes ? { words: HUD.changes, tone: 'warn' } : { words: '', tone: null };
}

/**
 * The top line of `page` while the stream is in `state`: `left` is `<device> · <link word> · <input> ·
 * <rate>` (the link word only while the link is not on, each left out when absent), `right` the verdict,
 * replaced by the core title's status words (`reconnecting`, a card's title, `fault`, `reading again`,
 * `resting`) while the page is in trouble.
 * `tone` is `'warn'` (the verdict, or a page reading again), `'bad'` (a link lost, a card, a
 * fault) or `null`.
 */
export function topLine(page, state) {
  const device = page?.device ?? null;
  const name = device?.name ?? entryOf(page)?.name ?? '';
  const [input, rate] = inputAndRate(device);
  const link = device?.link;
  const said = link && link.state !== 'on' ? link.word : null;
  const left = [name, said, input, rate].filter(Boolean).join(' · ');
  const status = statusOf(page, state);
  if (status !== TITLE.connected) {
    return { left, right: status, tone: state === 'waiting' || state === 'resting' ? 'warn' : 'bad' };
  }
  const verdict = verdictOf(page);
  return { left, right: verdict.words, tone: verdict.tone };
}

/** A gain as the line writes it: a sign and one decimal, `0.0` bare, `−` U+2212. The model writes a
 * negative gain with U+2212, which `parseFloat` does not read as a sign. */
export function gainWords(text) {
  const db = Number.parseFloat(String(text).replace('\u2212', '-'));
  if (!Number.isFinite(db)) return '—';
  const tenths = Math.round(Math.abs(db) * 10) / 10;
  if (tenths === 0) return '0.0';
  return `${db < 0 ? '−' : '+'}${tenths.toFixed(1)}`;
}

/** The EQ line of the computer's tone (core `toneOf`): `OFF`, or `EQ` with the preset's name when one is on;
 * no gains, and a word of it while the EQ's state is only what its file says. `null` with no tone. */
function computerEqLine(tone) {
  if (!tone) return null;
  const on = tone.modes.find((m) => m.name === 'eq')?.on === true;
  const gains = tone.verified ? [] : [HUD.unconfirmed];
  if (!on) return { lead: '', name: HUD.off, gains };
  return tone.preset
    ? { lead: HUD.eq, name: String(tone.preset).toUpperCase(), gains }
    : { lead: '', name: HUD.eq, gains };
}

/**
 * The EQ line: `{ lead, name, gains }`, `name` the bright part. In EQ it is `EQ` then the preset's
 * name when one is on (`lead: 'EQ'`, `name: 'WARM'`, else `lead: ''`, `name: 'EQ'`) and the
 * bands' gains; Loudness `LOUDNESS 3` and Flat `FLAT` have none. `null` for a device with no tone. With the
 * computer as the side shown (`comp`, ./computer-view.js) it is the computer's: `OFF` or `EQ` and the preset.
 */
export function eqLine(page, comp = null) {
  if (comp?.side === 'computer') return computerEqLine(comp.tone);
  const device = page?.device;
  const tone = device?.tone;
  if (!tone) return null;
  if (tone.mode === 'loudness') {
    const strength = tone.strength === null || tone.strength === undefined ? '' : ` ${tone.strength}`;
    return { lead: '', name: `${HUD.loudness}${strength}`, gains: [] };
  }
  if (tone.mode === 'flat') return { lead: '', name: HUD.flat, gains: [] };
  const gains = (tone.bands?.rows ?? []).map((r) => gainWords(r.g));
  const preset = device.presets?.status?.name;
  return preset
    ? { lead: HUD.eq, name: String(preset).toUpperCase(), gains }
    : { lead: '', name: HUD.eq, gains };
}

/** The name of the DAC an amp is linked after, or `null`. */
function dacOf(page) {
  const row = (page.fleet?.hops ?? []).find((h) => h.device === page.shown && h.live);
  return (page.fleet?.devices ?? []).find((d) => d.url === row?.after)?.name ?? null;
}

/**
 * The volume: `figures` (`−23.5`, `−∞` while muted, with U+2212), its `unit`, whether it is
 * `muted`, the `aria` words, and `none`, the words that stand in the figures' place for an amp
 * with no volume of its own (else `null`). With the computer as the side shown it is the computer's: `42`
 * and `%`, the figure kept (and dimmed) while muted, `—` and no unit before the first read.
 */
export function volumeOf(page, comp = null) {
  if (comp?.side === 'computer') {
    const c = comp.volume;
    return {
      figures: c.text, unit: c.pct === null ? '' : c.unit, muted: c.muted, dim: c.muted, aria: c.aria, none: null,
    };
  }
  const v = page?.device?.volume;
  if (!v) {
    const amp = entryOf(page)?.family === 'amp';
    return {
      figures: '', unit: '', muted: false, aria: '', none: amp ? HUD.noVolume(dacOf(page)) : null,
    };
  }
  const unit = v.unit || HUD.defaultUnit;
  const muted = Boolean(v.muted);
  const text = String(v.text ?? '').trim();
  const cut = text.endsWith(unit) ? text.slice(0, -unit.length).trim() : text;
  const figures = muted ? HUD.silence : cut.replace(/^[-\u2212]/, '−');
  const aria = muted
    ? `muted, minus infinity ${unit}`
    : `${figures.replace('−', 'minus ')} ${unit}`.trim();
  return { figures, unit, muted, aria, none: null };
}
