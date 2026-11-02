/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for the theme Paper's handwritten notes (themes/paper/notes.js, changes.js and
// draw/notes.js): which facts become notes and in whose words, the one red note and its order,
// the last change and when it is this page's own, and how a note is drawn (the arrow an ink
// stroke and the word `to`, no style attribute, only characters the faces carry). The pages are the
// fixtures'. Run: node paper-notes.mjs
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createChanges, fieldsOf } from '../../../bins/svolderd/web/themes/paper/changes.js';
import { notes as drawNotes, tiltOf } from '../../../bins/svolderd/web/themes/paper/draw/notes.js';
import { RED_ORDER, notesOf, trackLine } from '../../../bins/svolderd/web/themes/paper/notes.js';
import { ARROW, CHARS } from '../../../bins/svolderd/web/themes/paper/words.js';

let n = 0;
const ok = (name, fn) => { fn(); n++; console.log('ok', name); };

const fixture = JSON.parse(readFileSync(new URL('./fixtures/ember-parts.json', import.meta.url), 'utf8'));
const clone = (value) => JSON.parse(JSON.stringify(value));
const pageOf = (name) => clone(fixture.cases.find((c) => c.name === name).page);
const notesFor = (page, opts = {}) => notesOf(page, page.device, opts);
const reds = (list) => list.filter((x) => x.red);
const MINUS = '−';

ok('at_most_one_note_is_red_and_it_is_the_most_important', () => {
  for (const c of fixture.cases) {
    const list = notesFor(clone(c.page));
    assert.ok(reds(list).length <= 1, `${c.name}: ${reds(list).map((x) => x.id)}`);
  }
  assert.equal(reds(notesFor(pageOf('lost')))[0].id, 'card');
  assert.equal(reds(notesFor(pageOf('fault')))[0].id, 'card');
  assert.match(reds(notesFor(pageOf('dac-risk-hot')))[0].id, /^volume-\d+$/);
  const route = pageOf('playing-changed-route');
  route.device.tone.peak = null;
  const list = notesFor(route);
  assert.equal(reds(list)[0].id, 'path');
  assert.equal(reds(list)[0].text,
    '2 hops change the sound: computer and DAC');
});

ok('two_red_volume_notes_make_one_red_note_the_first', () => {
  const page = pageOf('dac-risk-hot');
  page.device.card = null;
  page.device.volume.notes = [
    { eq: false, popup: false, red: true, text: 'Too loud' },
    { eq: false, popup: false, red: true, text: 'Over the cap' },
  ];
  const list = notesFor(page);
  assert.deepEqual(reds(list).map((x) => x.id), ['volume-0']);
  assert.ok(list.some((x) => x.id === 'volume-1' && !x.red), 'the second is written, not red');
});

ok('the_red_order_is_card_volume_peak_path_passphrase', () => {
  assert.deepEqual(RED_ORDER, ['card', 'volume', 'peak', 'path', 'passphrase']);
  const page = pageOf('playing-changed-route');
  page.device.card = clone(pageOf('lost').device.card);
  page.device.volume.notes = [{ eq: false, popup: false, red: true, text: 'Too loud' }];
  page.device.facts.passphrase_note = 'Passphrase does not match';
  const without = (fn) => { fn(page.device); return reds(notesFor(page))[0]?.id; };
  assert.equal(reds(notesFor(page))[0].id, 'card');
  assert.equal(without((d) => { d.card = null; }), 'volume-0');
  assert.equal(without((d) => { d.volume.notes = []; }), 'peak');
  assert.equal(without((d) => { d.tone.peak = null; }), 'path');
  assert.equal(without((d) => { d.playing.path.class = 'ok'; }), 'passphrase');
  assert.equal(without((d) => { d.facts.passphrase_note = null; }), undefined);
  // The path note is blue words when another note is the red one.
  const again = pageOf('playing-changed-route');
  const list = notesFor(again);
  assert.equal(reds(list)[0].id, 'peak');
  assert.equal(list.find((x) => x.id === 'path').text,
    `48 kHz ${ARROW} 96 kHz ${ARROW} 48 kHz · resampled 48 ${ARROW} 96 kHz · host EQ on · DAC reads 48 kHz`);
});

ok('the_card_is_a_note_here_and_drawn_once_elsewhere', () => {
  const list = notesFor(pageOf('lost'));
  const card = list.find((x) => x.id === 'card');
  assert.equal(card.about, 'card');
  assert.equal(card.text, 'Gungnir 2 is out of reach');
  const none = pageOf('no-devices');
  assert.ok(notesOf(none, null).find((x) => x.id === 'card'), 'a page card is a card note');
  assert.ok(!drawNotes(pageOf('lost'), pageOf('lost').device, { now: 0 }).includes('data-note="card"'));
});

