/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for core/path-words.js: what the path names about the sound, as structure. The
// stages of the computer's filter chain the path names, whether the computer changes the sound (the
// `computer` hop's flag, else the route's host EQ), the DAC's own stage, and whether the path changes
// the sound anywhere. The words are the daemon's: the cases hold the five stage words and the five
// DAC words as the Path judge writes them (crates/svolder-ipc/src/host/path.rs) and that nothing else
// is picked out. Night drive's top line reads the same functions: node/nightdrive-hud.mjs is the proof
// its words did not change.
// Run: node core-path-words.mjs
import assert from 'node:assert/strict';
import {
  DAC_WORDS, STAGE_WORDS, changedOf, computerOf, dacOf,
} from '../../../bins/svolderd/web/core/path-words.js';

let n = 0;
const ok = (name, fn) => { fn(); n++; console.log('ok', name); };

/** A page whose shown device has this path, over a route with or without the host EQ. */
const pageOf = ({ words = [], hops = [], hostEq } = {}) => ({
  host: hostEq === undefined ? null : { route: { host_eq: hostEq } },
  device: { playing: { path: { words, hops } } },
});
const hop = (label, changes, detail = '') => ({ label, changes, detail });

ok('the_stage_and_dac_words_are_the_judges', () => {
  assert.deepEqual(STAGE_WORDS,
    ['host EQ on', 'loudness on', 'crossfeed on', 'virtual speakers on', 'EQ pass-through']);
  assert.deepEqual(DAC_WORDS,
    ['tone control', 'balance off centre', 'muted', 'phase inverted', 'NOS filter']);
});

ok('the_computer_names_the_stages_of_its_chain_in_the_paths_order_and_no_other_word', () => {
  const page = pageOf({
    hostEq: true,
    words: ['untouched', 'loudness on', 'host EQ on', 'player is 48 kHz', 'crossfeed on'],
    hops: [hop('computer', true)],
  });
  assert.deepEqual(computerOf(page), {
    eq: true, changes: true, stages: ['loudness on', 'host EQ on', 'crossfeed on'],
  });
  assert.deepEqual(computerOf(pageOf({ hostEq: true, words: ['EQ pass-through'], hops: [hop('computer', false)] })),
    { eq: true, changes: false, stages: ['EQ pass-through'] });
  // the route has the host EQ and the path names no stage: the theme says its fixed words
  assert.deepEqual(computerOf(pageOf({ hostEq: true, words: ['untouched'], hops: [hop('computer', false)] })),
    { eq: true, changes: false, stages: [] });
});

ok('without_the_host_eq_no_stage_is_named_whatever_the_words_say', () => {
  const page = pageOf({ hostEq: false, words: ['host EQ on'], hops: [hop('computer', true)] });
  assert.deepEqual(computerOf(page), { eq: false, changes: true, stages: [] });
  assert.deepEqual(computerOf(pageOf({ words: ['host EQ on'] })), { eq: false, changes: false, stages: [] });
});

ok('the_computers_flag_is_its_hops_and_the_routes_host_eq_when_the_path_has_no_such_hop', () => {
  // the hop's own flag wins, either way
  assert.equal(computerOf(pageOf({ hostEq: true, hops: [hop('computer', false)] })).changes, false);
  assert.equal(computerOf(pageOf({ hostEq: false, hops: [hop('computer', true)] })).changes, true);
  // a flag that is not `true` is no change
  assert.equal(computerOf(pageOf({ hops: [{ label: 'computer', changes: 'yes' }] })).changes, false);
  // no computer hop: the route's host EQ decides
  assert.equal(computerOf(pageOf({ hostEq: true, hops: [hop('player', false)] })).changes, true);
  assert.equal(computerOf(pageOf({ hostEq: false, hops: [hop('player', false)] })).changes, false);
  assert.equal(computerOf(pageOf({})).changes, false);
});

ok('the_dac_names_its_own_stage_a_volume_and_the_five_words_and_nothing_else', () => {
  const dac = hop('DAC', false, 'USB 1, volume -3.3 dB, tone control, 48 kHz, NOS filter, balance off centre');
  assert.deepEqual(dacOf(pageOf({ hops: [dac] })),
    ['volume -3.3 dB', 'tone control', 'NOS filter', 'balance off centre']);
  assert.deepEqual(dacOf(pageOf({ hops: [hop('DAC', false, 'muted, phase inverted')] })), ['muted', 'phase inverted']);
  assert.deepEqual(dacOf(pageOf({ hops: [hop('DAC', false, 'USB 1, 48 kHz')] })), []);
  assert.deepEqual(dacOf(pageOf({ hops: [hop('computer', true, 'volume 100 %')] })), [], 'only the DAC hop');
  assert.deepEqual(dacOf(pageOf({ hops: [hop('DAC', false)] })), []);
  assert.deepEqual(dacOf(pageOf({})), []);
});

ok('the_path_changes_the_sound_when_the_computer_does_or_the_dac_has_a_stage_of_its_own', () => {
  assert.equal(changedOf(pageOf({ hostEq: true, hops: [hop('computer', true)] })), true);
  assert.equal(changedOf(pageOf({ hops: [hop('computer', false), hop('DAC', false, 'volume -3.3 dB')] })), true);
  assert.equal(changedOf(pageOf({ hops: [hop('computer', false), hop('DAC', false, 'USB 1, 48 kHz')] })), false);
  assert.equal(changedOf(pageOf({})), false);
});

ok('a_page_with_nothing_in_it_is_no_change_and_throws_nothing', () => {
  for (const page of [undefined, null, {}, { device: null }, { device: { playing: null } }, { host: {} }]) {
    assert.deepEqual(computerOf(page), { eq: false, changes: false, stages: [] });
    assert.deepEqual(dacOf(page), []);
    assert.equal(changedOf(page), false);
  }
});

console.log(`core-path-words.mjs: ${n} passed`);
