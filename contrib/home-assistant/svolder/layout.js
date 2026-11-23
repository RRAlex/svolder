// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later
// The cards of one computer's page and of each device's section: svolder's own cards (the chain,
// scenes, faceplate and EQ cards, in cards/) under Home Assistant headings, and Home Assistant's
// entities card for what no svolder card shows. Every function is pure: entities in, config out.
import { take } from './find.js';

const grid = (cards, extra = {}) => ({ type: 'grid', cards, ...extra });
const heading = (text, icon, extra = {}) => ({ type: 'heading', heading: text, icon, ...extra });
const custom = (name, config) => ({
  type: `custom:svolder-${name}-card`, ...config, grid_options: { columns: 'full' },
});
/** The entity ids of `roles` (`{ role: entry|null }`), left out when no entity has the role. */
const ids = (roles) => Object.fromEntries(Object.entries(roles).filter(([, e]) => e).map(([k, e]) => [k, e.entity_id]));
const dm = (dom) => [dom, { number: 'sensor', select: 'sensor', switch: 'binary_sensor' }[dom]];
/** Like `take`, but the entity stays in the pool (a card shows it AND the entities card keeps it). */
const peek = (pool, domains, slugs, owner) => take(pool.slice(), domains, slugs, owner);

/** The computer's own sections: the scenes, then the chain. `entities` is its entries. */
export function computerSection(title, entities) {
  const pool = entities.slice();
  const get = (dom, slugs) => take(pool, dom, slugs, title);
  const chain = ids({
    eq: get(['switch', 'binary_sensor'], 'eq'),
    eq_preset: get(['select', 'sensor'], 'eq_preset'),
    preamp: get(dm('number'), ['eq_preamp', 'preamp']),
    loudness: get(['switch', 'binary_sensor'], 'loudness'),
    loudness_strength: get(dm('number'), ['loudness_strength']),
    crossfeed: get(['switch', 'binary_sensor'], 'crossfeed'),
    crossfeed_preset: get(['select', 'sensor'], 'crossfeed_preset'),
    speakers: get(['switch', 'binary_sensor'], 'virtual_speakers'),
    speakers_set: get(['select', 'sensor'], 'virtual_speakers_set'),
    path: get('sensor', 'path'),
    config: get(['binary_sensor', 'sensor'], ['pipewire_config', 'config']),
  });
  const scene = get(dm('select'), 'scene');
  const sceneSave = get('text', 'scene_save');

  // the scenes first, then the computer's chain, as the approved layout draws the right column
  const sections = [];
  if (scene) {
    sections.push(grid([
      heading('Scenes', 'mdi:palette'),
      custom('scenes', { entities: ids({ scene, save: sceneSave }) }),
    ]));
  }
  if (Object.keys(chain).length) {
    sections.push(grid([heading(title, 'mdi:laptop'), custom('chain', { name: title, entities: chain })]));
  }
  const rest = pool.filter((e) => e.entity_category !== 'diagnostic');
  const diag = pool.filter((e) => e.entity_category === 'diagnostic');
  return { sections, rest, diag };
}

/** One device's sections: the faceplate, then the EQ. `title` the device's name, `entities` its entries. */
export function deviceSections(title, entities) {
  const pool = entities.slice();
  const get = (dom, slugs) => take(pool, dom, slugs, title);
  const eqPreset = get(dm('select'), 'eq_preset');
  const preset = eqPreset ? null : get(dm('select'), 'preset');
  const faceplate = ids({
    volume: get(dm('number'), 'volume'),
    mute: get(dm('switch'), 'mute'),
    phase: get(dm('switch'), 'phase_invert'),
    input: get(dm('select'), 'input'),
    preset: eqPreset || preset,
    sample_rate: get('sensor', 'sample_rate'),
    spdif: get('binary_sensor', 's_pdif_locked'),
    usb: get('binary_sensor', 'usb_connected'),
    usb1: get('binary_sensor', 'usb_1_connected'),
    usb2: get('binary_sensor', 'usb_2_connected'),
    link: get('binary_sensor', 'link'),
    balance: get(dm('number'), 'balance'),
    signal: get('sensor', 'signal'),
    firmware: peek(pool, 'sensor', 'firmware', title),
  });

  const mode = peek(pool, dm('select'), 'tone_control_mode', title);
  const strength = peek(pool, dm('number'), 'loudness_strength', title);
  const bands = [];
  for (let n = 1; n <= 12; n += 1) {
    const b = {
      gain: get(dm('number'), `band_${n}_gain`),
      frequency: get(dm('number'), `band_${n}_frequency`),
      q: get(dm('number'), `band_${n}_q`),
    };
    if (b.gain || b.frequency || b.q) bands.push({ n, ...ids(b) });
  }

  // the EQ card shows the mode as chips, so the entities card drops it when that card exists
  if (bands.length && mode) pool.splice(pool.indexOf(mode), 1);

  const sections = [];
  if (Object.keys(faceplate).length) {
    sections.push(grid([heading(title, 'mdi:amplifier'), custom('faceplate', { name: title, entities: faceplate })]));
  }
  if (bands.length) {
    sections.push(grid([
      heading(`${title}: EQ`, 'mdi:equalizer'),
      custom('eq', {
        name: 'EQ',
        entities: { preset: faceplate.preset, ...ids({ mode, strength }), bands },
      }),
    ]));
  }
  const rest = pool.filter((e) => e.entity_category !== 'diagnostic');
  const diag = pool.filter((e) => e.entity_category === 'diagnostic');
  return { sections, rest, diag };
}

/** A plain entities card listing `entries`, or none for an empty list. */
export function entitiesSection(heading_text, icon, entries) {
  if (!entries.length) return null;
  return grid([
    heading(heading_text, icon),
    { type: 'entities', entities: entries.map((e) => ({ entity: e.entity_id })), grid_options: { columns: 'full' } },
  ]);
}
