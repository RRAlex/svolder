/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The Devices popup's draws: model in, string out. Pure: no page, no core, no clock but the
// times it is handed. The side list is Overview, then one entry per configured device with its
// dot and, on the device the page shows, the tag Shown. The Overview is every device's name,
// state and key graphs; a device's page is the same block for that one device, with *Show on the
// page* (or, on the shown device, the words that it is the one shown).
//
// A device here is `{ url, name, connected, since, shown, model, series }`: `url` its address
// (the model's URL segment), `since` the unix milliseconds it has not been connected since (or
// none), `shown` whether the page shows it, `model` its device model as the store holds it (or
// none: its controls turn a raw reading into words) and `series` the answer of
// `api/v1/devices/{device}/series` (`{ now_ms, secs, series: [{ id, unit, aggregate, points }]
// }`), `{ error }` when it could not be read, or none while it is on its way.

import { attr, esc } from '../../html.js';
import { sparkline } from '../../plot.js';
import { stepWords } from '../../words.js';
import { dot, tag } from '../frame.js';
import { devicePage } from '../listed.js';
import { FACTS_ID, factsHtml } from './facts.js';
import { readingsHtml } from './readings.js';
import { settingsHtml } from './settings.js';
import { ADD, WALK_WORDS } from './walk.js';

export const NAME = 'devices';

/** The id of the Overview's entry, which is also the first page. */
export const OVERVIEW = 'overview';

/** The side list before the devices are known. */
export const ENTRIES = [{ id: OVERVIEW, label: 'Overview' }];

/** The words of the popup. */
export const WORDS = {
  add: WALK_WORDS.entry,
  shown: 'Shown',
  connected: 'connected',
  away: 'not connected',
  show: 'Show on the page',
  isShown: 'This is the device the page shows.',
  none: 'No device is configured.',
  gone: 'This device is no longer configured.',
  waiting: 'Reading the last 30 minutes…',
  noGraphs: 'This device has no key readings.',
  signal: 'Signal strength',
};

/** The page of the device at `url`, as the router knows it, and the device of a page (`null`
 * for the Overview and the list). */
export const pageOf = devicePage;
export const deviceOf = (page) => (typeof page === 'string' && page.startsWith('d-')
  ? page.slice(2) : null);

/** `url` as an element id is made of it: each character's code in hex, so two devices never
 * share one and no character needs escaping. */
export const domOf = (url) => [...String(url)]
  .map((c) => c.codePointAt(0).toString(16).padStart(2, '0')).join('');

/** The ids the wire finds a device's live places by. */
export const stateId = (url) => `popup-devices-state-${domOf(url)}`;
export const graphsId = (url) => `popup-devices-graphs-${domOf(url)}`;
export const blockId = (url) => `popup-devices-device-${domOf(url)}`;
/** The place of *Show on the page* on a device's page, and the button itself: a control in a
 * place the wire puts has an id, so the focus finds it again. */
export const SHOW_ID = 'popup-devices-show';
export const SHOW_BUTTON_ID = 'popup-devices-show-button';
/** What the page pane holds: the wire puts it when the devices in it change, and the pane
 * itself (its scroll, a list's panel) stays. */
export const BODY_ID = 'popup-devices-body';
/** The line the Overview opens with after a device was removed: it takes the focus, so it is
 * read. */
export const NOTICE_ID = 'popup-devices-notice';
/** The place the Overview opens with, above the removal line: the daemon's words that
 * `config.toml` changed since its last reload (empty while it did not). The place always stands
 * on the Overview, so the status region exists before its words come, and the wire puts the
 * words alone, which never moves the focus. */
export const CONFIG_ID = 'popup-devices-config';

/** What the config place holds: the daemon's words as they are, or nothing. */
export function configHtml(words) {
  return typeof words === 'string' && words ? `<p class="fp-note fp-config">${esc(words)}</p>` : '';
}

/** `ms` (unix milliseconds) as the clock's `HH:MM`, in the viewer's own time zone. */
export function clockWords(ms) {
  const at = new Date(ms);
  const two = (value) => String(value).padStart(2, '0');
  return `${two(at.getHours())}:${two(at.getMinutes())}`;
}

