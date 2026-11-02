/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// What changed on the device since the page last looked, for the sheet's "last change" note: the
// fields worth a note (the volume, the mute, every choice and toggle, the tone mode, the recalled
// preset) read from the model, compared with the last reading. Changes of one field close together
// are one change, so a drag reads as one. A change is `mine` only when this page's log holds a
// request for that field and value that it can still own (svolder keeps no record of who changed
// what). Pure: the model, the log and the time in.

import { MINE_MS } from './act.js';

/** Changes of one field this close to each other (ms) are one change: its `from` is the first's. */
export const RUN_MS = 3000;

/** The fields of a device a change is noted for, by name: `{ label, value, words }`, where `value`
 * is what the log of requests holds for the field and `words` is how the value is said. */
export function fieldsOf(device) {
  const out = new Map();
  const v = device?.volume;
  if (v && typeof v.raw === 'number') {
    out.set('volume', { label: 'volume', value: v.raw, words: v.texts?.[v.raw - v.lo] ?? v.text });
    out.set('mute', { label: 'mute', value: Boolean(v.muted), words: v.muted ? 'muted' : 'unmuted' });
  }
  for (const c of device?.controls ?? []) {
    if (c.kind === 'choice') {
      const now = (c.options ?? []).find((o) => o.on)?.label;
      if (now !== undefined) out.set(`control:${c.key}`, { label: c.label, value: now, words: now });
    } else if (c.kind === 'toggle' && typeof c.on === 'boolean') {
      out.set(`control:${c.key}`, { label: c.label, value: c.on, words: c.on ? 'on' : 'off' });
    }
  }
  const t = device?.tone;
  if (t?.mode) {
    const label = (t.modes ?? []).find((m) => m.name === t.mode)?.label ?? t.mode;
    out.set('mode', { label: 'tone', value: t.mode, words: label });
  }
  const name = device?.presets?.status?.name;
  if (name) out.set('preset', { label: 'preset', value: name, words: name });
  return out;
}

/** Whether a request in `log` makes the change of `field` to `value`, seen at `at`, this page's
 * own: asked that value of that field, not refused, and seen within `MINE_MS` of the request
 * (answered or not, so a request that never gets an answer stops counting). */
function isMine(log, field, value, at) {
  return log.some((e) => e.field === field && e.value === value && e.ok !== false
    && at - e.at <= MINE_MS);
}

/** Which of several fields that changed together is the one told, first first: a recalled preset
 * moves the mode with it, a mute comes with a volume. */
function rank(field) {
  if (field === 'preset') return 0;
  if (field === 'mode') return 1;
  if (field.startsWith('control:')) return 2;
  return field === 'volume' ? 3 : 4;
}

/** The keeper of the last change seen. */
export function createChanges() {
  let fields = null;
  let url = null;
  let last = null;

  return {
    /** A new model or device: what it holds is remembered and is no change. Another device than
     * the one remembered drops the last change. */
    baseline(device) {
      if (device?.url !== url) last = null;
      url = device?.url ?? null;
      fields = fieldsOf(device);
    },

    /** The change since the last call (the one to tell when several came together), as
     * `{ field, label, from, to, value, at, mine }` (`from` and `to` in words), or `null`. `log` is
     * act's log. The first reading is no change. */
    seen(device, at, log) {
      const now = fieldsOf(device);
      const before = fields;
      fields = now;
      url = device?.url ?? null;
      if (!before) return null;
      let found = null;
      for (const [field, f] of now) {
        const was = before.get(field);
        if (was === undefined || was.value === f.value) continue;
        const run = last?.field === field && at - last.at <= RUN_MS;
        if (found && rank(found.field) <= rank(field)) continue;
        found = {
          field,
          label: f.label,
          from: run ? last.from : was.words,
          to: f.words,
          value: f.value,
          at,
          mine: isMine(log, field, f.value, at),
        };
      }
      if (found) last = found;
      return found;
    },

    last: () => last,
  };
}
