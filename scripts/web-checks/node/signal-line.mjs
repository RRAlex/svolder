/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for the theme Signal's line (themes/signal/line.js): which hops the line has, in
// what order, which are live and which are rings, where the computer's stretch changes the sound,
// the stand-ins (an amp no hop links, the AutoEq headphones) and the choice that falls back to the
// shown device when its hop goes. Run: node signal-line.mjs
import assert from 'node:assert/strict';
import { followedUrls, hopsOf, keepChoice, shownHopId } from '../../../bins/svolderd/web/themes/signal/line.js';

let n = 0;
const ok = (name, fn) => { fn(); n++; console.log('ok', name); };

const DAC = { name: 'Bifrost 3', url: 'Bifrost%203', family: 'dac', family_word: 'DAC' };
const OTHER = { name: 'Gungnir 2', url: 'Gungnir%202', family: 'dac', family_word: 'DAC' };
const AMP = { name: 'Asgard 3', url: 'Asgard%203', family: 'amp', family_word: 'Amp' };

const route = (more = {}) => ({
  sink: { words: 'Bifrost 3', rate_hz: 96000, format: 'S32LE', volume_pct: 100, muted: false },
  player_rate_hz: 96000,
  host_eq: false,
  eq_preset: null,
  eq_autoeq: false,
  others_mixing: 0,
  ...more,
});
const host = (more = {}) => ({
  now_ms: 1,
  sink_label: false,
  buttons: true,
  track: {
    headline: 'Teardrop', artist: 'Massive Attack', album: '', source: 'Demo Tape · Strawberry',
    status: 'playing', cover: null, length_us: null, progress: null,
  },
  path: null,
  route: route(),
  ...more,
});
const hop = (id, kind, name, after, more = {}) => ({
  id, kind, name, note: null, after, device: null, live: false, ...more,
});
const pathOf = (computer = false, dac = false, unknown = false) => ({
  class: 'ok',
  words: '',
  hops: [
    { label: 'player', detail: '96 kHz S16LE', changes: false },
    { label: 'computer', detail: '96 kHz S32LE', changes: computer },
    { label: 'DAC', detail: 'USB 1, 96 kHz', changes: dac, unknown },
  ],
});
/** A page showing `shown` (a device of `devices`). */
function page({
  devices = [DAC], hops = [], shown = DAC.url, withHost = host(), path = null,
} = {}) {
  const device = { url: shown, playing: path ? { path } : { path: null } };
  return {
    shown,
    device,
    followed: {},
    fleet: { devices, hops },
    host: withHost,
  };
}
const ids = (list) => list.map((h) => h.id);

ok('the_line_runs_player_computer_dac_then_its_hops_in_file_order', () => {
  const p = page({
    devices: [DAC, AMP],
    hops: [
      hop('Asgard 3', 'amp', 'Asgard 3', DAC.url, { device: AMP.url, live: true }),
      hop('HD 650', 'headphones', 'HD 650', DAC.url, { note: 'on the desk' }),
    ],
  });
  const list = hopsOf(p, DAC.url);
  assert.deepEqual(ids(list), ['player', 'computer', shownHopId(DAC.url), `device:${AMP.url}`,
    'hop:HD 650']);
  assert.deepEqual(list.map((h) => h.kind), ['player', 'computer', 'device', 'device', 'declared']);
  assert.equal(list[2].url, DAC.url);
  assert.equal(list[3].hopId, 'Asgard 3');
  assert.equal(list[4].hopId, 'HD 650');
  assert.equal(list[0].name, 'Strawberry');
  assert.equal(list[2].name, 'Bifrost 3');
});

ok('a_hop_after_another_dac_is_not_on_this_line', () => {
  const p = page({
    devices: [DAC, OTHER],
    hops: [hop('HD 650', 'headphones', 'HD 650', OTHER.url), hop('Grado', 'headphones', 'Grado', DAC.url)],
  });
  assert.deepEqual(ids(hopsOf(p, DAC.url)).slice(3), ['hop:Grado']);
  assert.deepEqual(ids(hopsOf(p, DAC.url)).includes('hop:HD 650'), false);
});

