/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for the PipeWire config popup (core/popups/setup/) and the chip each theme draws on
// its path line. The web prints the daemon's words and judges nothing, so every case reads the
// words back from the card's `path.config` it was given: the draw lists all seven lines with the
// sink first, a failing line is tagged and in its own class, hostile words are escaped, and a card
// with no `setup` (an older daemon) says `config not read` and draws no chip. Then the draws of
// Ember (a chip button after the verdict), Signal (a chip beside the verdict, on the player's
// screen and the computer's), and Paper (a note that is a button, its tick drawn as a stroke, in
// the characters its faces carry). Last the wire on the real core over jsdom: the popup is listed
// with its row for everyone (before Help), it opens, it says `config not read` for an older daemon,
// and it redraws when the card's `playing` part and the model arrive, in either order (a write's
// answer and the stream's event can come in either order).
// Run: node core-setup.mjs
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { page } from './dom.mjs';
import { createCore } from '../../../bins/svolderd/web/core/core.js';
import { HELP, POPUPS, SETUP, rowId } from '../../../bins/svolderd/web/core/popups/listed.js';
import { WORDS, pageHtml, changesOf, setupOf } from '../../../bins/svolderd/web/core/popups/setup/draw.js';
import { PARTS } from '../../../bins/svolderd/web/themes/ember/draw/parts.js';
import { notes as drawNotes } from '../../../bins/svolderd/web/themes/paper/draw/notes.js';
import { notesOf } from '../../../bins/svolderd/web/themes/paper/notes.js';
import { CHARS } from '../../../bins/svolderd/web/themes/paper/words.js';
import { computerFacts } from '../../../bins/svolderd/web/themes/signal/draw/computer.js';
import { playerScreen } from '../../../bins/svolderd/web/themes/signal/draw/player.js';

let n = 0;
const ok = async (name, fn) => { await fn(); n++; console.log('ok', name); };
console.warn = () => {};

/** The limit the daemon sends with the check (svolder-ipc's `SETUP_LIMIT`); the popup draws it as sent. */
const LIMIT = 'The limit, as the daemon words it, with <b> & "marks".';
const KEYS = [
  ['rates', '44.1-384 kHz all allowed'], ['forced rate', 'none'], ['dither', 'off'],
  ['24-bit output', 'S32LE in use'], ['no plug', 'hw:Audio'], ['stereo to stereo', 'FL FR'],
  ['no filter in the sink', 'no filter graph'],
];
const lines = (over = {}) => KEYS.map(([key, words]) => ({
  key, label: key, outcome: over[key] ? 'fail' : 'pass', words: over[key] ?? words,
}));
const PASS = {
  outcome: 'untouched', chip: 'config ✓', limit: LIMIT, summary: 'PipeWire config lets it through untouched',
  sink: 'Bifrost 3 Analog Stereo', lines: lines(),
  from_player: '44.1 kHz · S16LE · stereo', to_dac: '44.1 kHz · 32-bit (S32_LE) · stereo · hw:2',
};
const TWO = {
  outcome: 'changes', chip: 'config 2', limit: LIMIT, summary: 'PipeWire config changes the sound: 2',
  sink: 'Bifrost 3 Analog Stereo',
  lines: lines({ rates: '44.1 kHz resampled (allowed: 48)', 'forced rate': '48 kHz forced' }),
};
const UNREAD = {
  outcome: 'unread', chip: 'config ?', limit: LIMIT, summary: 'PipeWire config not read', sink: 'not read',
  lines: KEYS.map(([key]) => ({ key, label: key, outcome: 'unknown', words: 'not read' })),
};
const HOSTILE = '<img src=x onerror=alert(1)> "Q" & \'co\'';