/** A device's state in words: connected, or not connected, since when if the daemon knows. */
export function stateWords(device, words = WORDS) {
  if (device.connected) return words.connected;
  return Number.isFinite(device.since) ? `${words.away} since ${clockWords(device.since)}`
    : words.away;
}

/** The side list: Overview, a break, then each device with its dot (full: connected, a ring:
 * not; a screen reader hears the words) and the tag on the one the page shows, and last, after a
 * break, *+ Add a device* (not for a reader: it writes). */
export function entriesOf(devices, words = WORDS, { readOnly = false } = {}) {
  return [
    ...ENTRIES,
    ...devices.map((device, i) => ({
      id: pageOf(device.url),
      label: device.name,
      dot: device.connected ? 'on' : 'off',
      dotWords: device.connected ? words.connected : words.away,
      ...(device.shown ? { tag: words.shown } : {}),
      ...(i === 0 ? { group: true } : {}),
    })),
    ...(readOnly ? [] : [{ id: ADD, label: words.add, group: true }]),
  ];
}

/** What is drawn inside an entry's button: the dot, the label, the tag. The frame draws the
 * same; the wire puts this into a standing entry when only a dot or the tag changed. */
export function entryInner(entry) {
  return `${dot(entry.dot, entry.dotWords ?? null)}${esc(entry.label)}`
    + `${entry.tag === undefined ? '' : tag(entry.tag)}`;
}

/** The state line of a device: its dot (an ornament here, the words say it) and the words. */
export function stateHtml(device, words = WORDS) {
  return `${dot(device.connected ? 'on' : 'off')} ${esc(stateWords(device, words))}`;
}

/** The control of `model` (a device model) that the series `id` is the reading of, with what
 * kind of reading it is: the volume or a slider (`level`: its steps have words), a switch, a
 * choice, a readout or a meter; `null` for the signal and for a key the model does not hold. */
export function controlOf(model, id) {
  if (!model) return null;
  const { volume } = model;
  if (volume?.key === id) return { kind: 'level', label: volume.label, on: volume };
  for (const control of model.controls ?? []) {
    if (control.key !== id) continue;
    const kind = control.kind === 'slider' ? 'level' : control.kind;
    return { kind, label: control.label, on: control };
  }
  if (model.power?.key === id) return { kind: 'toggle', label: 'Power', on: model.power };
  for (const control of [...(model.power?.toggles ?? []), ...(model.power?.readouts ?? [])]) {
    if (control.key === id) return { kind: control.kind, label: control.label, on: control };
  }
  const meter = (model.meters?.meters ?? []).find((one) => one.key === id);
  return meter ? { kind: 'meter', label: meter.channel, on: meter } : null;
}

/** A reading `value` of the series `one` in words: the signal with its unit, a level through
 * the model's own words for its steps (as the volume and the sliders are worded everywhere), a
 * switch on or off, anything else its number. With `current` the value is the device's reading
 * now, which the model words itself for a readout (its text) and a choice (the option that is
 * on); the model has no words for an earlier value of those. */
export function readingWords(one, value, model, { current = false } = {}) {
  if (!Number.isFinite(value)) return '–';
  const control = controlOf(model, one.id);
  if (control?.kind === 'level') return stepWords(control.on, Math.round(value));
  if (control?.kind === 'toggle') return value >= 0.5 ? 'on' : 'off';
  if (current && control?.kind === 'readout' && control.on.text) return control.on.text;
  const chosen = current && control?.kind === 'choice'
    ? (control.on.options ?? []).find((option) => option.on)?.label : null;
  if (chosen) return chosen;
  const number = String(Number(value.toFixed(1)));
  return one.unit ? `${number} ${one.unit}` : number;
}

/** What the series `one` is called: the signal, else its control's label, else its id. */
export function seriesName(one, model, words = WORDS) {
  if (one.id === 'signal') return words.signal;
  return controlOf(model, one.id)?.label || one.id;
}

/**
 * The order the key graphs stand in, the one place that says it: the signal strength first,
 * then the device's key readings exactly as the daemon lists them, which is the order of the
 * device's profile. The profile decides; nothing is sorted by kind here.
 */