ok('a_linked_hop_is_live_only_when_listed', () => {
  const linked = hop('Asgard 3', 'amp', 'Asgard 3', DAC.url, { device: AMP.url, live: true });
  const gone = hop('Asgard 3', 'other', 'Asgard 3', DAC.url, { device: AMP.url, live: false });
  const live = hopsOf(page({ devices: [DAC, AMP], hops: [linked] }), DAC.url).at(-1);
  assert.equal(live.live, true);
  assert.equal(live.kind, 'device');
  const unlisted = hopsOf(page({ devices: [DAC], hops: [gone] }), DAC.url).at(-1);
  assert.equal(unlisted.live, false);
  assert.equal(unlisted.kind, 'declared');
  const declared = hopsOf(page({ hops: [hop('HD 650', 'headphones', 'HD 650', DAC.url)] }), DAC.url).at(-1);
  assert.equal(declared.live, false);
  assert.equal(declared.kind, 'declared');
});

ok('no_host_means_the_line_starts_at_the_dac', () => {
  const list = hopsOf(page({ withHost: null, hops: [hop('HD 650', 'headphones', 'HD 650', DAC.url)] }),
    DAC.url);
  assert.deepEqual(ids(list), [shownHopId(DAC.url), 'hop:HD 650']);
  const noRoute = hopsOf(page({ withHost: host({ route: null }) }), DAC.url);
  assert.deepEqual(ids(noRoute), ['player', 'computer', shownHopId(DAC.url)]);
  const noTrack = hopsOf(page({ withHost: host({ track: null }) }), DAC.url);
  assert.deepEqual(ids(noTrack), ['computer', shownHopId(DAC.url)]);
  assert.deepEqual(ids(hopsOf(page({ withHost: host({ track: null, route: null }) }), DAC.url)),
    ['computer', shownHopId(DAC.url)]);
});

ok('the_computer_hop_stays_while_the_route_is_not_read', () => {
  const list = hopsOf(page({ withHost: host({ route: null }) }), DAC.url);
  const computer = list.find((h) => h.kind === 'computer');
  assert.deepEqual(computer.words, ['route not read']);
  assert.equal(computer.changes, false);
  assert.equal(computer.live, true);
});

ok('an_amp_no_hop_links_stands_alone_after_a_stand_in_ring', () => {
  const alone = hopsOf(page({ devices: [DAC, AMP], shown: AMP.url }), AMP.url);
  assert.deepEqual(ids(alone), ['player', 'computer', 'standin-dac', shownHopId(AMP.url)]);
  const ring = alone[2];
  assert.equal(ring.kind, 'standin');
  assert.equal(ring.name, 'a DAC svolder does not drive');
  assert.equal(ring.live, false);
  const linked = hop('Asgard 3', 'amp', 'Asgard 3', DAC.url, { device: AMP.url, live: true });
  // an amp no hop links, and an amp whose DAC the fleet does not list, stand alone and the computer says nothing of a route
  const noRoute = host({ route: null });
  assert.deepEqual(hopsOf(page({ devices: [DAC, AMP], shown: AMP.url, withHost: noRoute }), AMP.url)[1].words, []);
  const unlisted = hopsOf(page({ devices: [AMP], shown: AMP.url, hops: [linked] }), AMP.url);
  assert.deepEqual(ids(unlisted), ['player', 'computer', 'standin-dac', shownHopId(AMP.url)]);
});

ok('a_linked_amp_shown_alone_has_the_dac_its_row_names_before_it', () => {
  const linked = hop('Asgard 3', 'amp', 'Asgard 3', DAC.url, { device: AMP.url, live: true });
  const list = hopsOf(page({
    devices: [DAC, OTHER, AMP], shown: AMP.url, hops: [linked], withHost: host({ route: null }),
  }), AMP.url);
  assert.deepEqual(ids(list), ['player', 'computer', shownHopId(DAC.url), shownHopId(AMP.url)]);
  const dac = list[2];
  assert.equal(dac.kind, 'device');
  assert.equal(dac.live, true);
  assert.equal(dac.url, DAC.url);
  assert.equal(dac.name, 'Bifrost 3');
  assert.equal(dac.via, true);
  assert.equal(list[3].via, undefined);
  // the amp's page holds no route: the computer is drawn without facts or words
  assert.deepEqual(list[1].words, []);
  assert.equal(list[1].changes, false);
  // the stream follows the DAC (its volume and power), never the amp it shows
  assert.deepEqual(followedUrls(list, AMP.url), [DAC.url]);
  // a DAC is not before anything: no `via` on its own page
  const own = hopsOf(page({ devices: [DAC, AMP], hops: [linked] }), DAC.url);
  assert.equal(own.some((h) => h.via), false);
});

