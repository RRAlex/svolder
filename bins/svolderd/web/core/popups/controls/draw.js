/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The Device controls popup's draws: a model in, a string out. Pure, with no page and no core.
//
// The popup is the controls of the page by kind, for a theme that has no sheet of its own: the shown
// device's (`targetsOf` of core/contract.js lists each one) and, flipped to the computer, the
// computer's (`computerTargetsOf`). Every word is the model's (the daemon's); this file only sets
// them out. Every control is a button, a field or a pill of frame.css's classes and carries the
// attributes the wire and the core's driver find it by:
//
//   data-act      what a tap does: `volume-down`, `volume-up`, `mute`, `mode`, `loudness`, `band`,
//                 `preset`, `preset-save`, `preset-rename`, `preset-delete`, `slot`, `slot-save`,
//                 `setting`, `choice`, `toggle`, `slider`, `power`, `power-toggle`, `transport`;
//                 the computer's `c-volume-down`, `c-volume-up`, `c-mute`, `c-mode`, `c-bypass`,
//                 `c-add`, `c-remove`, `c-point` (a field or the type menu), `c-preset`; `side`
//                 (the Device | This computer switch) and `sound` (the link to the Sound popup)
//   data-key      the control's key (`data-act` of a control of the model)
//   data-band, data-param, data-step, data-name, data-slot, data-value, data-mode, data-button,
//   data-n        the rest of what a tap names
//   data-readout  a readout's key; `data-shown="volume"` the volume's figure
//
// A viewer who may not write sees every control drawn inert (`aria-disabled="true"`, never
// `disabled`, so a tap still reaches the wire, which says the core's words) and a field read-only.
// Where the model offers no choice (no volume, no tone, no presets) the section is left out.
//
// The model of a draw:
//
//   device    the shown device's model, or `null`
//   computer  `{ sw, volume, tone, levels, bypass }` of core/computer.js (`view`, `volumeOf`,
//             `toneOf`, `levelsOf`, `bypassOf`), or `null` where the page has no computer's side
//   side      `'dac'` or `'computer'`: what is drawn
//   inert     whether the viewer may change nothing
//   writer    whether the viewer has the Sound popup (a writer)

import { attr, esc } from '../../html.js';
import { POINT_TYPES } from '../../computer.js';
import { gainText, hzText, qText } from '../../words.js';
import { dot, heading, rows } from '../frame.js';

/** The ids of the places the wire draws into. */
export const IDS = {
  side: 'ctl-side', say: 'ctl-say', body: 'ctl-body', ask: 'ctl-ask', cask: 'ctl-cask',
};

/** The popup's own words: what the daemon did not send. */
export const WORDS = {
  title: 'Device controls',
  device: 'Device',
  computer: 'This computer',
  switchFor: 'Controls of',
  noDevice: 'No device is shown.',
  noControls: 'This device has no controls to show here.',
  volume: 'Volume',
  lower: 'Lower the volume',
  raise: 'Raise the volume',
  mute: 'MUTE',
  muted: '−∞ dB',
  power: 'Power',
  controls: 'Controls',
  mode: 'Tone',
  loudness: 'Loudness strength',
  bands: 'Bands',
  presets: 'Presets',
  saveAs: 'Save as…',
  rename: 'Rename…',
  remove: 'Remove…',
  slots: 'On the device',
  settings: 'Built in',
  saveOver: 'Save…',
  player: 'Player',
  previous: 'Previous',
  next: 'Next',
  play: 'Play',
  pause: 'Pause',
  points: 'Points',
  addPoint: 'Add point',
  bypass: 'Bypass',
  levels: 'Levels',
  total: 'Total',
  off: 'off',
  sound: 'Sound…',
  soundNote: 'Loudness, crossfeed, virtual speakers and the EQ library are in the Sound popup.',
  notRead: 'not read yet',
  headers: ['Type', 'Frequency', 'Gain', 'Q'],
  param: { f: 'frequency', g: 'gain', q: 'Q' },
  answers: {
    save: 'save', rename: 'rename', delete: 'delete', keep: 'keep', cancel: 'cancel',
  },
  newName: 'New preset name',
  presetName: 'Preset name',
  askDelete: (name) => `Delete the preset ${name}?`,
  askSlot: (name) => `Write the bands over ${name} on the device?`,
};

