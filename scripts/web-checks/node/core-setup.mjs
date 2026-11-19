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
import { WORDS, pageHtml, setupOf } from '../../../bins/svolderd/web/core/popups/setup/draw.js';
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
const pathWith = (setup) => ({
  verdict: 'untouched', words: ['untouched'], title: '',
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
    assert.equal(countOf(html, 'class="fp-row"'), 7, 'every line, always');
    assert.ok(html.indexOf(setup.summary) < html.indexOf('data-key="rates"'));
    assert.ok(html.indexOf('data-key="no filter in the sink"') < html.indexOf(LIMIT.slice(0, 30)));
    for (const line of setup.lines) assert.ok(html.includes(line.words), line.words);
  }
  const unread = pageHtml({ setup: UNREAD });
  assert.equal(countOf(unread, 'fp-tag'), 0, 'nothing fails when nothing was read');
  assert.equal(countOf(unread, 'data-outcome="unknown"'), 7);
  assert.equal(countOf(unread, 'aria-label="not read"'), 0, '`not read` is read out once, in the words');
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

await ok('ember_draws_a_chip_button_after_the_verdict_in_the_daemons_words', () => {
  for (const setup of [PASS, TWO, UNREAD]) {
    const html = PARTS.playing({ playing: playingWith(setup) }, MODEL(setup));
    const chip = new RegExp(`<button type="button" id="np-config" class="np-setup ${setup.outcome}" `
      + `data-config aria-haspopup="dialog" title="${rx(setup.summary)}" `
      + `aria-label="${rx(setup.chip)}, ${rx(setup.summary)}">${rx(setup.chip)}</button>`);
    assert.match(html, chip);
    assert.ok(html.indexOf('np-verdict') < html.indexOf('np-setup'));
    assert.ok(html.indexOf('np-setup') < html.indexOf('np-hop'), 'before the hops');
  }
});

await ok('ember_draws_no_chip_for_an_older_daemon', () => {
  const html = PARTS.playing({ playing: playingWith(null) }, MODEL(null));
  assert.ok(!html.includes('np-setup') && !html.includes('config'));
  assert.ok(html.includes('np-verdict'));
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