ok('the_eq_is_said_in_words_for_each_mode', () => {
  const eq = (name, mutate) => {
    const page = pageOf(name);
    mutate?.(page.device);
    return notesFor(page).find((x) => x.id === 'eq').text;
  };
  assert.equal(eq('dac-flat'), 'flat: the bands are bypassed');
  assert.equal(eq('dac-loudness-strength'), 'loudness, strength 40');
  assert.equal(eq('dac'), 'Warm: +3 dB at 40 Hz, +2 dB at 10 kHz');
  const unpreset = (d) => { d.presets.status = null; d.presets.rows.forEach((r) => { r.on = false; }); };
  assert.equal(eq('dac', unpreset), '+3 dB at 40 Hz, +2 dB at 10 kHz');
  assert.equal(eq('dac-edited-preset'), 'EQ: +4.5 dB at 40 Hz, +2 dB at 10 kHz');
  assert.equal(eq('dac', (d) => { d.tone.markers.forEach((m) => { m.db = 0; }); }),
    'EQ on, every band at 0 dB');
  assert.equal(eq('dac', (d) => {
    unpreset(d);
    d.tone.markers = [{ band: 1, db: -1.3, hz: 999.7 }, { band: 2, db: 0.1, hz: 60 },
      { band: 3, db: 4.8, hz: 2515 }];
  }), `+5 dB at 2.5 kHz, ${MINUS}1.5 dB at 1 kHz`);
});

ok('the_peak_is_said_once', () => {
  const ids = (page) => notesFor(page).map((x) => x.id);
  const volumeEq = (id, list) => list.filter((x) => x.id === id);
  // not at risk: the volume's own EQ note is dropped, the peak note carries the threshold
  const calm = pageOf('dac');
  assert.deepEqual(ids(calm).filter((id) => id.startsWith('volume-')), []);
  const peak = notesFor(calm).find((x) => x.id === 'peak');
  assert.ok(peak.red);
  assert.equal(peak.text, `peak +3.3 dB: above ${MINUS}3.3 dB, loud tracks may clip`);
  // at risk: the red volume note says it, the peak note is dropped
  const hot = pageOf('dac-risk-hot');
  const list = notesFor(hot);
  assert.ok(!ids(hot).includes('peak'));
  assert.equal(volumeEq('volume-0', list).length, 1);
  assert.ok(list.find((x) => x.id === 'volume-0').red);
  assert.match(list.find((x) => x.id === 'volume-0').text, /This EQ peaks at/);
  // a note of the volume that is no EQ note stays beside the peak note
  const cap = pageOf('dac-climb-cap-risk');
  assert.ok(ids(cap).includes('peak'));
  assert.equal(ids(cap).filter((id) => id.startsWith('volume-')).length, 1, ids(cap).join());
  // no peak: nothing of it is said, and the volume's EQ note (should the model send one) stays
  const none = pageOf('dac');
  none.device.tone.peak = null;
  assert.ok(notesFor(none).find((x) => x.id === 'volume-0'));
});

ok('the_path_note_names_the_one_hop_that_changes', () => {
  const page = pageOf('playing-changed-route');
  page.device.tone.peak = null;
  page.device.playing.path.hops[1].changes = false;
  const note = notesFor(page).find((x) => x.id === 'path');
  assert.ok(note.red);
  assert.equal(note.text, 'DAC: USB 1, 48 kHz, the only hop that changes the sound');
});

ok('the_rate_note_says_when_the_rate_matches', () => {
  const note = notesFor(pageOf('playing')).find((x) => x.id === 'path');
  assert.equal(note.text, '48 kHz · rate matches all the way · untouched');
  assert.ok(!note.red);
  const bare = pageOf('playing');
  bare.device.playing.path.hops = [];
  assert.equal(notesFor(bare).find((x) => x.id === 'path').text, 'untouched');
});

