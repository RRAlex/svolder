/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The thin writes to a device: its controls, EQ mode and bands, loudness, presets, slots,
// settings, connect and the Identify cue. Each is one POST to the device its last `options` name
// (`{ device }`), finished as write.js says.

import { identifyWords } from './words.js';
import { NO_DEVICE, REFUSED } from './write.js';

/**
 * The device writes over `store`, posting with `post` and saying things with `toast`; `writes` is
 * the core's `createWrites`.
 */
export function createDeviceActions({ store, post, toast, writes }) {
  const { turnedAway, target, write } = writes;

  const deviceActions = {
    setControl: (key, body, options) => write(`controls/${key}`, body, options),
    setMode: (mode, options) => write('eq/mode', { mode }, options),
    setBand: (band, param, body, options) => write(`eq/bands/${band}`, { param, ...body }, options,
      REFUSED),
    setLoudness: (body, options) => write('eq/loudness', body, options, REFUSED),
    recallPreset: (name, options) => write('presets/recall', { name }, options),
    savePreset: (name, options) => write('presets', { name }, options),
    renamePreset: (from, to, options) => write('presets/rename', { from, to }, options),
    deletePreset: (name, options) => write('presets/delete', { name }, options),
    recallSlot: (slot, options) => write('device-presets/recall', { slot }, options),
    saveSlot: (slot, confirmed, options) => write('device-presets/save',
      { slot, confirm: Boolean(confirmed) }, options),
    applySetting: (name, options) => write('device-presets/apply', { name }, options),
    connect: (options) => write('connect', {}, options, 'The device did not answer.'),

    /** The Identify cue; the toast is its words, with the gap the device's facts carry. */
    async identify(options) {
      const away = turnedAway();
      if (away) return away;
      const url = target(options);
      if (!url) return { ok: false, status: 0, error: NO_DEVICE };
      const answer = await post(`api/v1/devices/${url}/identify`, {});
      if (answer.status !== 401) {
        const gap = store.part('facts', url)?.identify_gap_secs ?? 2;
        toast(identifyWords(answer, String(gap)));
      }
      return { ok: answer.status === 200, ...answer };
    },
  };

  return deviceActions;
}