/** A path with `setup` on it (none: an older daemon's card), the verdict of an untouched path. */
const rowWith = (setup) => ({
  label: { tone: setup?.outcome === 'changes' ? 'warn' : 'good', words: 'Path: untouched end to end' },
  hops: [
    { key: 'player', label: 'player', words: [] }, { key: 'computer', label: 'computer', words: [] },
    { key: 'dac', label: 'DAC USB 1', words: [] },
  ],
  values: [
    { text: '44.1 kHz', tone: 'dim' }, { text: 'volume 0 dB', tone: 'dim' },
    ...(setup?.outcome === 'changes' ? [{ text: setup.chip, tone: 'warn' }] : []),
  ],
  words: null,
});
const pathWith = (setup) => ({
  verdict: 'untouched', end_to_end: 'untouched', words: ['untouched'], title: '', row: rowWith(setup),
  hops: [
    { label: 'player', detail: '44.1 kHz S16LE, volume 100 %', changes: false, unknown: false },
    { label: 'DAC', detail: 'USB 1, 44.1 kHz', changes: false, unknown: false },
  ],
  ...(setup ? { config: setup } : {}),
});
const playingWith = (setup) => ({
  now_ms: 1_700_000_100_000, sink_label: false, buttons: true, path: pathWith(setup),
  track: {
    headline: 'So What', artist: 'Miles Davis', album: 'Kind of Blue', source: null,
    status: 'playing', cover: null, length_us: 562_000_000, progress: null,
  },
});
const MODEL = (setup, extra = {}) => ({
  v: 1, viewer: { guest: false, writes: true, signed_in: true, version: '0.37.1' },
  fleet: { devices: [{ url: 'A', name: 'Bifrost' }], hops: [] }, shown: 'A',
  device: { volume: { raw: 40 }, playing: playingWith(setup) }, followed: {}, card: null,
  host: null, strip: null, ...extra,
});

const rx = (text) => text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
const countOf = (html, text) => html.split(text).length - 1;

await ok('the_page_lists_the_sink_first_the_summary_the_seven_lines_and_the_limit', () => {
  for (const setup of [PASS, TWO, UNREAD]) {
    const html = pageHtml({ setup });
    assert.ok(html.indexOf(setup.sink) < html.indexOf(setup.summary), 'the sink, then the summary');
    assert.equal(countOf(html, 'data-key='), 7, 'every line, always');
    assert.ok(html.indexOf(setup.summary) < html.indexOf('data-key="rates"'));
    assert.ok(html.indexOf('data-key="no filter in the sink"') < html.indexOf(LIMIT.slice(0, 30)));
    for (const line of setup.lines) assert.ok(html.includes(line.words), line.words);
  }
  const unread = pageHtml({ setup: UNREAD });
  assert.equal(countOf(unread, 'fp-tag'), 0, 'nothing fails when nothing was read');
  assert.equal(countOf(unread, 'data-outcome="unknown"'), 7);
  assert.equal(countOf(unread, 'aria-label="not read"'), 0, '`not read` is read out once, in the words');
});

await ok('the_two_io_lines_sit_under_the_sink_in_the_daemons_words', () => {
  const html = pageHtml({ setup: PASS });
  const at = (text) => html.indexOf(text);
  assert.ok(at(PASS.sink) < at('data-io="from_player"') && at('data-io="from_player"') < at('data-io="to_dac"'));
  assert.ok(at('data-io="to_dac"') < at(PASS.summary), 'both before the summary');
  assert.ok(html.includes(`>${WORDS.fromPlayer}</span>`) && html.includes(`>${WORDS.toDac}</span>`));
  assert.equal(WORDS.fromPlayer, 'From player');
  assert.equal(WORDS.toDac, 'To DAC');
  assert.ok(html.includes(PASS.from_player) && html.includes(PASS.to_dac));
  const hostile = pageHtml({ setup: { ...PASS, from_player: HOSTILE, to_dac: HOSTILE } });
  assert.ok(!hostile.includes('<img'), 'escaped');
  // an older daemon sends neither: the rows are left out and nothing is invented
  const older = pageHtml({ setup: { ...PASS, from_player: undefined, to_dac: undefined } });
  assert.equal(countOf(older, 'data-io='), 0);
  assert.equal(countOf(older, 'data-key='), 7);
  // an empty word is left out too
  assert.equal(countOf(pageHtml({ setup: { ...PASS, to_dac: '' } }), 'data-io='), 1);
});