ok('the_host_eq_marks_the_computers_stretch_as_changing', () => {
  const find = (p) => hopsOf(p, DAC.url).find((h) => h.kind === 'computer');
  assert.equal(find(page({ path: pathOf(true) })).changes, true);
  assert.equal(find(page({ path: pathOf(false) })).changes, false);
  // the path's own flag wins over the route's host EQ
  assert.equal(find(page({ path: pathOf(false), withHost: host({ route: route({ host_eq: true }) }) })).changes,
    false);
  // with no path (an amp shown, a route without a DAC path) the route's host EQ says
  assert.equal(find(page({ withHost: host({ route: route({ host_eq: true }) }) })).changes, true);
  assert.equal(find(page({ withHost: host({ route: route({ host_eq: false }) }) })).changes, false);
  const shownDac = hopsOf(page({ path: pathOf(true, true) }), DAC.url).find((h) => h.id === shownHopId(DAC.url));
  assert.equal(shownDac.changes, true);
});

ok('the_autoeq_headphones_ring_ends_the_line_when_no_headphones_are_declared', () => {
  const eq = host({ route: route({ host_eq: true, eq_preset: 'Sennheiser HD 650', eq_autoeq: true }) });
  const list = hopsOf(page({ withHost: eq }), DAC.url);
  const ring = list.at(-1);
  assert.equal(ring.id, 'standin-autoeq');
  assert.equal(ring.kind, 'standin');
  assert.equal(ring.name, 'Sennheiser HD 650');
  assert.deepEqual(ring.words, ['from your AutoEq preset']);
  assert.equal(ring.live, false);
  // after a declared amp too
  const withAmp = hopsOf(page({ withHost: eq, hops: [hop('Asgard 3', 'amp', 'Asgard 3', DAC.url)] }),
    DAC.url);
  assert.deepEqual(ids(withAmp).slice(-2), ['hop:Asgard 3', 'standin-autoeq']);
});

ok('a_band_preset_draws_no_headphones_ring', () => {
  const band = host({ route: route({ host_eq: true, eq_preset: 'Warm', eq_autoeq: false }) });
  assert.equal(hopsOf(page({ withHost: band }), DAC.url).some((h) => h.id === 'standin-autoeq'), false);
  const off = host({ route: route({ host_eq: false, eq_preset: null, eq_autoeq: false }) });
  assert.equal(hopsOf(page({ withHost: off }), DAC.url).some((h) => h.id === 'standin-autoeq'), false);
});

ok('a_declared_headphones_hop_replaces_the_autoeq_ring', () => {
  const eq = host({ route: route({ host_eq: true, eq_preset: 'Sennheiser HD 650', eq_autoeq: true }) });
  const list = hopsOf(page({ withHost: eq, hops: [hop('HD 650', 'headphones', 'HD 650', DAC.url)] }), DAC.url);
  assert.equal(list.some((h) => h.id === 'standin-autoeq'), false);
  assert.equal(list.at(-1).id, 'hop:HD 650');
  // a headphones hop after another DAC does not replace it
  const other = hopsOf(page({
    devices: [DAC, OTHER], withHost: eq, hops: [hop('HD 650', 'headphones', 'HD 650', OTHER.url)],
  }), DAC.url);
  assert.equal(other.at(-1).id, 'standin-autoeq');
});

ok('a_choice_that_disappears_goes_back_to_the_shown_device', () => {
  const list = hopsOf(page({ hops: [hop('HD 650', 'headphones', 'HD 650', DAC.url)] }), DAC.url);
  const shown = shownHopId(DAC.url);
  assert.equal(keepChoice('hop:HD 650', list, shown), 'hop:HD 650');
  assert.equal(keepChoice('hop:Gone', list, shown), shown);
  assert.equal(keepChoice(null, list, shown), shown);
  assert.equal(keepChoice('hop:HD 650', [], shown), shown);
});