/** One button: `act` and the `data` it carries, `text` and its `label` (the accessible name when the
 * text is a sign), `pressed` for a switch (absent: not one), `off` for a control that cannot be
 * used now (a tap on it is the wire's to answer). */
function button({
  id, act, text, label, pressed, off, cls = 'fi-btn', data = {},
}) {
  const extra = Object.entries(data).map(([key, value]) => attr(`data-${key}`, String(value))).join('');
  return `<button type="button" class="${cls}"${attr('id', id)}${attr('data-act', act)}${extra}`
    + `${attr('aria-label', label)}${pressed === undefined ? '' : attr('aria-pressed', String(pressed))}`
    + `${off ? ' aria-disabled="true"' : ''}>${esc(text)}</button>`;
}

/** A pill of a group (`fp-pill`), `on` when it is the one chosen. */
function pill({ id, act, text, on, off, data = {} }) {
  return button({
    id, act, text, pressed: on, off, cls: `fp-pill${on ? ' on' : ''}`, data,
  });
}

/** `−` and `+` around the words of a value: `less` and `more` are the buttons' parts (`id`, `act`,
 * `label`, `data`) and `value` markup. */
function stepper({ less, more, value, off }) {
  const step = (one, sign, dir) => button({
    ...one, text: sign, off, cls: 'fi-btn fpx-step', data: { ...one.data, step: dir },
  });
  return `<span class="fpx-steps">${step(less, '−', -1)}${value}${step(more, '+', 1)}</span>`;
}

/** A section: its heading and `body`, in a place of its own so the wire puts it alone. */
const section = (id, words, body) => ({
  id, html: `${words ? heading(words) : ''}${body}`,
});

/** The words of a device's volume figure: the model's, `−∞ dB` while it is muted. */
export const volumeWords = (volume) => (volume.muted ? WORDS.muted : (volume.text ?? ''));

/** The figure of a value, in an element a reader announces politely. */
const figure = (words, shown) => `<output class="fpd-mono"${attr('data-shown', shown)}>${esc(words)}</output>`;

// ---- the device's side ----

function volumeSection(device, inert) {
  const volume = device.volume;
  if (!volume) return null;
  const mute = volume.mute
    ? button({
      id: 'ctl-mute', act: 'mute', text: WORDS.mute, pressed: volume.muted === true, off: inert,
      data: { key: volume.mute },
    }) : '';
  return section('ctl-volume', WORDS.volume, `<div class="fp-pills">${stepper({
    less: { id: 'ctl-volume-down', act: 'volume-down', label: WORDS.lower, data: { repeat: 1 } },
    more: { id: 'ctl-volume-up', act: 'volume-up', label: WORDS.raise, data: { repeat: 1 } },
    value: `${mute}${figure(volumeWords(volume), 'volume')}`,
    off: inert,
  })}</div>`);
}

/** The power key, its switches and its readouts. */
function powerSection(device, inert) {
  const power = device.power;
  if (!power) return null;
  const main = button({
    id: 'ctl-power', act: 'power', text: power.word ?? WORDS.power, pressed: power.on === true,
    label: `${WORDS.power}: ${power.word ?? ''}`.trim(), off: inert,
  });
  const toggles = (power.toggles ?? []).map((toggle) => button({
    id: `ctl-power-${toggle.key}`, act: 'power-toggle', text: toggle.label,
    pressed: toggle.on === true, off: inert, data: { key: toggle.key },
  })).join('');
  const readouts = (power.readouts ?? []).map(readoutRow);
  return section('ctl-power', WORDS.power, `<div class="fp-pills">${main}${toggles}</div>`
    + `${readouts.length ? rows(readouts) : ''}`);
}

/** A readout as a row of labels and values. */
function readoutRow(control) {
  return {
    attrs: attr('data-readout', control.key),
    label: esc(control.label),
    value: esc(control.text ?? ''),
  };
}

