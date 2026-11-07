/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The line of the theme Signal: the hops the sound passes, left to right, from the model. No DOM
// here; draw/line.js draws what this lists.
//
//   hop     { id, kind, name, words, live, changes, url, hopId, volume, trouble, via }
//   via     true on the DAC before a linked amp: choosing it shows that device
//   kind    player | computer | device | declared | standin
//   ids     `player`, `computer`, `device:<url>` (the shown device and a linked, listed hop),
//           `hop:<id>` (a hop the person declared), `standin-dac` and `standin-autoeq`
//
// The line runs the player (when the computer's player is read), the computer (while the computer
// is watched; its facts come with the route), the shown device, then the hops declared after it,
// in file order. An amp a hop links has the DAC that hop names before it (a live hop that shows
// that device when chosen); an amp no hop links stands alone: a ring for the DAC svolder does not
// drive comes before it. The ring the applied AutoEq preset names ends the line when no
// headphones are declared after the device.

import { heldCard } from './draw/card.js';
import { volumeWords } from './slim.js';
import {
  AUTOEQ, CHANGES, COMPUTER, COMPUTER_NAME, COMPUTER_SERVER, CONTROLS_BELOW, STATES, KIND_WORDS, NO_DAC, AS_SENT,
  ROUTE_NOT_READ, UNAVAILABLE, playerName, rateWords,
} from './words.js';

/** What each kind of hop opens below the line: `(page, hop, view) => html`. Later modules put
 * their drawers here; a kind with none opens nothing. */
export const openers = {};

/** What each kind of hop opens below the first screen, the shelf: `(page, hop, view) => html`. */
export const shelves = {};

/** The id of the hop of the device `url`. */
export const shownHopId = (url) => `device:${url}`;

/** The device `url`'s model: the shown one's, or a followed one's. */
export const deviceOf = (page, url) => (url === page.shown ? page.device : page.followed?.[url]) ?? null;

/** The words of the player: the path's reading of its stream, else its rate, then its status. */
function playerHop(page, path) {
  const { track } = page.host;
  const detail = path.find((hop) => hop.label === 'player')?.detail
    || rateWords(page.host.route?.player_rate_hz);
  return {
    id: 'player',
    kind: 'player',
    name: playerName(track.source),
    words: [detail, track.status].filter(Boolean),
    live: true,
    changes: false,
    url: null,
    hopId: null,
  };
}

/** The computer: `PipeWire · <rate>`, then `controls below` while it is the hop chosen (or why its
 * controls cannot be used), and whether it changes the sound. The path's own flag says so; with no
 * path to the device, the route's host EQ does. */
function computerHop(page, path, readsRoute, chosen) {
  const { route } = page.host;
  const unavailable = page.host.computer?.unavailable ?? null;
  const base = {
    id: 'computer', kind: 'computer', name: COMPUTER_NAME, live: true, url: null, hopId: null,
  };
  // What the hop says after its server: the reason the controls cannot be used, else where they are.
  const below = unavailable ? UNAVAILABLE(unavailable) : (chosen ? CONTROLS_BELOW : '');
  if (!route) {
    // A page that holds no route (an amp's) says nothing of it.
    return { ...base, words: [readsRoute ? ROUTE_NOT_READ : '', below].filter(Boolean), changes: false, unavailable: Boolean(unavailable) };
  }
  const fromPath = path.find((hop) => hop.label === 'computer');
  const changes = fromPath ? fromPath.changes === true : route.host_eq === true;
  const sink = route.sink;
  const server = sink ? [COMPUTER_SERVER, rateWords(sink.rate_hz)].filter(Boolean).join(' · ') : COMPUTER.noSink;
  const said = below || (changes ? CHANGES : '');
  return { ...base, words: [server, said].filter(Boolean), changes, unavailable: Boolean(unavailable) };
}

/** Whether a card means the link to the device is down (out of reach, looking, refused) and not a
 * fault on a device that is connected. */
export const unreachable = (held) => Boolean(held) && held.kind !== 'fault';

/** What is wrong at the shown device: `bad` when its card says the link is down, `fault` when a
 * connected device reports a fault, `wait` while the page reads it again or rests, else nothing. */
export function troubleOf(page, state) {
  const held = heldCard(page);
  if (held) return unreachable(held) ? 'bad' : 'fault';
  return state === 'waiting' || state === 'resting' ? 'wait' : null;
}

/** The words the shown device's label says while it is in trouble: the state of its link (when the
 * link is not up), then what the card says. */
function troubleWords(page, state, trouble) {
  const link = page.device?.link;
  const said = link && link.state !== 'on' ? [link.word] : [];
  if (trouble === 'bad') return [...said, heldCard(page).title];
  if (trouble === 'fault') return [heldCard(page).title];
  return state === 'resting' ? [STATES.resting] : [...said, STATES.waitingLine];
}