ok('the_track_note_counts_its_time_and_names_the_album', () => {
  const page = pageOf('playing');
  page.device.playing.track.album = 'Demo Tape';
  const at = page.device.playing.track.progress.read_at_ms;
  const track = (ms) => notesFor(page, { clockNowMs: ms }).find((x) => x.id === 'track').text;
  assert.equal(track(at), '0:30 of 20:00 · Demo Tape');
  assert.equal(track(at + 5000), '0:35 of 20:00 · Demo Tape');
  page.device.playing.track.status = 'paused';
  assert.equal(track(at + 5000), '0:30 of 20:00 · Demo Tape');
  delete page.device.playing.track.album;
  assert.equal(track(at), '0:30 of 20:00');
  page.device.playing.track.progress = null;
  assert.equal(track(at), '20:00');
  page.device.playing.track.length_us = null;
  assert.ok(!notesFor(page, { clockNowMs: at }).some((x) => x.id === 'track'));
});

ok('the_facts_and_the_other_sections_have_their_notes', () => {
  const page = pageOf('dac-facts-heard');
  const list = notesFor(page);
  const text = (id) => list.find((x) => x.id === id)?.text;
  assert.equal(text('firmware'), `firmware ${page.device.facts.firmware} · DSP ${page.device.facts.dsp}`);
  assert.equal(text('link'), `link ${page.device.facts.signal}`);
  assert.equal(text('passphrase'), 'Passphrase does not match the code the device advertises');
  page.device.facts.dsp = null;
  assert.equal(notesFor(page).find((x) => x.id === 'firmware').text,
    `firmware ${page.device.facts.firmware}`);
  assert.equal(notesFor(pageOf('dac-presets-off')).find((x) => x.id === 'presets').text,
    'Saving presets is off: the configuration file is read-only');
  assert.equal(notesFor(pageOf('amp')).find((x) => x.id === 'power-warming_up').text, 'warming up: no');
  const risk = notesFor(pageOf('dac-climb-cap-risk')).filter((x) => x.about === 'volume');
  assert.equal(risk.length, 1, 'the climb popup is not a note and the EQ peak is said by its own note');
  assert.equal(risk[0].id, 'volume-2');
});

// ---- the last change ----

const AT = new Date(2026, 0, 1, 12, 4).getTime();
const raw = (device, value) => { device.volume.raw = value; };
const volumeText = (device, value) => device.volume.texts[value - device.volume.lo];

ok('a_change_from_elsewhere_gives_its_time_only', () => {
  const page = pageOf('dac');
  const keeper = createChanges();
  raw(page.device, 40);
  assert.equal(keeper.seen(page.device, AT - 60000, []), null, 'the first reading is no change');
  keeper.baseline(page.device);
  raw(page.device, 42);
  const change = keeper.seen(page.device, AT, []);
  assert.equal(change.mine, false);
  const text = notesFor(page, { change }).find((x) => x.id === 'change');
  assert.equal(text.text,
    `last change: ${volumeText(page.device, 40)} ${ARROW} ${volumeText(page.device, 42)}, 12:04`);
  assert.equal(text.about, 'volume');
  assert.ok(!text.red);
});

ok('a_change_this_page_sent_says_from_this_browser', () => {
  const page = pageOf('dac');
  const change = (log, at = AT) => {
    const keeper = createChanges();
    raw(page.device, 40);
    keeper.baseline(page.device);
    raw(page.device, 42);
    return keeper.seen(page.device, at, log);
  };
  const said = (c, opts = {}) => notesFor(page, { change: c, ...opts }).find((x) => x.id === 'change').text;
  const mine = change([{ field: 'volume', value: 42, at: AT - 500 }]);
  assert.ok(said(mine).endsWith(', 12:04, from this browser'), said(mine));
  assert.ok(said(mine, { coarse: true }).endsWith(', from this phone'));
  const late = change([{ field: 'volume', value: 42, at: AT - 4000, answeredAt: AT - 3900, ok: true }]);
  assert.ok(!said(late).includes('from this'), said(late));
  const waiting = change([{ field: 'volume', value: 42, at: AT - 1000, answeredAt: null, ok: null }]);
  assert.ok(said(waiting).endsWith('from this browser'));
  const other = change([{ field: 'volume', value: 43, at: AT - 500 }]);
  assert.ok(!said(other).includes('from this'));
  const refused = change([{ field: 'volume', value: 42, at: AT - 500, ok: false }]);
  assert.ok(!said(refused).includes('from this'));
});

ok('a_request_never_answered_stops_counting', () => {
  const page = pageOf('dac');
  const keeper = createChanges();
  raw(page.device, 40);
  keeper.baseline(page.device);
  raw(page.device, 42);
  const stuck = [{ field: 'volume', value: 42, at: AT - 60000, answeredAt: null, ok: null }];
  assert.equal(keeper.seen(page.device, AT, stuck).mine, false);
});