/** One control of the model by its kind. `inert` draws it quiet. */
function controlHtml(control, inert) {
  const label = esc(control.label);
  if (control.kind === 'choice') {
    const pills = (control.options ?? []).map((option, i) => pill({
      id: `ctl-c-${control.key}-${i}`, act: 'choice', text: option.label, on: option.on === true,
      off: inert, data: { key: control.key, value: option.label },
    })).join('');
    return `<div class="fp-pills" role="group" aria-label="${label}"`
      + `${attr('data-key', control.key)}>${pills}</div>`;
  }
  if (control.kind === 'toggle') {
    return `<div class="fp-pills">${button({
      id: `ctl-c-${control.key}`, act: 'toggle', text: control.label, pressed: control.on === true,
      off: inert, data: { key: control.key },
    })}</div>`;
  }
  if (control.kind === 'slider') {
    const words = control.text ?? String(control.raw ?? '');
    return `<div class="fp-pills" role="group" aria-label="${label}">${stepper({
      less: {
        id: `ctl-s-${control.key}-less`, act: 'slider', label: `Lower ${control.label}`,
        data: { key: control.key },
      },
      more: {
        id: `ctl-s-${control.key}-more`, act: 'slider', label: `Raise ${control.label}`,
        data: { key: control.key },
      },
      value: figure(words, control.key),
      off: inert,
    })}</div>`;
  }
  return esc(control.text ?? '');
}

function controlsSection(device, inert) {
  const list = device.controls ?? [];
  if (list.length === 0) return null;
  const readouts = list.filter((control) => control.kind === 'readout').map(readoutRow);
  const sets = list.filter((control) => control.kind !== 'readout').map((control) => {
    const head = control.kind === 'toggle' ? '' : `<h4 class="fp-heading">${esc(control.label)}</h4>`;
    return `<div data-control="${esc(control.key)}">${head}${controlHtml(control, inert)}</div>`;
  });
  return section('ctl-controls', WORDS.controls, `${sets.join('')}${readouts.length ? rows(readouts) : ''}`);
}

/** The tone modes (flat, EQ, loudness) and the loudness strength while the mode is loudness. */
function toneSections(device, inert) {
  const tone = device.tone;
  if (!tone) return [];
  const out = [];
  if ((tone.modes ?? []).length > 0) {
    out.push(section('ctl-mode', WORDS.mode, `<div class="fp-pills" role="group" aria-label="${WORDS.mode}">${
      tone.modes.map((mode) => pill({
        id: `ctl-mode-${mode.name}`, act: 'mode', text: mode.label, on: mode.on === true, off: inert,
        data: { mode: mode.name },
      })).join('')}</div>`));
  }
  if (tone.loud) {
    out.push(section('ctl-loudness', WORDS.loudness, `<div class="fp-pills">${stepper({
      less: { id: 'ctl-loudness-less', act: 'loudness', label: `Lower ${WORDS.loudness}` },
      more: { id: 'ctl-loudness-more', act: 'loudness', label: `Raise ${WORDS.loudness}` },
      value: figure(tone.strength ?? '—', 'loudness'),
      off: inert,
    })}</div>`));
  }
  const bands = tone.bands?.rows ?? [];
  if (bands.length > 0) {
    const list = bands.map((row) => ({
      attrs: attr('data-band-row', row.band),
      label: esc(`Band ${row.band}`),
      value: ['f', 'g', 'q'].map((param) => stepper({
        less: {
          id: `ctl-band-${row.band}-${param}-less`, act: 'band',
          label: `Lower band ${row.band} ${WORDS.param[param]}`, data: { band: row.band, param },
        },
        more: {
          id: `ctl-band-${row.band}-${param}-more`, act: 'band',
          label: `Raise band ${row.band} ${WORDS.param[param]}`, data: { band: row.band, param },
        },
        value: `<output class="fpd-mono" data-cell="band" data-band="${esc(row.band)}"`
          + ` data-param="${param}">${esc(row[param])}</output>`,
        off: inert,
      })).join(' '),
    }));
    out.push(section('ctl-bands', WORDS.bands, rows(list)));
  }
  return out;
}