/** The shown device: the path's reading of its input, and whether the sound is untouched; its
 * volume's words, which the line writes under its name while another hop is chosen. */
function shownHop(page, entry, path, withPath, state) {
  const trouble = troubleOf(page, state);
  const dac = path.find((hop) => hop.label === 'DAC');
  // A DAC with something not read says neither: nothing was checked, so nothing is claimed.
  const claim = dac?.unknown === true ? null : dac?.changes ? CHANGES : AS_SENT;
  const words = trouble ? troubleWords(page, state, trouble)
    : withPath && dac ? [dac.detail, claim] : [entry.family_word];
  return {
    id: shownHopId(entry.url),
    kind: 'device',
    name: entry.name,
    words: words.filter(Boolean),
    live: true,
    changes: dac?.changes === true,
    url: entry.url,
    hopId: null,
    volume: volumeWords(deviceOf(page, entry.url)),
    trouble,
  };
}

/** A declared or linked hop after the shown device. */
function afterHop(row) {
  const linked = row.device && row.live;
  const kind = KIND_WORDS[row.kind] ?? KIND_WORDS.other;
  const words = [row.note ? `${kind} · ${row.note}` : kind];
  return {
    id: linked ? shownHopId(row.device) : `hop:${row.id}`,
    kind: linked ? 'device' : 'declared',
    name: row.name,
    words,
    live: linked === true,
    changes: false,
    url: linked ? row.device : null,
    hopId: row.id,
  };
}

/** The DAC a linked amp is shown after: a live hop that shows that device when chosen. */
function dacBefore(dac) {
  return {
    id: shownHopId(dac.url), kind: 'device', name: dac.name, words: [dac.family_word], live: true,
    changes: false, url: dac.url, hopId: null, via: true,
  };
}

/** A ring that stands for something svolder does not know. */
const standin = (id, name, words) => ({
  id, kind: 'standin', name, words, live: false, changes: false, url: null, hopId: null,
});

/**
 * The hops of the line of the device `shownUrl`, left to right, `state` being the stream's and
 * `chosen` the hop id the person has chosen (the computer's hop says its controls are below while
 * it is). Nothing for a device the fleet does not list.
 */
export function hopsOf(page, shownUrl, state = 'open', chosen = null) {
  const entry = (page.fleet?.devices ?? []).find((device) => device.url === shownUrl);
  if (!entry) return [];
  const rows = page.fleet?.hops ?? [];
  const path = deviceOf(page, shownUrl)?.playing?.path?.hops ?? [];
  const isAmp = entry.family === 'amp';
  const link = isAmp ? rows.find((row) => row.device === shownUrl) : null;
  const dac = link && (page.fleet?.devices ?? []).find((device) => device.url === link.after);
  const alone = isAmp && !dac;
  const after = rows.filter((row) => row.after === shownUrl);
  const hops = [];
  if (page.host?.track) hops.push(playerHop(page, path));
  if (page.host) hops.push(computerHop(page, path, !isAmp, chosen === 'computer'));
  if (alone) hops.push(standin('standin-dac', NO_DAC, []));
  else if (dac) hops.push(dacBefore(dac));
  hops.push(shownHop(page, entry, path, !alone, state));
  hops.push(...after.map(afterHop));
  const route = page.host?.route;
  const headphones = after.some((row) => row.kind === 'headphones');
  if (!isAmp && route?.eq_preset && route.eq_autoeq && !headphones) {
    hops.push(standin('standin-autoeq', route.eq_preset, [AUTOEQ]));
  }
  return hops;
}

/** The side the choice of the hop `id` is: the computer's exactly while its hop is chosen, the
 * device's for every other hop. */
export const sideOf = (id) => (id === 'computer' ? 'computer' : 'dac');

/** The hop to keep chosen: `chosen` while the line still has it, else the computer's when the side
 * kept is the computer's and the line has its hop, usable (a load), else `shown` (the shown device's hop),
 * which is also where a person who has not chosen starts. */
export function keepChoice(chosen, hops, shown, side = 'dac') {
  if (chosen && hops.some((hop) => hop.id === chosen)) return chosen;
  const computer = hops.find((hop) => hop.id === 'computer');
  // a computer that cannot be used is not chosen for a person on a load: the device's side shows, and its hop says why
  return side === 'computer' && computer && !computer.unavailable ? 'computer' : shown;
}

/** The URLs of the devices the line links after the shown one, which the stream follows for their
 * power and meters; the shown device is not among them, and a declared hop has none. */
export function followedUrls(hops, shownUrl) {
  return hops.filter((hop) => hop.kind === 'device' && hop.url && hop.url !== shownUrl).map((hop) => hop.url);
}