ok('changes_together_tell_one_in_a_fixed_order', () => {
  const run = (mutate) => {
    const page = pageOf('dac');
    const keeper = createChanges();
    keeper.baseline(page.device);
    mutate(page.device);
    return keeper.seen(page.device, AT, []).field;
  };
  const vol = (d) => { raw(d, 50); };
  const mute = (d) => { d.volume.muted = true; };
  const mode = (d) => { d.tone.mode = 'flat'; };
  const preset = (d) => { d.presets.status.name = 'Bass'; };
  assert.equal(run((d) => { mute(d); vol(d); }), 'volume');
  assert.equal(run((d) => { vol(d); mode(d); }), 'mode');
  assert.equal(run((d) => { mode(d); preset(d); vol(d); }), 'preset');
});

ok('a_part_with_no_clock_shows_no_time', () => {
  const page = pageOf('playing');
  page.device.playing.track.album = 'Demo Tape';
  delete page.device.playing.now_ms;
  const html = drawNotes(page, page.device, { now: 5000 });
  assert.ok(!html.includes('NaN'), html);
  assert.equal(notesFor(page).find((x) => x.id === 'track').text, '20:00 · Demo Tape');
  assert.ok(!html.includes('data-clock'));
  // the tick's line is the first draw's line, an album's own arrow included
  page.device.playing.track.album = 'A \u2192 B';
  const line = trackLine(page.device.playing.track, 1700000097000);
  assert.equal(line, `0:30 of 20:00 · A ${ARROW} B`);
});

ok('a_drag_of_many_steps_is_one_change', () => {
  const page = pageOf('dac');
  const keeper = createChanges();
  raw(page.device, 40);
  keeper.baseline(page.device);
  let change = null;
  for (let step = 41; step <= 50; step++) {
    raw(page.device, step);
    change = keeper.seen(page.device, AT + (step - 41) * 200, []);
  }
  assert.equal(change.from, volumeText(page.device, 40));
  assert.equal(change.to, volumeText(page.device, 50));
  assert.equal(keeper.last(), change);
  // A pause longer than a run starts a new change from where it stood.
  raw(page.device, 55);
  const next = keeper.seen(page.device, AT + 10000, []);
  assert.equal(next.from, volumeText(page.device, 50));
});

ok('the_first_reading_and_a_new_model_are_not_changes', () => {
  const page = pageOf('dac');
  const keeper = createChanges();
  assert.equal(keeper.seen(page.device, AT, []), null);
  assert.equal(keeper.last(), null);
  raw(page.device, 50);
  keeper.baseline(page.device);
  assert.equal(keeper.seen(page.device, AT, []), null, 'a replaced model is remembered, no change');
  raw(page.device, 51);
  assert.ok(keeper.seen(page.device, AT, []));
  // The same device replaced keeps the note; another device drops it.
  keeper.baseline(page.device);
  assert.ok(keeper.last());
  keeper.baseline({ ...page.device, url: 'other' });
  assert.equal(keeper.last(), null);
});

ok('every_kind_of_change_is_said_in_its_words', () => {
  const run = (name, mutate, log = []) => {
    const page = pageOf(name);
    const keeper = createChanges();
    keeper.baseline(page.device);
    mutate(page.device);
    const change = keeper.seen(page.device, AT, log);
    const note = notesFor(page, { change }).find((x) => x.id === 'change');
    return note;
  };
  assert.equal(run('dac', (d) => { d.volume.muted = true; }).text, 'last change: muted, 12:04');
  assert.equal(run('dac-muted', (d) => { d.volume.muted = false; }).text, 'last change: unmuted, 12:04');
  const input = pageOf('dac').device.controls.find((c) => c.key === 'input');
  const was = input.options.find((o) => o.on).label;
  const next = input.options.find((o) => !o.on).label;
  const choice = run('dac', (d) => {
    const c = d.controls.find((x) => x.key === 'input');
    c.options.forEach((o) => { o.on = o.label === next; });
  }, [{ field: 'control:input', value: next, at: AT - 100 }]);
  assert.equal(choice.text, `last change: Input ${was} ${ARROW} ${next}, 12:04, from this browser`);
  assert.equal(choice.about, 'controls');
  const phase = pageOf('dac').device.controls.find((c) => c.kind === 'toggle');
  const toggled = run('dac', (d) => { d.controls.find((c) => c.key === phase.key).on = !phase.on; });
  assert.equal(toggled.text, `last change: ${phase.label} ${phase.on ? 'off' : 'on'}, 12:04`);
  const mode = run('dac', (d) => { d.tone.mode = 'flat'; });
  assert.equal(mode.text, `last change: tone EQ ${ARROW} Flat, 12:04`);
  assert.equal(mode.about, 'tone');
  const preset = run('dac', (d) => { d.presets.status.name = 'Bass'; });
  assert.equal(preset.text, 'last change: preset Bass, 12:04');
  assert.equal(preset.about, 'presets');
  assert.ok([...fieldsOf(pageOf('dac').device).keys()].includes('mode'));
});

