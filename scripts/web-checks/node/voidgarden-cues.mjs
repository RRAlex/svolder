/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for Void Garden's signal cues (themes/voidgarden/cues.js), on page models per cue
// (fixtures/voidgarden-pages.json, shaped as the daemon's `model` event): a muted DAC, a muted
// sink, a paused player, crossfeed on, a lost link, a weak signal (fresh, old, at the edge), the
// changed flag passed in, 2 s of silent frames, the link drop seen once, the volume, the rates.
// Run: node voidgarden-cues.mjs
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import {
  CueTracker, SIGNAL_OLD_SECS, SILENT_PAUSE_S, STATUS_PAUSE_S, WEAK_BELOW_DBM, cuesOf, dbmOf, rateOf, weakOf,
} from '../../../bins/svolderd/web/themes/voidgarden/cues.js';

let n = 0;
const ok = (name, fn) => { fn(); n++; console.log('ok', name); };

const doc = JSON.parse(readFileSync(new URL('./fixtures/voidgarden-pages.json', import.meta.url)));
const P = doc.pages;
const NOW = doc.now_ms;
const REST = {
  mute: false, pause: false, changed: false, cross: false, weak: false, linkDrop: false, volume: 0.5, rateHz: 96000, inputRateHz: 96000,
};
/** The cues of page `name`, against the resting cues: only `over` differs. */
const only = (name, over, opts) => assert.deepEqual(cuesOf(P[name], opts), { ...REST, ...over }, name);

ok('the_quiet_page_has_no_cue_and_every_cue_key_is_there', () => {
  only('base', {});
  for (const name of Object.keys(P)) {
    assert.deepEqual(Object.keys(cuesOf(P[name])).sort(), Object.keys(REST).sort(), name);
  }
});

ok('a_muted_dac_or_a_muted_sink_is_mute', () => {
  only('muted_dac', { mute: true });
  only('muted_sink', { mute: true });
});

ok('a_player_that_is_not_playing_is_pause_and_no_player_is_not', () => {
  only('paused_player', { pause: true });
  only('stopped_player', { pause: true });
  only('no_player', {});
  only('no_host', { rateHz: null });
});

ok('crossfeed_on_in_the_paths_words_is_cross_and_other_stage_words_are_not', () => {
  only('crossfeed_on', { cross: true });
  only('other_stage_words', {});
});

ok('changed_is_the_wires_and_the_path_is_never_judged_here', () => {
  only('base', { changed: true }, { changed: true });
  // the fixture's path says `changed` and names crossfeed: only the flag decides `changed`
  assert.equal(P.crossfeed_on.device.playing.path.verdict, 'changed');
  only('crossfeed_on', { cross: true });
  only('crossfeed_on', { cross: true, changed: true }, { changed: true });
});

ok('a_fresh_reading_below_80_dbm_is_weak_and_nothing_else_is', () => {
  assert.equal(WEAK_BELOW_DBM, -80);
  only('weak_signal', { weak: true });
  only('weak_live', { weak: true });
  only('edge_signal', {});
  only('old_weak_signal', {});
  only('no_signal', {});
  only('no_facts', {});
  // the age is the daemon's clock: far enough on, the same reading is old
  assert.equal(weakOf(P.weak_signal, NOW), true);
  assert.equal(weakOf(P.weak_signal, NOW + (SIGNAL_OLD_SECS - 15) * 1000), true);
  assert.equal(weakOf(P.weak_signal, NOW + (SIGNAL_OLD_SECS + 15) * 1000), false);
  assert.equal(weakOf(null), false);
});

ok('the_signals_words_read_as_dbm_with_either_minus', () => {
  assert.equal(dbmOf('−58 dBm live'), -58);
  assert.equal(dbmOf('-83 dBm advert'), -83);
  assert.equal(dbmOf('5 dBm live'), 5);
  assert.equal(dbmOf('—'), null);
  assert.equal(dbmOf(undefined), null);
});

ok('the_volume_is_the_shown_sides_fraction', () => {
  only('base', {});
  assert.equal(cuesOf(P.base, { side: 'computer' }).volume, 0.25);
  assert.equal(cuesOf(P.unread_volume).volume, null);
  assert.equal(cuesOf(P.amp_no_volume).volume, null);
  assert.equal(cuesOf(P.no_host, { side: 'computer' }).volume, null);
  const top = structuredClone(P.base);
  top.device.volume.raw = 200;
  assert.equal(cuesOf(top).volume, 1);
  top.device.volume.raw = 0;
  assert.equal(cuesOf(top).volume, 0);
});

ok('the_rates_are_the_sinks_and_the_devices_input', () => {
  assert.equal(cuesOf(P.base).rateHz, 96000);
  assert.equal(cuesOf(P.base).inputRateHz, 96000);
  const other = structuredClone(P.base);
  other.host.route.sink.rate_hz = 44100;
  other.device.controls = other.device.controls.map((c) => (c.key === 'sample_rate' ? { ...c, text: '44.1 kHz' } : c));
  assert.equal(cuesOf(other).rateHz, 44100);
  assert.equal(cuesOf(other).inputRateHz, 44100);
  assert.equal(rateOf('48 kHz'), 48000);
  assert.equal(rateOf('44.1 kHz'), 44100);
  assert.equal(rateOf('44,1 kHz'), 44100);
  assert.equal(rateOf('48000 Hz'), 48000);
  assert.equal(rateOf('locked'), null);
  assert.equal(rateOf(undefined), null);
});