/** The tone presets: recall, and where the model lets them be saved, renamed and deleted, the three
 * writes that ask in place (`IDS.ask`): the person's own presets carry a rename and a remove each. */
function presetsSection(device, inert) {
  const presets = device.presets;
  if (!presets) return null;
  const quiet = inert || presets.recall === false;
  const editable = presets.editable === true;
  const pills = (presets.rows ?? []).map((row, i) => {
    const one = pill({
      id: `ctl-preset-${i}`, act: 'preset', text: row.name, on: row.on === true, off: quiet,
      data: { name: row.name },
    });
    if (!editable || row.built_in) return one;
    return `<span class="fpx-steps">${one}${button({
      id: `ctl-preset-rename-${i}`, act: 'preset-rename', text: WORDS.rename,
      label: `Rename ${row.name}`, off: inert, data: { name: row.name },
    })}${button({
      id: `ctl-preset-remove-${i}`, act: 'preset-delete', text: WORDS.remove,
      label: `Remove ${row.name}`, off: inert, data: { name: row.name },
    })}</span>`;
  }).join(' ');
  const status = presets.status
    ? `<p class="fp-note">${esc(presets.status.lead ?? '')}${esc(presets.status.name ?? '')}${esc(presets.status.tail ?? '')}</p>` : '';
  const save = editable
    ? `<div class="fp-pills">${button({ id: 'ctl-preset-save', act: 'preset-save', text: WORDS.saveAs, off: inert })}</div>` : '';
  const note = presets.note ? `<p class="fp-note">${esc(presets.note)}</p>` : '';
  return section('ctl-presets', WORDS.presets,
    `<div class="fp-pills">${pills}</div>${status}${save}${note}<div id="${IDS.ask}"></div>`);
}

/** The device's own presets: its slots (recall, or write the bands over one after a question) and
 * the settings it has built in. */
function slotsSection(device, inert) {
  const presets = device.device_presets;
  if (!presets) return null;
  const quiet = inert || presets.recall === false;
  const slots = (presets.slots ?? []).map((slot) => `<span class="fpx-steps">${pill({
    id: `ctl-slot-${slot.slot}`, act: 'slot', text: slot.name, off: quiet, data: { slot: slot.slot },
  })}${presets.save ? button({
    id: `ctl-slot-save-${slot.slot}`, act: 'slot-save', text: WORDS.saveOver,
    label: `Save the bands over ${slot.name}`, off: inert, data: { slot: slot.slot, name: slot.name },
  }) : ''}</span>`).join(' ');
  const built = (presets.builtins ?? []).map((one, i) => pill({
    id: `ctl-setting-${i}`, act: 'setting', text: one.name, on: one.on === true, off: quiet,
    data: { name: one.name },
  })).join('');
  return section('ctl-slots', WORDS.slots, `<div class="fp-pills">${slots}</div>`
    + `${heading(WORDS.settings)}<div class="fp-pills">${built}</div><div id="${IDS.cask}"></div>`);
}

/** The buttons of the computer's player: previous, play or pause, next; only with a track. */
function transportSection(device, inert) {
  const playing = device.playing;
  if (!playing?.buttons || !playing.track) return null;
  const going = playing.track.status === 'playing';
  const one = (name, text) => button({
    id: `ctl-transport-${name}`, act: 'transport', text, off: inert, data: { button: name },
  });
  const headline = playing.track.headline ? `<p class="fp-note">${esc(playing.track.headline)}</p>` : '';
  return section('ctl-transport', WORDS.player,
    `<div class="fp-pills">${one('previous', WORDS.previous)}${going ? one('pause', WORDS.pause)
      : one('play', WORDS.play)}${one('next', WORDS.next)}</div>${headline}`);
}

/** The sections of the device's side, in the order they are drawn. */
export function deviceSections(device, inert) {
  if (!device) return [];
  return [
    powerSection(device, inert), volumeSection(device, inert), controlsSection(device, inert),
    ...toneSections(device, inert), presetsSection(device, inert), slotsSection(device, inert),
    transportSection(device, inert),
  ].filter(Boolean);
}