ok('each_note_names_what_it_is_about', () => {
  const ABOUT = new Set(['card', 'volume', 'tone', 'playing', 'device', 'power', 'presets', 'controls']);
  for (const c of fixture.cases) {
    const page = clone(c.page);
    for (const note of notesFor(page)) {
      assert.ok(ABOUT.has(note.about), `${c.name}: ${note.id} is about ${note.about}`);
      assert.ok(note.text.length > 0);
    }
  }
});

// ---- how a note is drawn ----

ok('a_note_draws_its_arrow_as_an_ink_stroke_and_says_to', () => {
  const page = pageOf('playing-changed-route');
  const html = drawNotes(page, page.device, { now: 0 });
  assert.ok(!html.includes('→') && !html.includes(ARROW), 'no arrow glyph, no marker left');
  const strokes = html.match(/<svg class="p-arrow" aria-hidden="true"/g) ?? [];
  assert.equal(strokes.length, 3, 'two in the rates, one in the host\'s words');
  assert.ok(html.includes('<span class="p-sr"> to </span>'));
  const plain = html.replace(/<span class="p-sr">([^<]*)<\/span>/g, '$1').replace(/<[^>]*>/g, '');
  assert.ok(plain.includes('48 kHz to 96 kHz to 48 kHz · resampled 48 to 96 kHz'), plain);
  const tick = drawNotes(pageOf('playing'), pageOf('playing').device, { now: 0 });
  assert.ok(tick.includes('rate matches all the way<svg class="p-tick" aria-hidden="true"'), tick);
});

ok('drawn_notes_are_ink_with_a_tilt_and_no_style_attribute', () => {
  for (const c of fixture.cases) {
    const page = clone(c.page);
    const html = drawNotes(page, page.device, { now: 0, coarse: false });
    assert.ok(!/\sstyle=/.test(html), `${c.name}: style attribute`);
    const text = html.replace(/<[^>]*>/g, '').replace(/&#39;/g, '\'').replace(/&quot;/g, '"')
      .replace(/&amp;/g, '&').replace(/&lt;/g, '<').replace(/&gt;/g, '>');
    assert.match(text, CHARS, `${c.name}: ${text}`);
    for (const m of html.matchAll(/<span class="pn t([0-4])( red)?" data-note="([^"]+)" data-n="(\d+)"/g)) {
      assert.equal(Number(m[1]), tiltOf(m[3]), 'the tilt follows the id');
    }
    assert.ok((html.match(/ red"/g) ?? []).length <= 1, `${c.name}: one red note`);
  }
  assert.equal(tiltOf('peak'), tiltOf('peak'));
  assert.ok(new Set(['peak', 'eq', 'path', 'track', 'firmware', 'link'].map(tiltOf)).size > 1);
});

ok('a_note_with_markup_in_its_words_is_escaped', () => {
  const page = pageOf('dac');
  page.device.presets.note = '<img src=x onerror=alert(1)> & "more"';
  const html = drawNotes(page, page.device, { now: 0 });
  assert.ok(!html.includes('<img'), html);
  assert.ok(html.includes('&lt;img src=x onerror=alert(1)&gt; &amp; &quot;more&quot;'));
});

ok('the_track_note_carries_what_keeps_its_time', () => {
  const page = pageOf('playing');
  const html = drawNotes(page, page.device, { now: 5000 });
  const m = /<span class="pn t\d" data-note="track"([^>]*)>/.exec(html);
  assert.ok(m, html);
  assert.match(m[1], /data-clock="1700000100000"/);
  assert.match(m[1], /data-drawn="5000"/);
  assert.match(m[1], /data-position="30000000" data-rate="1000" data-read-at="1700000097000"/);
  // The same part drawn later is on the daemon's clock, not frozen at the part's own time.
  const later = drawNotes(page, page.device, { now: 8000 });
  assert.match(later, /data-clock="1700000103000"/);
  // A fresh part is read at its own time.
  page.device.playing = { ...page.device.playing, now_ms: 1700000200000 };
  assert.match(drawNotes(page, page.device, { now: 9000 }), /data-clock="1700000200000"/);
});

console.log(`paper-notes: ${n} passed`);