await ok('a_failing_line_is_tagged_and_in_its_own_class', () => {
  const html = pageHtml({ setup: TWO });
  assert.equal(countOf(html, 'data-outcome="fail"'), 2);
  assert.equal(countOf(html, `<span class="fp-tag">${WORDS.fails}</span>`), 2);
  assert.equal(countOf(html, 'fpk-fail'), 2);
  assert.equal(countOf(pageHtml({ setup: PASS }), 'fpk-fail'), 0);
  assert.match(html, /data-outcome="changes"/);
  assert.match(pageHtml({ setup: PASS }), /data-outcome="untouched"/);
});

await ok('the_daemons_words_are_printed_as_given_and_escaped', () => {
  const setup = {
    ...PASS, summary: HOSTILE, sink: HOSTILE,
    lines: [{ key: HOSTILE, label: HOSTILE, outcome: 'fail', words: HOSTILE }],
  };
  const html = pageHtml({ setup });
  assert.ok(!html.includes('<img'), html);
  assert.ok(html.includes('&lt;img src=x onerror=alert(1)&gt;'));
});

await ok('no_setup_says_config_not_read_with_no_lines', () => {
  for (const none of [null, undefined]) {
    const html = pageHtml({ setup: none });
    assert.ok(html.includes(WORDS.unread));
    assert.equal(WORDS.unread, 'config not read');
    assert.equal(countOf(html, 'fp-row'), 0);
  }
});

await ok('the_limit_is_the_daemons_sentence_escaped_and_the_popup_holds_no_copy', () => {
  assert.equal(WORDS.limit, undefined, 'the sentence is svolder-ipc\'s, not the popup\'s');
  assert.doesNotMatch(JSON.stringify(WORDS), /bit-perfect|bit-exact|setup/i);
  const html = pageHtml({ setup: PASS });
  assert.ok(html.includes('The limit, as the daemon words it, with &lt;b&gt; &amp; &quot;marks&quot;.'), html);
  assert.equal(countOf(pageHtml({ setup: { ...PASS, limit: undefined } }), 'fp-note'), 0,
    'an older daemon sends none, and none is invented');
});

await ok('setup_is_read_from_the_shown_devices_path_and_nowhere_else', () => {
  assert.equal(setupOf(MODEL(TWO)), TWO);
  assert.equal(setupOf(MODEL(null)), null, 'an older daemon');
  assert.equal(setupOf({ device: null, host: null }), null);
  assert.equal(setupOf({ device: null, host: { path: { config: PASS } } }), null,
    'the daemon never sends a config on the host\'s path');
  assert.equal(setupOf(undefined), null);
});

const CHANGES = [
  { key: 'player', label: 'player', words: [{ text: 'volume 50 %', tone: 'warn' }] },
  { key: 'computer', label: 'computer', words: [{ text: 'resampled', tone: 'warn' }, { text: '1 other sound mixing', tone: 'warn' }] },
  { key: 'dac', label: 'DAC USB 1', words: [{ text: 'volume -6.0 dB', tone: 'warn' }, { text: 'tone control', tone: 'warn' }] },
];