// ---- the computer's side ----

/** The words of one field of a point of the computer's EQ, as the table writes them. */
export const POINT_WORDS = { f: hzText, g: gainText, q: qText };

function computerVolume(volume, inert) {
  if (!volume) return null;
  const quiet = inert || volume.inert;
  const words = volume.muted ? WORDS.muted : (volume.pct === null ? volume.text : `${volume.text} ${volume.unit}`);
  return section('ctl-c-volume', WORDS.volume, `<div class="fp-pills">${stepper({
    less: { id: 'ctl-c-volume-down', act: 'c-volume-down', label: WORDS.lower, data: { repeat: 1 } },
    more: { id: 'ctl-c-volume-up', act: 'c-volume-up', label: WORDS.raise, data: { repeat: 1 } },
    value: `${button({
      id: 'ctl-c-mute', act: 'c-mute', text: WORDS.mute, pressed: volume.muted === true, off: quiet,
    })}${figure(words, 'c-volume')}`,
    off: quiet,
  })}</div>`);
}

/** One point: a row that wraps (`fi-form`), each field with its word, so nothing scrolls sideways. */
function pointRow(point, inert) {
  const n = point.n;
  const [typeWord, fWord, gWord, qWord] = WORDS.headers;
  const options = POINT_TYPES.map((type) => `<option${type === point.type ? ' selected' : ''}>${type}</option>`).join('');
  const type = `<label>${esc(typeWord)}<select class="fi-input" id="ctl-p${n}t" data-act="c-point" data-n="${n}"`
    + ` data-param="type" aria-label="Point ${n} type"${inert ? ' aria-disabled="true"' : ''}>${options}</select></label>`;
  const field = (param, word) => `<label>${esc(word)}<input class="fi-input" id="ctl-p${n}${param}" type="text"`
    + ` inputmode="decimal" autocomplete="off" data-act="c-point" data-n="${n}" data-param="${param}"`
    + ` value="${esc(POINT_WORDS[param](point[param]))}" aria-label="Point ${n} ${WORDS.param[param]}"`
    + `${inert ? ' readonly aria-disabled="true"' : ''}></label>`;
  const remove = button({
    id: `ctl-px-${n}`, act: 'c-remove', text: '×', label: `Remove point ${n}`, off: inert,
    data: { n },
  });
  return `<div class="fi-form fp-pills" data-row data-n="${n}" role="group" aria-label="Point ${n}">`
    + `<span class="fp-note">${n}</span>${type}${field('f', fWord)}${field('g', gWord)}${field('q', qWord)}${remove}</div>`;
}

function computerPoints(tone, inert) {
  if (!tone) return [];
  const table = `<div data-points>${tone.points.map((point) => pointRow(point, inert)).join('')}</div>`;
  const add = tone.canAdd
    ? `<div class="fp-pills">${button({ id: 'ctl-c-add', act: 'c-add', text: WORDS.addPoint, off: inert })}</div>` : '';
  const note = `<p class="fp-note">${esc(tone.line)}</p>`;
  const out = [
    section('ctl-c-mode', WORDS.mode, `<div class="fp-pills" role="group" aria-label="${WORDS.mode}">${
      tone.modes.map((mode) => pill({
        id: `ctl-c-mode-${mode.name}`, act: 'c-mode', text: mode.label, on: mode.on, off: inert,
        data: { mode: mode.name },
      })).join('')}</div>`),
    section('ctl-c-points', WORDS.points, `${table}${add}${note}`),
  ];
  const presets = tone.presets ?? [];
  if (presets.length > 0) {
    out.push(section('ctl-c-presets', WORDS.presets, `<div class="fp-pills">${presets.map((preset, i) => pill({
      id: `ctl-c-preset-${i}`, act: 'c-preset', text: preset.name, on: tone.preset === preset.name,
      off: inert || preset.playable === false, data: { name: preset.name },
    })).join('')}</div><div id="${IDS.cask}"></div>`));
  }
  return out;
}