ok('a_chosen_amp_that_leaves_the_fleet_gives_the_box_back_to_the_dac', () => {
  const linked = hop('Asgard 3', 'amp', 'Asgard 3', DAC.url, { device: AMP.url, live: true });
  const shown = shownHopId(DAC.url);
  const chosen = shownHopId(AMP.url);
  const on = hopsOf(page({ devices: [DAC, AMP], hops: [linked] }), DAC.url);
  assert.equal(keepChoice(chosen, on, shown), chosen);
  assert.deepEqual(followedUrls(on, DAC.url), [AMP.url]);
  // unplugged and unlisted: the row stays, no longer live, and is a ring
  const unlisted = hopsOf(page({ devices: [DAC], hops: [{ ...linked, live: false }] }), DAC.url);
  assert.equal(keepChoice(chosen, unlisted, shown), shown);
  assert.deepEqual(followedUrls(unlisted, DAC.url), []);
  // unlinked (the row removed) or the DAC alone
  assert.equal(keepChoice(chosen, hopsOf(page({ devices: [DAC, AMP] }), DAC.url), shown), shown);
  // the list of followed devices: never the shown one, never a declared hop or a ring
  const mixed = hopsOf(page({ devices: [DAC, AMP], hops: [linked, hop('HD 650', 'headphones', 'HD 650', DAC.url)] }), DAC.url);
  assert.deepEqual(followedUrls(mixed, DAC.url), [AMP.url]);
  assert.deepEqual(followedUrls(mixed, AMP.url), [DAC.url]);
});

ok('a_chosen_hop_removed_elsewhere_gives_the_box_back', () => {
  const shown = shownHopId(DAC.url);
  const cans = hop('HD 650', 'headphones', 'HD 650', DAC.url);
  const on = hopsOf(page({ hops: [cans] }), DAC.url);
  assert.equal(keepChoice('hop:HD 650', on, shown), 'hop:HD 650');
  // renamed, moved to another line or removed (the CLI, another page): the box goes to the shown DAC
  for (const rows of [[], [{ ...cans, id: 'HD 660', name: 'HD 660' }], [{ ...cans, after: OTHER.url }]]) {
    assert.equal(keepChoice('hop:HD 650', hopsOf(page({ devices: [DAC, OTHER], hops: rows }), DAC.url), shown), shown);
  }
});

ok('the_player_quitting_while_chosen_gives_the_box_back_to_the_dac', () => {
  const shown = shownHopId(DAC.url);
  assert.equal(keepChoice('player', hopsOf(page(), DAC.url), shown), 'player');
  assert.equal(keepChoice('player', hopsOf(page({ withHost: host({ track: null }) }), DAC.url), shown), shown);
  assert.equal(keepChoice('player', hopsOf(page({ withHost: null }), DAC.url), shown), shown);
  assert.equal(keepChoice('computer', hopsOf(page({ withHost: null }), DAC.url), shown), shown);
});

ok('a_device_card_or_a_waiting_page_marks_the_shown_hop_in_trouble', () => {
  const p = page();
  const mine = (list) => list.find((h) => h.id === shownHopId(DAC.url));
  assert.equal(mine(hopsOf(p, DAC.url)).trouble, null);
  p.device.card = { title: 'Bifrost 3 is out of reach', kind: 'warn' };
  assert.equal(mine(hopsOf(p, DAC.url)).trouble, 'bad');
  const q = page();
  assert.equal(mine(hopsOf(q, DAC.url, 'waiting')).trouble, 'wait');
  assert.equal(mine(hopsOf(q, DAC.url, 'resting')).trouble, 'wait');
  assert.equal(mine(hopsOf(q, DAC.url, 'lost')).trouble, null);
});

ok('the_dac_says_neither_while_its_rate_is_not_read', () => {
  const words = (path) => hopsOf(page({ path }), DAC.url).find((h) => h.kind === 'device').words;
  assert.deepEqual(words(pathOf(false, false, true)), ['USB 1, 96 kHz']);
  assert.deepEqual(words(pathOf(false, false, false)), ['USB 1, 96 kHz', 'arrives as sent']);
  assert.deepEqual(words(pathOf(false, true, false)), ['USB 1, 96 kHz', 'changes the sound']);
});

console.log(`signal-line: ${n} passed`);