await ok('the_popup_names_every_hop_that_changes_the_sound_under_the_lines_and_before_the_limit', () => {
  const html = pageHtml({ setup: PASS, changes: CHANGES });
  assert.equal(countOf(html, 'data-changes'), 1);
  assert.ok(html.includes('<h3 class="fp-heading">What changes the sound</h3>'), html);
  for (const hop of ['player', 'computer', 'dac']) assert.equal(countOf(html, `data-hop="${hop}"`), 1, hop);
  assert.ok(html.indexOf('data-hop="player"') < html.indexOf('data-hop="computer"'));
  assert.ok(html.indexOf('data-hop="computer"') < html.indexOf('data-hop="dac"'));
  assert.ok(html.includes('<span class="fpk-lab">DAC USB 1</span>'), html);
  assert.ok(html.includes('<span class="fpk-fail">volume -6.0 dB</span><span class="fpk-dim"> · </span><span class="fpk-fail">tone control</span>'), html);
  assert.ok(!html.includes('changes the sound</dd>'), 'no trailing verb: the heading says it');
  const mixed = pageHtml({ setup: PASS, changes: [{ key: 'dac', label: 'DAC', words: [{ text: 'a', tone: 'warn' }, { text: '?', tone: 'dim' }] }] });
  assert.ok(mixed.includes('<span class="fpk-fail">a</span><span class="fpk-dim"> · </span><span class="fpk-dim">?</span></dd>'), 'warn yellow, dim dimmed');
  assert.ok(html.indexOf('data-outcome="untouched"') < html.indexOf('data-key="rates"'));
  assert.ok(html.indexOf('data-key="no filter in the sink"') < html.indexOf('data-changes'));
  assert.ok(html.indexOf('data-changes') < html.indexOf(LIMIT.slice(0, 30)));
  assert.equal(countOf(html, 'class="fp-row"'), 12, 'the seven lines, the two io rows and the three hops');
  for (const none of [[], null, undefined]) {
    const bare = pageHtml({ setup: PASS, changes: none });
    assert.ok(!bare.includes('data-changes') && !bare.includes(WORDS.changes), 'untouched: no heading');
  }
  assert.ok(!pageHtml({ setup: null, changes: CHANGES }).includes('data-changes'), 'nothing without a check');
  const hostile = pageHtml({ setup: PASS, changes: [{ key: 'dac', label: HOSTILE, words: [{ text: HOSTILE, tone: 'warn' }] }] });
  assert.ok(!hostile.includes('<img'), hostile);
  assert.ok(hostile.includes('data-hop="dac"'), 'the hook is the model\'s key, not the label');
});

await ok('changes_are_read_from_the_path_rows_changes_and_nowhere_else', () => {
  const model = MODEL(PASS);
  assert.deepEqual(changesOf(model), [], 'a row without any');
  model.device.playing.path.row.changes = CHANGES;
  assert.equal(changesOf(model), CHANGES);
  assert.deepEqual(changesOf({ device: null, host: { path: { row: { changes: CHANGES } } } }), []);
  assert.deepEqual(changesOf(undefined), []);
});

await ok('the_popup_is_listed_before_help_with_a_plain_row_for_everyone', () => {
  const at = POPUPS.findIndex((one) => one.name === SETUP);
  assert.equal(POPUPS[at + 1].name, HELP);
  assert.deepEqual(POPUPS[at].row, { label: 'PipeWire config…', sees: 'everyone' });
  assert.equal(POPUPS[at].title, 'PipeWire config');
  assert.equal(POPUPS[at].entries, undefined, 'one page, no side list');
  assert.equal(rowId(SETUP), 'menu-popup-setup');
  assert.doesNotMatch(JSON.stringify(POPUPS.map((one) => [one.title, one.row.label])), /setup/i);
});

// ---- Ember

await ok('ember_draws_the_config_chip_after_the_hops_only_when_the_values_carry_config_n', () => {
  const html = PARTS.playing({ playing: playingWith(TWO) }, MODEL(TWO));
  const chip = new RegExp(`<button type="button" id="np-config" class="np-setup changes" `
    + `data-config aria-haspopup="dialog" title="${rx(TWO.summary)}" `
    + `aria-label="${rx(TWO.chip)}, ${rx(TWO.summary)}">${rx(TWO.chip)}</button></p>`);
  assert.match(html, chip);
  assert.ok(html.indexOf('np-label') < html.indexOf('np-hop'));
  assert.ok(html.lastIndexOf('np-hop') < html.indexOf('np-setup'), 'after the hops');
  assert.ok(!html.includes('np-verdict'));
  // `config N` leaves the last chip's values for its own chip
  assert.equal(countOf(html, 'config 2'), 2, 'the chip and its aria-label, not the values');
});

await ok('ember_draws_no_chip_when_the_check_passes_or_was_not_read_and_the_label_still_opens_the_dialog', () => {
  for (const setup of [PASS, UNREAD]) {
    const html = PARTS.playing({ playing: playingWith(setup) }, MODEL(setup));
    assert.ok(!html.includes('np-setup') && !/config [0-9?\u2713]/.test(html), html);
    assert.ok(!html.includes('\u2713'), 'no check mark');
    assert.match(html, /<button type="button" id="np-label" class="np-label good" data-config aria-haspopup="dialog" title="Path: untouched end to end"><span aria-hidden="true">Path<\/span><span class="np-sr">Path: untouched end to end<\/span><\/button>/);
  }
});