function computerBypass(bypass, inert) {
  if (!bypass) return null;
  return section('ctl-c-bypass', null, `<div class="fp-pills">${button({
    id: 'ctl-c-bypass', act: 'c-bypass', text: WORDS.bypass, pressed: bypass.on === true,
    off: inert || !bypass.can,
  })}</div>`);
}

/** The stages and levels of the filter chain, read-only: each stage with its level or `off`, the
 * total and the daemon's words for it. */
function computerLevels(levels) {
  if (!levels) return null;
  const list = levels.stages.map((stage) => ({
    attrs: attr('data-stage', stage.key),
    label: `${dot(stage.on ? 'on' : 'off')}${esc(stage.name)}`,
    value: esc(stage.on ? stage.text : WORDS.off),
  }));
  list.push({
    attrs: attr('data-stage', 'total'), label: esc(WORDS.total), value: esc(levels.totalText),
  });
  const words = levels.words ? `<p class="fp-note">${esc(levels.words)}</p>` : '';
  return section('ctl-c-levels', WORDS.levels, `${rows(list)}${words}`);
}

function soundLink(writer) {
  if (!writer) return null;
  return section('ctl-c-sound', null, `<p class="fp-note">${esc(WORDS.soundNote)}</p>`
    + `<div class="fp-pills">${button({ id: 'ctl-c-sound', act: 'sound', text: WORDS.sound })}</div>`);
}

/** The sections of the computer's side: volume, mode, points, presets, bypass, levels and, for a
 * writer, the link to the Sound popup. A computer that cannot be used says why and draws no more. */
export function computerSections(computer, { inert, writer }) {
  if (!computer) return [];
  if (computer.sw?.unavailable) {
    return [section('ctl-c-unavailable', null, `<p class="fp-note">${esc(computer.sw.unavailable)}</p>`)];
  }
  return [
    computerVolume(computer.volume, inert), ...computerPoints(computer.tone, inert),
    computerBypass(computer.bypass, inert), computerLevels(computer.levels), soundLink(writer),
  ].filter(Boolean);
}

// ---- the page ----

/** The switch between the device's side and the computer's: two pills, the one shown pressed. It
 * only chooses what is looked at (nothing is written). `null` where the page has no computer's
 * side. */
export function sideHtml(computer, side, deviceName) {
  if (!computer?.sw) return '';
  const unavailable = computer.sw.unavailable;
  const one = (value, text) => pill({
    id: `ctl-side-${value}`, act: 'side', text, on: side === value,
    off: value === 'computer' && Boolean(unavailable), data: { side: value },
  });
  return `<div class="fp-pills" role="group" aria-label="${esc(WORDS.switchFor)}">`
    + `${one('dac', deviceName || WORDS.device)}${one('computer', WORDS.computer)}</div>`;
}

/** The line a refused write or a viewer's tap leaves: the daemon's words, in place. */
export function sayHtml(said) {
  if (!said) return '';
  return `<p class="fp-note" role="${said.failed ? 'alert' : 'status'}">${esc(said.text)}</p>`;
}

/** The sections the model draws, on the side it is on. */
export function sectionsOf({ device, computer, side, inert, writer }) {
  if (!device) return [section('ctl-none', null, `<p class="fp-note">${esc(WORDS.noDevice)}</p>`)];
  if (side === 'computer') return computerSections(computer, { inert, writer });
  const sections = deviceSections(device, inert);
  return sections.length > 0 ? sections
    : [section('ctl-empty', null, `<p class="fp-note">${esc(WORDS.noControls)}</p>`)];
}

/** A section as markup, in its place. */
export const sectionHtml = (one) => `<div id="${one.id}">${one.html}</div>`;

/** The popup's page: the switch, the line of words, and the sections. */
export function pageHtml(model, said = null) {
  const sections = sectionsOf(model);
  return `<div id="${IDS.side}">${sideHtml(model.computer, model.side, model.device?.name)}</div>`
    + `<div id="${IDS.say}" aria-live="polite">${sayHtml(said)}</div>`
    + `<div id="${IDS.body}">${sections.map(sectionHtml).join('')}</div>`;
}