export function ordered(series) {
  const list = [...(series ?? [])];
  const signal = (one) => one.id === 'signal';
  return [...list.filter(signal), ...list.filter((one) => !signal(one))];
}

/** The points of `one` inside the span that ends at `now` (`{ t, v }`, oldest first). A
 * control's reading holds until the next one, so its line is drawn in steps, from the span's
 * start (its first point is the reading that held then) and, while the device is `connected`,
 * on to now; the signal is only what was heard, when it was heard. */
export function pointsOf(one, { now, from, connected }) {
  const raw = (one.points ?? []).filter((p) => Number.isFinite(p?.[0]) && Number.isFinite(p?.[1])
    && p[0] <= now);
  if (one.id === 'signal') return raw.filter((p) => p[0] >= from).map(([t, v]) => ({ t, v }));
  const out = [];
  for (const [t, v] of raw) {
    const at = Math.max(t, from);
    if (out.length > 0) {
      if (at === out.at(-1).t) out.pop();
      else out.push({ t: at, v: out.at(-1).v });
    }
    out.push({ t: at, v });
  }
  if (connected && out.length > 0 && out.at(-1).t < now) out.push({ t: now, v: out.at(-1).v });
  return out;
}

/** How many readings of `one` are its own: the points it was sent with. */
const count = (one) => (one.points ?? []).filter((p) => Number.isFinite(p?.[1])).length;

/**
 * One key graph of `device`: its name and its last reading above the plot (core/plot.js, in the
 * frame's graph box), which a screen reader hears as one image with the name, the last reading
 * and the range. A series with one reading or none says so in words and draws no box.
 */
export function graphHtml(one, device, { now, secs }, words = WORDS) {
  const name = seriesName(one, device.model, words);
  const span = `${Math.round(secs / 60)} minutes`;
  const head = (last) => '<figcaption class="fpd-graph-head">'
    + `<span class="fpd-graph-name">${esc(name)}</span>`
    + `${last === null ? '' : `<span class="fpd-graph-last">${esc(last)}</span>`}</figcaption>`;
  const figure = (inside) => `<figure class="fpd-graph"${attr('data-series', one.id)}>`
    + `${inside}</figure>`;
  const own = count(one);
  if (own === 0) {
    return figure(`${head(null)}<p class="fp-note">No reading in the last ${esc(span)}.</p>`);
  }
  const read = one.points.filter((p) => Number.isFinite(p?.[1]));
  const values = read.map((p) => p[1]);
  const say = (value) => readingWords(one, value, device.model);
  // the last reading of a connected device is its reading now
  const last = readingWords(one, values.at(-1), device.model, { current: device.connected });
  if (own === 1) {
    const line = one.id === 'signal'
      ? `One reading in the last ${span}, at ${clockWords(read[0][0])}.`
      : `No change in the last ${span}.`;
    return figure(`${head(last)}<p class="fp-note">${esc(line)}</p>`);
  }
  const from = now - secs * 1000;
  const low = say(Math.min(...values));
  const high = say(Math.max(...values));
  const label = `${name}, last ${span}: ${device.connected ? 'now' : 'last'} ${last}, `
    + `between ${low} and ${high}`;
  const plot = sparkline({
    points: pointsOf(one, { now, from, connected: device.connected }),
    width: 300, height: 48, pad: 5, from, to: now, cls: 'fpd-spark',
  });
  return figure(`${head(last)}<div class="fp-graph fpd-plot" role="img"`
    + `${attr('aria-label', label)}>${plot}</div>`
    + `<p class="fpd-graph-range">${esc(low)} to ${esc(high)}</p>`);
}

/** The key graphs of `device`, in order; while its series is on its way or could not be read,
 * the words for that. */
export function graphsHtml(device, words = WORDS) {
  const answer = device.series;
  if (!answer) return `<p class="fp-note">${esc(words.waiting)}</p>`;
  if (answer.error) return `<p class="fp-note fp-failed">${esc(answer.error)}</p>`;
  const list = ordered(answer.series);
  if (list.length === 0) return `<p class="fp-note">${esc(words.noGraphs)}</p>`;
  const span = { now: answer.now_ms, secs: answer.secs ?? 1800 };
  return list.map((one) => graphHtml(one, device, span, words)).join('');
}