ok('a_stale_paused_player_does_not_flicker_the_scene_on_isolated_quiet_frames', () => {
  assert.equal(STATUS_PAUSE_S, 0.5);
  for (const rate of [30, 60, 144]) {
    const t = new CueTracker();
    // music with a quiet frame now and then (a fade, a gap) under a player that says paused
    for (let i = 0; i < rate * 4; i++) {
      const silent = i % 4 === 2;
      assert.equal(t.update(P.paused_player, { dt: 1 / rate, silent }).pause, false, `${rate}/s frame ${i}`);
    }
  }
});

ok('two_seconds_of_silent_frames_also_pause_and_sound_ends_it', () => {
  assert.equal(SILENT_PAUSE_S, 2);
  const t = new CueTracker();
  for (let i = 0; i < 19; i++) assert.equal(t.update(P.base, { dt: 0.1, silent: true }).pause, false, `at ${i}`);
  assert.equal(t.update(P.base, { dt: 0.1, silent: true }).pause, true, 'at 2.0 s');
  assert.equal(t.update(P.base, { dt: 0.1, silent: true }).pause, true);
  assert.equal(t.update(P.base, { dt: 0.1, silent: false }).pause, false, 'sound again');
  for (let i = 0; i < 19; i++) t.update(P.base, { dt: 0.1, silent: true });
  assert.equal(t.update(P.base, { dt: 0.1, silent: false }).pause, false, 'the count starts over');
  assert.equal(t.update(P.base, { dt: 0.1, silent: true }).pause, false);
  // a player that is paused is paused while the frames are silent; sound heard outranks it (the
  // capture is of the whole sink: a stale paused player must not rest the scene)
  const quiet = new CueTracker();
  for (let i = 0; i < 4; i++) assert.equal(quiet.update(P.paused_player, { dt: 0.1, silent: true }).pause, false, `quiet ${i}`);
  assert.equal(quiet.update(P.paused_player, { dt: 0.1, silent: true }).pause, true, 'a paused player after 0.5 s of silence');
  assert.equal(new CueTracker().update(P.paused_player, { dt: 0.1, silent: false }).pause, false);
  assert.equal(new CueTracker().update(P.stopped_player, { dt: 0.1, silent: false }).pause, false);
  // the same at any step rate: 2 s is 2 s
  for (const rate of [30, 60, 144]) {
    const r = new CueTracker();
    let first = null;
    for (let i = 1; i <= rate * 3 && first === null; i++) {
      if (r.update(P.base, { dt: 1 / rate, silent: true }).pause) first = i / rate;
    }
    assert.ok(Math.abs(first - 2) <= 1 / rate + 1e-9, `${first} at ${rate}`);
  }
});

ok('a_link_that_leaves_on_is_one_link_drop_and_only_once', () => {
  const t = new CueTracker();
  assert.equal(t.update(P.base).linkDrop, false);
  assert.equal(t.update(P.link_lost).linkDrop, true, 'on to off');
  assert.equal(t.update(P.link_lost).linkDrop, false, 'once');
  assert.equal(t.update(P.link_waiting).linkDrop, false, 'off to wait is not a drop');
  assert.equal(t.update(P.base).linkDrop, false, 'back on');
  assert.equal(t.update(P.link_waiting).linkDrop, true, 'on to wait');
  assert.equal(t.update(P.base).linkDrop, false);
  assert.equal(t.update(P.link_lost).linkDrop, true, 'and again');
  // a page that opens with the link already lost has seen no drop
  const fresh = new CueTracker();
  assert.equal(fresh.update(P.link_lost).linkDrop, false);
  assert.equal(fresh.update(P.base).linkDrop, false);
  // no device and no entry: nothing to say, and the state kept
  const gone = new CueTracker();
  gone.update(P.base);
  assert.equal(gone.update({ shown: null, fleet: { devices: [] } }).linkDrop, false);
  assert.equal(gone.update(P.link_lost).linkDrop, true);
});

ok('showing_another_device_is_never_a_link_drop', () => {
  // the page shows the DAC (on), then an amp that is still connecting: a tear, not a drop
  const amp = structuredClone(P.link_waiting);
  amp.shown = 'Amp';
  amp.device.url = 'Amp';
  amp.device.name = 'Amp';
  const t = new CueTracker();
  assert.equal(t.update(P.base).linkDrop, false);
  assert.equal(t.update(amp).linkDrop, false, 'the shown device changed');
  assert.equal(t.update(amp).linkDrop, false);
  // and back: the DAC is on again, no drop either; its own drop is still seen
  assert.equal(t.update(P.base).linkDrop, false);
  assert.equal(t.update(P.link_lost).linkDrop, true, 'the same device leaves on');
  // the amp's own link going on then off is a drop of the amp
  const ampOn = structuredClone(amp);
  ampOn.device.link = { word: 'connected', state: 'on', old: false };
  t.update(ampOn);
  assert.equal(t.update(amp).linkDrop, true, 'the amp lost its link');
});

ok('the_link_is_read_from_the_fleet_when_the_device_has_none', () => {
  const entry = structuredClone(P.base);
  delete entry.device.link;
  const lost = structuredClone(entry);
  lost.fleet.devices[0].link = { word: 'not connected', state: 'off', old: true };
  const t = new CueTracker();
  t.update(entry);
  assert.equal(t.update(lost).linkDrop, true);
});

ok('the_tracker_passes_the_flags_and_side_through', () => {
  const t = new CueTracker();
  const c = t.update(P.crossfeed_on, { changed: true, side: 'computer', dt: 0.1 });
  assert.equal(c.changed, true);
  assert.equal(c.cross, true);
  assert.equal(c.volume, 0.25);
});

console.log(`${n} ok`);