await ok('with_no_config_check_the_label_is_plain_text_that_opens_nothing', () => {
  const html = PARTS.playing({ playing: playingWith(null) }, MODEL(null));
  assert.ok(!html.includes('data-config') && !html.includes('np-setup'), html);
  assert.match(html, /<span class="np-label good" title="Path: untouched end to end"><span aria-hidden="true">Path<\/span>/);
});

// ---- Signal

await ok('signal_draws_the_chip_beside_the_verdict_on_the_players_screen', () => {
  for (const setup of [PASS, TWO, UNREAD]) {
    const html = playerScreen(MODEL(setup));
    assert.match(html, new RegExp(`<button type="button" id="sg-config" class="sg-config ${setup.outcome}" `
      + `data-act="config" aria-haspopup="dialog" title="${rx(setup.summary)}" `
      + `aria-label="${rx(setup.chip)}, ${rx(setup.summary)}">${rx(setup.chip)}</button>`));
    assert.ok(html.indexOf('sg-verdict') < html.indexOf('sg-config'));
    assert.ok(!html.includes('sg-chip'), 'the page\'s other chips are not this one');
  }
  assert.ok(!playerScreen(MODEL(null)).includes('sg-config'), 'an older daemon');
});

await ok('signals_config_chip_has_a_class_of_its_own_and_the_old_chip_rule_is_not_widened', () => {
  const css = readFileSync(new URL('../../../bins/svolderd/web/themes/signal/style.css', import.meta.url), 'utf8');
  assert.ok(/^\.sg-config \{/m.test(css));
  const rules = css.split('\n').filter((line) => /^\.sg-chip\b/.test(line));
  assert.ok(rules.every((line) => !/margin-inline-start: 12px/.test(line)), rules.join('\n'));
  assert.ok(!/\.sg-chip \{[^}]*margin-inline-start: 12px/s.test(css), 'no later .sg-chip rule restyles them');
});

await ok('signal_draws_the_chip_on_the_computers_verdict_row_too', () => {
  const route = { sink: { words: 'Bifrost', rate_hz: 44100, format: 'S32LE' }, others_mixing: 0 };
  const withRoute = (setup) => MODEL(setup, { host: { route, path: null } });
  assert.match(computerFacts(withRoute(TWO)), /<dd class="sg-ok"[^>]*>.*sg-config changes.*config 2/s);
  assert.ok(!computerFacts(withRoute(null)).includes('sg-config'));
});

await ok('signal_colours_the_path_by_end_to_end_not_by_the_verdict_to_the_dacs_input', () => {
  const asked = (end) => {
    const model = MODEL(null);
    model.device.playing.path = { ...pathWith(null), end_to_end: end };
    model.host = { track: model.device.playing.track };
    return playerScreen(model, 1_700_000_100_000);
  };
  // the verdict (to the DAC's input) is untouched in both; the DAC's own stage decides the colour
  assert.ok(asked('untouched').includes('sg-verdict sg-ok'));
  assert.ok(asked('changed').includes('sg-verdict sg-warn'));
  // the computer's own row reads the same key
  const facts = (end) => computerFacts(MODEL(null, {
    host: { route: { sink: { words: 'Bifrost', rate_hz: 44100, format: 'S32LE' }, others_mixing: 0 },
      path: null },
    device: { ...MODEL(null).device, playing: { ...playingWith(null), path: { ...pathWith(null), end_to_end: end } } },
  }));
  assert.match(facts('untouched'), /<dd class="sg-ok"/);
  assert.match(facts('changed'), /<dd class="sg-warn"/);
});

await ok('signal_says_the_row_labels_words_not_untouched_when_the_dac_alone_changes_the_sound', () => {
  const asked = (end, words) => {
    const model = MODEL(null);
    const row = { ...rowWith(null), label: { tone: 'warn', words } };
    model.device.playing.path = { ...pathWith(null), end_to_end: end, row };
    model.host = { track: model.device.playing.track };
    return playerScreen(model, 1_700_000_100_000);
  };
  // the verdict to the DAC's input says `untouched`; the DAC's stage changes the sound
  const changed = asked('changed', 'Path: the DAC changes the sound');
  assert.ok(changed.includes('sg-verdict sg-warn'));
  assert.ok(changed.includes('>the DAC changes the sound<'), changed);
  assert.ok(!changed.includes('>untouched<'), 'yellow never says untouched');
  // the sentence behind the words (tooltip, screen reader) leads with the same words
  const titled = (end) => {
    const model = MODEL(null);
    const row = { ...rowWith(null), label: { tone: 'warn', words: 'Path: the DAC changes the sound' } };
    model.device.playing.path = { ...pathWith(null), end_to_end: end, title: 'untouched up to the DAC; volume -26.1 dB', row };
    model.host = { track: model.device.playing.track };
    return playerScreen(model, 1_700_000_100_000);
  };
  const sentence = 'the DAC changes the sound: untouched up to the DAC; volume -26.1 dB';
  assert.ok(titled('changed').includes(`title="${sentence}"`), titled('changed'));
  assert.ok(titled('changed').includes(`<span class="sg-sr">${sentence}</span>`), titled('changed'));
  assert.ok(titled('changed').includes('<span aria-hidden="true">the DAC changes the sound</span>'));
  // with the judge's own words the sentence is the judge's alone
  assert.ok(titled('untouched').includes('title="untouched up to the DAC; volume -26.1 dB"'));
  // end to end untouched: the judge's own words, as before
  assert.ok(asked('untouched', 'Path: untouched end to end').includes('sg-ok'));
  assert.ok(asked('untouched', 'Path: untouched end to end').includes('>untouched<'));
  // an older model without end_to_end: the verdict's words
  const old = MODEL(null);
  old.device.playing.path = { ...pathWith(null), end_to_end: undefined };
  old.host = { track: old.device.playing.track };
  assert.ok(playerScreen(old, 1_700_000_100_000).includes('>untouched<'));
  // the computer's own row says the same
  const facts = computerFacts(MODEL(null, {
    host: { route: { sink: { words: 'Bifrost', rate_hz: 44100, format: 'S32LE' }, others_mixing: 0 },
      path: null },
    device: { ...MODEL(null).device, playing: { ...playingWith(null),
      path: { ...pathWith(null), end_to_end: 'changed', row: { ...rowWith(null), label: { tone: 'warn', words: 'Path: the DAC changes the sound' } } } } },
  }));
  assert.ok(facts.includes('>the DAC changes the sound<') && !facts.includes('>untouched<'), facts);
});

// ---- Paper

await ok('paper_has_the_chip_as_a_note_after_the_path_note_with_the_tick_as_a_marker', () => {
  const page2 = MODEL(PASS);
  const list = notesOf(page2, page2.device);
  const ids = list.map((one) => one.id);
  assert.ok(ids.indexOf('config') === ids.indexOf('path') + 1, ids.join());
  const note = list.find((one) => one.id === 'config');
  assert.equal(note.chip, 'untouched');
  assert.equal(note.title, PASS.summary);
  assert.match(note.text.replace('\uE001', ''), CHARS, 'only characters the faces carry');
  assert.equal(note.red, false, 'the chip is never the sheet\'s one red note');
  assert.equal(notesOf(MODEL(TWO), MODEL(TWO).device).find((one) => one.id === 'config').text,
    'config 2');
  assert.ok(!notesOf(MODEL(null), MODEL(null).device).some((one) => one.id === 'config'));
});

await ok('paper_draws_the_chip_as_a_button_with_an_ink_tick_and_the_summary_as_its_name', () => {
  for (const setup of [PASS, TWO, UNREAD]) {
    const p = MODEL(setup);
    const html = drawNotes(p, p.device, { now: 1_700_000_100_000 });
    const button = html.match(/<button[^>]*data-note="config"[^>]*>.*?<\/button>/s)?.[0];
    assert.ok(button, html);
    assert.ok(button.includes(`p-setup ${setup.outcome}`) && button.includes('data-act="config"'));
    assert.ok(button.includes(`aria-label="${setup.chip}, ${setup.summary}"`));
    assert.ok(button.includes('id="p-config"'));
    assert.equal(button.includes('p-tick'), setup === PASS);
    assert.doesNotMatch(button.replace(/ aria-label="[^"]*"/, ''), /✓|/, 'the marker is turned into the stroke');
  }
});

// ---- the wire

const BODY = '<div id="theme"></div><div id="toasts"></div>';
function rig(model) {
  const p = page(BODY, { url: 'http://box.lan:9000/' });
  globalThis.window = p.window;
  globalThis.document = p.document;
  globalThis.AbortController = p.window.AbortController;
  p.window.EventSource = class { addEventListener() {} close() {} };
  p.window.matchMedia = () => ({ matches: false, addEventListener() {}, removeEventListener() {} });
  p.window.fetch = async () => ({ status: 200, ok: true, json: async () => ({}) });
  const core = createCore({ page: structuredClone(model), themes: [], window: p.window });
  core.mounted = { keys: [], zones: {}, unmount() {} };
  const $ = (id) => p.document.getElementById(id);
  const drawn = async () => {
    for (let i = 0; i < 50 && $('popup-setup')?.querySelector('.fp-box')?.dataset.state !== 'ready'; i++) {
      await p.settle();
    }
    await p.settle();
  };
  const text = () => $('popup-setup').querySelector('.fp-page').textContent;
  const open = async () => { core.router.open(SETUP); await drawn(); };
  return { ...p, core, $, open, text, drawn };
}

await ok('it_opens_from_the_router_and_prints_the_card_it_was_given', async () => {
  const r = rig(MODEL(TWO));
  await r.open();
  assert.equal(r.$('popup-setup-title').textContent, 'PipeWire config');
  assert.ok(r.text().includes(TWO.summary) && r.text().includes('44.1 kHz resampled'));
  assert.equal(r.$('popup-setup').querySelectorAll('.fp-row').length, 7);
  r.window.close();
});

await ok('an_older_daemons_card_says_config_not_read', async () => {
  const r = rig(MODEL(null));
  await r.open();
  assert.equal(r.text().trim(), 'config not read');
  r.window.close();
});

await ok('the_menu_has_the_row_for_a_guest_and_it_opens_the_popup', async () => {
  const guest = MODEL(PASS);
  guest.viewer = { guest: true, writes: false };
  const r = rig(guest);
  r.core.frame.openMenu();
  assert.equal(r.$('menu-popup-setup').textContent.trim().replace(/\s*›$/, ''),
    'PipeWire config…');
  r.$('menu-popup-setup').click();
  await r.drawn();
  assert.equal(r.core.router.state().popup, SETUP);
  assert.ok(r.text().includes(PASS.summary));
  r.window.close();
});

// A write answers with the model and the stream sends the `playing` part: the card the popup draws
// is whichever of them came last, and both carry the same check, so what matters is that each event
// alone redraws (a listener deleted fails exactly its case). Nothing writes the check, so no later
// event can overwrite a newer one: trap 1's two orders are n/a.
async function follows(name, arrive) {
  await ok(name, async () => {
    const r = rig(MODEL(PASS));
    await r.open();
    assert.ok(r.text().includes(PASS.summary));
    arrive(r.core.model);
    await r.settle();
    assert.ok(r.text().includes(TWO.summary), r.text());
    assert.ok(!r.text().includes(PASS.summary));
    r.window.close();
  });
}
await follows('a_playing_part_alone_redraws_it', (store) => {
  const next = playingWith(TWO);
  store.page().device.playing = next;
  store.emit('part', { device: 'A', name: 'playing', data: next });
});
await follows('a_model_alone_redraws_it', (store) => store.replace(MODEL(TWO)));
await follows('a_devices_shape_alone_redraws_it', (store) => {
  store.page().device = MODEL(TWO).device;
  store.emit('device', { device: 'A', data: store.page().device });
});

console.log(`core-setup.mjs: ${n} passed`);