/** A device's block: its name (a heading at `level`), its state line, `settings` (markup, on a
 * device's page), its key graphs and `after` (markup: on a device's page, its facts and
 * readings). `head` (markup) stands at the right end of the name's line, wrapping under the name
 * when it does not fit: *Show on the page* on a device's page. One that is not connected is marked
 * `fpd-away`, which the sheet dims by a token. */
export function deviceHtml(device, {
  level = 3, settings = '', after = '', head = '',
} = {}, words = WORDS) {
  return `<section class="fpd-device${device.connected ? '' : ' fpd-away'}"`
    + `${attr('id', blockId(device.url))}${attr('data-device', device.url)}>`
    + `<div class="fpd-head"><h${level} class="fpd-name">${esc(device.name)}</h${level}>${head}</div>`
    + `<p class="fpd-state"${attr('id', stateId(device.url))}>${stateHtml(device, words)}</p>`
    + `${settings}<div class="fpd-graphs"${attr('id', graphsId(device.url))}>`
    + `${graphsHtml(device, words)}</div>${after}`
    + '</section>';
}

/** What stands in the place of *Show on the page*: the button, or on the shown device the
 * words that it is the one shown. */
export function showHtml(device, words = WORDS) {
  if (device.shown) return `<p class="fp-note">${esc(words.isShown)}</p>`;
  return `<button type="button" class="fi-btn"${attr('id', SHOW_BUTTON_ID)}`
    + `${attr('data-show', device.url)}>${esc(words.show)}</button>`;
}

/**
 * The page pane at `page`, as one place (`BODY_ID`). It holds the Overview (at the list too,
 * whose pane a wide window fills with it): every device's block in the list's order; or a
 * device's page: that device's block, with *Show on the page* on its name's line. `devices` are the configured
 * ones; `error` is why they could not be read; `profiles` are the answer's, and `settings` the
 * states of the settings by each device's `unit` (settings.js says what a state holds); `notice`
 * is the line the Overview opens with (a device was removed), none otherwise; `configChanged` is
 * the daemon's words that its file changed since the last reload, none otherwise; `readings` is the
 * state of the All readings disclosure (`{ open }`, readings.js); `walk` is the markup of the
 * *+ Add a device* page (walk.js), which the page `ADD` holds in place of any device; `readOnly`
 * is a reader's page: no settings, no config line, no add page, no *Identify*.
 */
export function pageHtml(model, words = WORDS) {
  return `<div${attr('id', BODY_ID)}>${bodyHtml(model, words)}</div>`;
}

/** What `pageHtml` holds in its one place (`BODY_ID`). */
export function bodyHtml({
  page, devices, error = null, profiles = [], settings = {}, notice = null, readings = null,
  walk = '', configChanged = null, readOnly = false,
}, words = WORDS) {
  const failed = error ? `<p class="fp-note fp-failed" role="alert">${esc(error)}</p>` : '';
  if (page === ADD && !readOnly) return `${failed}${walk}`;
  const url = deviceOf(page);
  if (url !== null) {
    const device = devices.find((one) => one.url === url);
    if (!device) return `${failed}<p class="fp-note">${esc(words.gone)}</p>`;
    const own = readOnly ? ''
      : settingsHtml(device, profiles, settings[device.unit ?? device.url] ?? {});
    const after = `<div${attr('id', FACTS_ID)}>${factsHtml(device, undefined, { readOnly })}</div>`
      + readingsHtml(device, readings);
    const head = `<div class="fpd-show"${attr('id', SHOW_ID)}>${showHtml(device, words)}</div>`;
    return `${failed}${deviceHtml(device, {
      level: 3, settings: own, after, head,
    }, words)}`;
  }
  const config = `<div${attr('id', CONFIG_ID)} role="status">${configHtml(configChanged)}</div>`;
  const said = (readOnly ? '' : config)
    + (notice ? `<p class="fp-note"${attr('id', NOTICE_ID)} tabindex="-1" role="status">`
      + `${esc(notice)}</p>` : '');
  if (devices.length === 0) return `${failed}${said}<p class="fp-note">${esc(words.none)}</p>`;
  return failed + said + devices.map((device) => deviceHtml(device, { level: 3 }, words)).join('');
}
