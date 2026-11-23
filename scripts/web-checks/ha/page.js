// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later
// The check page for the svolder cards: loads the resource as Home Assistant does, mounts each card
// over a mocked `hass` and records every service call in `window.__calls`. `?mode=dark|light|hadark` (the svolder
// theme, Home Assistant's default light, its default dark), `?commands=off` makes every control a sensor.
import '../../../contrib/home-assistant/svolder.js';

const q = new URLSearchParams(location.search);
document.body.className = ['light', 'hadark'].includes(q.get('mode')) ? q.get('mode') : 'dark';
const off = q.get('commands') === 'off';
const stamp = new Date(Date.now() - 30 * 60000).toISOString();

const raw = {
  'number.desk_volume': ['-40', { min: -96.33, max: 0, step: 0.334, unit_of_measurement: 'dB' }],
  'switch.desk_mute': ['off'], 'switch.desk_phase_invert': ['off'],
  'select.desk_input': ['optical', { options: ['usb', 'optical', 'coax'] }],
  'select.desk_eq_preset': ['open ref', { options: ['flat', 'open ref', 'warm', 'custom'] }],
  'sensor.desk_sample_rate': ['48', { unit_of_measurement: 'kHz' }],
  'binary_sensor.desk_s_pdif_locked': ['on'], 'binary_sensor.desk_usb_1_connected': ['on'], 'binary_sensor.desk_usb_2_connected': ['off'],
  'binary_sensor.desk_link': ['on'], 'sensor.desk_signal': ['-58', { unit_of_measurement: 'dBm' }], 'sensor.desk_firmware': ['3'],
  'number.desk_balance': ['0', { min: -127, max: 127, step: 1 }],
  'select.desk_tone_control_mode': ['eq', { options: ['flat', 'eq', 'loudness'] }],
  'number.desk_loudness_strength': ['0', { min: 0, max: 127, step: 1 }],
  'switch.pc_eq': ['on'], 'sensor.pc_eq_preset': ['open ref'],
  'switch.pc_loudness': ['off'], 'number.pc_loudness_strength': ['64', { min: 0, max: 127, step: 1 }],
  'switch.pc_crossfeed': ['on'], 'select.pc_crossfeed_preset': ['mild', { options: ['mild', 'strong'] }],
  'switch.pc_virtual_speakers': ['off'], 'select.pc_virtual_speakers_set': ['near', { options: ['near', 'far'] }],
  'sensor.pc_path': ['direct'], 'binary_sensor.pc_pipewire_config': ['off'],
  'select.pc_scene': ['Late night', {
    options: ['Late night', 'Desk', 'Movie', 'custom'], known: true,
    scenes: [
      { name: 'Late night', parts: [{ who: 'computer', present: true }, { who: 'Desk DAC', present: true }] },
      { name: 'Desk', parts: [{ who: 'computer', present: true }, { who: 'Desk DAC', present: true }] },
      { name: 'Movie', parts: [{ who: 'computer', present: true }, { who: 'Desk DAC', present: true }, { who: 'Hall', present: false }] },
    ],
  }],
  'text.pc_scene_save': [''],
};
[[31, 6, 0.5], [213, -2.6, 1], [1232, -2.2, 0.5], [1736, 4.8, 0.9], [14801, -4.3, 0.5]].forEach(([f, g, qq], i) => {
  const n = i + 1;
  raw[`number.desk_band_${n}_frequency`] = [String(f), { min: 20, max: 20000, step: 1 }];
  raw[`number.desk_band_${n}_gain`] = [String(g), { min: -12, max: 12, step: 0.1 }];
  raw[`number.desk_band_${n}_q`] = [String(qq), { min: 0.1, max: 10, step: 0.1 }];
});

const sensorId = (id) => (off ? id.replace(/^(number|select)\./, 'sensor.').replace(/^(switch|text)\./, 'binary_sensor.') : id);
const states = {};
for (const [id, [state, attrs = {}]] of Object.entries(raw)) {
  if (off && id.startsWith('text.')) continue;
  const { options, ...noOptions } = attrs;
  states[sensorId(id)] = {
    entity_id: sensorId(id), state, attributes: off && !id.startsWith('select.pc_scene') ? noOptions : attrs,
    last_updated: stamp, last_changed: stamp,
  };
}
window.__calls = [];
const hass = { states, callService: (domain, service, data) => window.__calls.push([domain, service, data]) };
const e = (id) => sensorId(id);

const cards = {
  'svolder-faceplate-card': {
    name: 'Desk DAC',
    entities: {
      volume: e('number.desk_volume'), mute: e('switch.desk_mute'), phase: e('switch.desk_phase_invert'), input: e('select.desk_input'),
      preset: e('select.desk_eq_preset'), sample_rate: 'sensor.desk_sample_rate', spdif: 'binary_sensor.desk_s_pdif_locked',
      usb1: 'binary_sensor.desk_usb_1_connected', usb2: 'binary_sensor.desk_usb_2_connected', link: 'binary_sensor.desk_link',
      balance: e('number.desk_balance'), signal: 'sensor.desk_signal', firmware: 'sensor.desk_firmware',
    },
  },
  'svolder-eq-card': {
    name: 'EQ',
    entities: {
      preset: e('select.desk_eq_preset'), mode: e('select.desk_tone_control_mode'), strength: e('number.desk_loudness_strength'),
      bands: [1, 2, 3, 4, 5].map((n) => ({
        n, gain: e(`number.desk_band_${n}_gain`), frequency: e(`number.desk_band_${n}_frequency`), q: e(`number.desk_band_${n}_q`),
      })),
    },
  },
  'svolder-chain-card': {
    name: 'pc',
    entities: {
      eq: e('switch.pc_eq'), eq_preset: 'sensor.pc_eq_preset', loudness: e('switch.pc_loudness'),
      loudness_strength: e('number.pc_loudness_strength'), crossfeed: e('switch.pc_crossfeed'),
      crossfeed_preset: e('select.pc_crossfeed_preset'), speakers: e('switch.pc_virtual_speakers'),
      speakers_set: e('select.pc_virtual_speakers_set'), path: 'sensor.pc_path', config: 'binary_sensor.pc_pipewire_config',
    },
  },
  'svolder-scenes-card': { entities: { scene: e('select.pc_scene'), ...(off ? {} : { save: 'text.pc_scene_save' }) } },
};
const main = document.getElementById('m');
for (const [tag, config] of Object.entries(cards)) {
  const el = document.createElement(tag);
  el.id = tag;
  main.appendChild(el);
  el.setConfig({ type: `custom:${tag}`, ...config });
  el.hass = hass;
}
window.__hass = hass;
window.__ready = true;
