/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for the theme Night drive's HUD (themes/nightdrive/): the top line, the EQ line and
// the volume as the model's words, the outrun volume's SVG, and the fade with a fake clock.
// Run: node nightdrive-hud.mjs
import assert from 'node:assert/strict';
import { eqLine, gainWords, topLine, volumeOf } from '../../../bins/svolderd/web/themes/nightdrive/hud.js';
import { outrunVolume } from '../../../bins/svolderd/web/themes/nightdrive/draw/volume.js';
import { drawDash, moveProgress } from '../../../bins/svolderd/web/themes/nightdrive/draw/dash.js';
import { inertOf } from '../../../bins/svolderd/web/themes/nightdrive/inert.js';
import { createFade, fadeAfter, wireHud } from '../../../bins/svolderd/web/themes/nightdrive/hud-wire.js';
import { REST_MS, createRest } from '../../../bins/svolderd/web/themes/nightdrive/rest.js';
import { switchKeys } from '../../../bins/svolderd/web/themes/nightdrive/draw/dash.js';
import { eqMarkup } from '../../../bins/svolderd/web/themes/nightdrive/draw/hud.js';
import { computerView } from '../../../bins/svolderd/web/themes/nightdrive/computer-view.js';
import { DAC as RIG_DAC, GUEST, computerRig, modelOf } from './nightdrive-rig.mjs';

let n = 0;
const ok = (name, fn) => { fn(); n++; console.log('ok', name); };

const DAC = { name: 'Bifrost 3', url: 'Bifrost%203', family: 'dac', family_word: 'DAC' };
const AMP = { name: 'Asgard 3', url: 'Asgard%203', family: 'amp', family_word: 'Amp' };
const pathOf = (changes) => ({
  hops: [
    { label: 'player', detail: '96 kHz S16LE', changes: false },
    { label: 'computer', detail: '96 kHz S32LE', changes },
    { label: 'DAC', detail: 'USB 1, 96 kHz', changes: false },
  ],
});
const controls = () => [
  { key: 'input', kind: 'choice', options: [{ label: 'Optical', on: false }, { label: 'USB', on: true }] },
  { key: 'sample_rate', kind: 'readout', text: '96 kHz' },
];
function page(more = {}, deviceMore = {}) {
  return {
    shown: DAC.url,
    fleet: { devices: [DAC, AMP], hops: [] },
    host: { route: { host_eq: false } },
    device: {
      name: 'Bifrost 3', url: DAC.url, controls: controls(), card: null,
      volume: { text: '-23.5 dB', unit: 'dB', muted: false, raw: 40 },
      playing: { path: pathOf(false) },
      ...deviceMore,
    },
    ...more,
  };
}
const toneOf = (more = {}) => ({
  mode: 'eq', loud: false, strength: null,
  bands: { rows: [{ g: '+3.04 dB' }, { g: '+1.45 dB' }, { g: '+0.0 dB' }, { g: '-1.0 dB' }, { g: '+2.0 dB' }] },
  ...more,
});

ok('the_top_line_names_device_input_and_rate', () => {
  const t = topLine(page(), 'open');
  assert.equal(t.left, 'Bifrost 3 · USB · 96 kHz');
  assert.equal(t.right, '');
  assert.equal(t.tone, null);
  const bare = topLine(page({}, { controls: [] }), 'open');
  assert.equal(bare.left, 'Bifrost 3');
});

ok('the_verdict_shows_when_the_computer_changes_the_sound', () => {
  const p = page({ host: { route: { host_eq: true } } }, { playing: { path: pathOf(true) } });
  const t = topLine(p, 'open');
  assert.equal(t.right, 'host EQ on · changes the sound');
  assert.equal(t.tone, 'warn');
  const quiet = topLine(page({ host: { route: { host_eq: true } } }), 'open');
  assert.equal(quiet.right, 'host EQ on');
  assert.equal(quiet.tone, null);
  assert.equal(topLine(page({ host: { route: { host_eq: false } } }), 'open').right, '');
});

ok('trouble_replaces_the_verdict', () => {
  const eq = { host: { route: { host_eq: true } } };
  const lost = topLine(page(eq, { playing: { path: pathOf(true) } }), 'lost');
  assert.deepEqual([lost.right, lost.tone], ['reconnecting', 'bad']);
  const card = topLine(page(eq, { card: { kind: 'lost', title: 'Out of reach' } }), 'open');
  assert.deepEqual([card.right, card.tone], ['out of reach', 'bad']);
  const fault = topLine(page(eq, { card: { kind: 'fault', title: 'Fault' } }), 'open');
  assert.deepEqual([fault.right, fault.tone], ['fault', 'bad']);
  const wait = topLine(page(eq), 'waiting');
  assert.deepEqual([wait.right, wait.tone], ['reading again', 'warn']);
  assert.equal(topLine(page(eq), 'resting').right, 'resting');
  assert.equal(topLine(page(), 'lost').left, 'Bifrost 3 · USB · 96 kHz');
});

ok('the_eq_line_reads_mode_or_preset_and_gains', () => {
  const withTone = (tone, presets) => page({}, { tone, presets });
  assert.deepEqual(eqLine(withTone(toneOf(), { status: { name: 'Warm' } })),
    { lead: 'EQ', name: 'WARM', gains: ['+3.0', '+1.5', '0.0', '−1.0', '+2.0'] });
  assert.deepEqual(eqLine(withTone(toneOf(), { status: { name: null } })),
    { lead: '', name: 'EQ', gains: ['+3.0', '+1.5', '0.0', '−1.0', '+2.0'] });
  assert.deepEqual(eqLine(withTone(toneOf({ mode: 'loudness', loud: true, strength: '3' }))),
    { lead: '', name: 'LOUDNESS 3', gains: [] });
  assert.deepEqual(eqLine(withTone(toneOf({ mode: 'flat' }))), { lead: '', name: 'FLAT', gains: [] });
  assert.equal(eqLine(page()), null);
  assert.equal(eqLine({ shown: null, device: null }), null);
});

ok('the_volume_reads_its_db_and_muted_reads_minus_infinity', () => {
  const v = volumeOf(page());
  assert.deepEqual([v.figures, v.unit, v.muted, v.none], ['−23.5', 'dB', false, null]);
  assert.equal(v.aria, 'minus 23.5 dB');
  const real = volumeOf(page({}, { volume: { text: '\u221226.1 dB', unit: 'dB', muted: false } }));
  assert.deepEqual([real.figures, real.aria], ['\u221226.1', 'minus 26.1 dB']);
  const up = volumeOf(page({}, { volume: { text: '0.0 dB', unit: 'dB', muted: false } }));
  assert.deepEqual([up.figures, up.aria], ['0.0', '0.0 dB']);
  const m = volumeOf(page({}, { volume: { text: '-23.5 dB', unit: 'dB', muted: true } }));
  assert.deepEqual([m.figures, m.unit, m.muted], ['−∞', 'dB', true]);
  assert.match(m.aria, /muted/);
});

ok('an_amp_without_a_volume_says_who_sets_it', () => {
  const amp = {
    shown: AMP.url,
    fleet: { devices: [DAC, AMP], hops: [{ id: 'a', kind: 'amp', device: AMP.url, after: DAC.url, live: true }] },
    host: null,
    device: { name: 'Asgard 3', url: AMP.url, controls: [], volume: null },
  };
  const v = volumeOf(amp);
  assert.equal(v.figures, '');
  assert.equal(v.none, 'This amp has no volume of its own: the Bifrost 3’s volume is the one that sets the level.');
  const alone = volumeOf({ ...amp, fleet: { devices: [AMP], hops: [] } });
  assert.equal(alone.none, 'This amp has no volume of its own.');
  assert.equal(volumeOf({ shown: null, fleet: { devices: [], hops: [] }, device: null }).none, null);
});

ok('the_outrun_volume_has_no_style_attribute_and_prefixed_ids', () => {
  for (const v of [volumeOf(page()), volumeOf(page({}, { volume: { text: '-23.5 dB', unit: 'dB', muted: true } }))]) {
    const svg = outrunVolume(v);
    assert.ok(svg.startsWith('<svg'));
    assert.ok(!/\sstyle\s*=/i.test(svg));
    assert.ok(!/<(script|style)\b/i.test(svg));
    assert.ok(svg.includes('viewBox="0 0 640 170"'));
    assert.ok(svg.includes('role="img"') && svg.includes(`aria-label="${v.aria}"`));
    const ids = [...svg.matchAll(/\bid="([^"]+)"/g)].map((m) => m[1]);
    assert.deepEqual(ids.sort(), ['nd-chrome', 'nd-fill', 'nd-glow', 'nd-stars']);
    assert.ok(svg.includes("font-family=\"'Mr Dafoe'") && svg.includes('font-size="64"'));
  }
  const plain = outrunVolume(volumeOf(page()));
  assert.ok(plain.includes('>23.5<'), 'the digits are text');
  assert.ok(plain.includes("font-family=\"'Russo One'") && plain.includes('font-size="118"'));
  assert.ok(!plain.includes('−') && !plain.includes('∞'), 'the minus is a path, not a glyph');
  const muted = outrunVolume(volumeOf(page({}, { volume: { text: '-23.5 dB', unit: 'dB', muted: true } })));
  assert.ok(!muted.includes('−') && !muted.includes('∞') && !muted.includes('23.5'));
  assert.ok(muted.includes('<path'), 'the minus and the infinity are paths');
  assert.ok(muted.includes('>dB<'));
});

/** A fake clock, the timers on it, and a fake document and root. */
function rig(after = 60, extra = {}) {
  let t = 0;
  const timers = [];
  const listeners = {};
  const classes = new Set();
  const doc = {
    addEventListener: (type, fn) => { (listeners[type] ??= []).push(fn); },
    removeEventListener: (type, fn) => { listeners[type] = (listeners[type] ?? []).filter((f) => f !== fn); },
  };
  const root = { classList: { add: (c) => classes.add(c), remove: (c) => classes.delete(c), contains: (c) => classes.has(c) } };
  const schedule = (fn, ms) => {
    const timer = { at: t + ms, fn };
    timers.push(timer);
    return () => { timers.splice(timers.indexOf(timer), 1); };
  };
  const fade = createFade({ after, root, doc, now: () => t, schedule, ...extra });
  return {
    fade,
    classes,
    listeners,
    input: (type) => (listeners[type] ?? []).forEach((fn) => fn({ type })),
    advance(ms) {
      const end = t + ms;
      for (;;) {
        timers.sort((a, b) => a.at - b.at);
        if (!timers.length || timers[0].at > end) break;
        const next = timers.shift();
        t = Math.max(t, next.at);
        next.fn();
      }
      t = end;
    },
    pending: () => timers.length,
  };
}

ok('the_fade_waits_15_seconds_unless_the_viewer_kept_another_time', () => {
  const prefs = (v) => ({ get: (_, fallback) => (v === undefined ? fallback : v) });
  assert.equal(fadeAfter(prefs()), 15);
  assert.equal(fadeAfter(undefined), 15);
  assert.equal(fadeAfter(prefs(40)), 40, 'a kept time stays');
  assert.equal(fadeAfter(prefs('x')), 15);
  assert.equal(fadeAfter(prefs(0)), 1);
  assert.equal(fadeAfter(prefs(9999)), 600);
  const r = rig(fadeAfter(prefs()));
  r.advance(14000);
  assert.ok(!r.classes.has('nd-faded'));
  r.advance(1500);
  assert.ok(r.classes.has('nd-faded'));
});

ok('the_fade_tells_its_watchers_when_it_fades_and_what_woke_it', () => {
  const r = rig(5);
  const seen = [];
  r.fade.watch((kind, event) => seen.push([kind, event?.type ?? null, r.fade.faded()]));
  r.advance(5500);
  r.input('pointerdown');
  assert.deepEqual(seen, [['fade', null, true], ['wake', 'pointerdown', false]]);
});

ok('the_fade_waits_its_time_and_any_input_wakes_it', () => {
  const r = rig(60);
  r.advance(59000);
  assert.ok(!r.classes.has('nd-faded'));
  r.advance(1500);
  assert.ok(r.classes.has('nd-faded'));
  r.input('pointermove');
  assert.ok(!r.classes.has('nd-faded'));
  r.advance(59000);
  assert.ok(!r.classes.has('nd-faded'));
  r.advance(1500);
  assert.ok(r.classes.has('nd-faded'));
  for (const type of ['pointerdown', 'keydown', 'wheel', 'touchstart', 'focusin']) {
    r.input(type);
    assert.ok(!r.classes.has('nd-faded'), type);
    r.advance(61000);
    assert.ok(r.classes.has('nd-faded'), type);
  }
  // a move now and then keeps it up: the idle time counts from the last input
  r.input('keydown');
  for (let i = 0; i < 5; i++) { r.advance(40000); r.input('pointermove'); }
  assert.ok(!r.classes.has('nd-faded'));
  r.fade.wake();
  r.fade.stop();
  assert.ok(!r.classes.has('nd-faded'));
  assert.ok(Object.values(r.listeners).every((list) => list.length === 0));
  assert.equal(r.pending(), 0);
});

ok('trouble_holds_the_fade_off', () => {
  const r = rig(2);
  r.fade.hold(true);
  r.advance(60000);
  assert.ok(!r.classes.has('nd-faded'));
  // trouble that arrives while faded brings the top back
  r.fade.hold(false);
  r.advance(2500);
  assert.ok(r.classes.has('nd-faded'));
  r.fade.hold(true);
  assert.ok(!r.classes.has('nd-faded'));
  r.advance(60000);
  assert.ok(!r.classes.has('nd-faded'));
  // it goes once the trouble does, after the idle time again
  r.fade.hold(false);
  r.advance(1000);
  assert.ok(!r.classes.has('nd-faded'));
  r.advance(1500);
  assert.ok(r.classes.has('nd-faded'));
  // a second hold(true) is not a second wake
  r.fade.hold(true);
  r.fade.hold(true);
  r.advance(10000);
  assert.ok(!r.classes.has('nd-faded'));
});

ok('a_volume_from_elsewhere_shows_the_volume_for_5_s_while_faded', () => {
  const r = rig(2);
  r.advance(2500);
  assert.ok(r.classes.has('nd-faded'));
  r.fade.showVolume(5000);
  assert.ok(r.classes.has('nd-faded') && r.classes.has('nd-vol-on'));
  r.advance(4900);
  assert.ok(r.classes.has('nd-vol-on'));
  r.advance(200);
  assert.ok(!r.classes.has('nd-vol-on') && r.classes.has('nd-faded'));
  // a second one while it shows restarts the 5 s
  r.fade.showVolume(5000);
  r.advance(3000);
  r.fade.showVolume(5000);
  r.advance(3000);
  assert.ok(r.classes.has('nd-vol-on'));
  r.advance(2100);
  assert.ok(!r.classes.has('nd-vol-on'));
  // input wakes everything and clears the volume's mark
  r.fade.showVolume(5000);
  r.input('pointermove');
  assert.ok(!r.classes.has('nd-faded') && !r.classes.has('nd-vol-on'));
  // not faded: nothing to show
  r.fade.showVolume(5000);
  assert.ok(!r.classes.has('nd-vol-on'));
});

ok('wire_hud_holds_the_fade_for_every_trouble_word', () => {
  let t = 0;
  const timers = [];
  const classes = new Set();
  const el = () => ({ textContent: '', className: '', innerHTML: '', hidden: false, classList: { add: (c) => classes.add(c), remove: (c) => classes.delete(c), contains: (c) => classes.has(c) } });
  const parts = { '.nd-left': el(), '.nd-right': el(), '.nd-vol': el(), '.nd-eq': el(), '#nightdrive': el() };
  const root = { querySelector: (sel) => parts[sel] };
  const doc = { addEventListener() {}, removeEventListener() {} };
  const core = {
    prefs: { get: () => 2 },
    model: { on: () => () => {}, shown: () => DAC.url, page: () => page() },
    computer: { switchOf: () => null, side: () => 'dac' },
  };
  const view = { state: 'open', asked: null };
  const schedule = (fn, ms) => {
    const timer = { at: t + ms, fn };
    timers.push(timer);
    return () => { const i = timers.indexOf(timer); if (i >= 0) timers.splice(i, 1); };
  };
  const advance = (ms) => {
    const end = t + ms;
    for (;;) {
      timers.sort((a, b) => a.at - b.at);
      if (!timers.length || timers[0].at > end) break;
      const next = timers.shift();
      t = Math.max(t, next.at);
      next.fn();
    }
    t = end;
  };
  const hud = wireHud({ root, core, view, now: () => t, schedule, doc });
  const faded = () => classes.has('nd-faded');
  const troubles = [
    ['waiting', 'waiting', page()],
    ['resting', 'resting', page()],
    ['lost', 'lost', page()],
    ['an unreachable card', 'open', page({}, { card: { kind: 'lost', title: 'Out of reach' } })],
    ['a fault card', 'open', page({}, { card: { kind: 'fault', title: 'Fault' } })],
  ];
  for (const [name, state, model] of troubles) {
    view.state = state;
    hud.draw(model);
    advance(60000);
    assert.ok(!faded(), `${name} holds the fade off`);
    // back to connected: the fade resumes after the full idle time
    view.state = 'open';
    hud.draw(page());
    advance(1900);
    assert.ok(!faded(), `${name}: not before the idle time`);
    advance(200);
    assert.ok(faded(), `${name}: after it`);
    hud.fade.wake();
  }
  // trouble that arrives while faded wakes it
  advance(3000);
  assert.ok(faded());
  view.state = 'lost';
  hud.draw(page());
  assert.ok(!faded());
  hud.stop();
});

// ---- the dash ----
const TRACK = {
  headline: 'Teardrop', artist: 'Massive Attack', album: 'Mezzanine', status: 'playing', length_us: 329_000_000,
  progress: { position_us: 102_000_000, rate_milli: 1000, read_at_ms: 1000 },
};
const player = (more = {}, track = TRACK) => ({ now_ms: 1000, buttons: true, track, ...more });
const dashPage = (deviceMore = {}, more = {}) => page(more, {
  volume: { text: '-23.5 dB', unit: 'dB', muted: false, raw: 40, mute: 'mute' },
  playing: { ...player(), path: pathOf(false) },
  ...deviceMore,
});
const acts = (html) => [...html.matchAll(/data-act="([^"]+)"/g)].map((m) => m[1]);
const buttonOf = (html, id) => html.match(new RegExp(`<button[^>]*id="${id}"[^>]*>`))?.[0] ?? '';

ok('the_dash_runs_minus_mute_plus_input_track_transport_device', () => {
  const html = drawDash(dashPage(), { state: 'open' }, 1000);
  // two options (Optical, USB) are chips
  assert.deepEqual(acts(html), ['volume-down', 'mute', 'volume-up', 'input', 'input', 'prev', 'play', 'next', 'sheet']);
  assert.ok(html.indexOf('nd-track') > html.indexOf('data-act="input"'), 'the track follows the input');
  assert.ok(html.indexOf('nd-track') < html.indexOf('data-act="prev"'), 'the transport follows the track');
  const text = (act) => html.match(new RegExp(`data-act="${act}"[^>]*>([^<]*)<`))?.[1];
  assert.equal(text('volume-down'), '−');
  assert.equal(text('mute'), 'MUTE');
  assert.equal(text('volume-up'), '+');
  assert.equal(text('sheet'), 'DEVICE ▴');
  assert.ok(/id="nd-vdown"/.test(html) && /id="nd-vup"/.test(html), 'the held keys have ids the hold follows');
  assert.ok(!/style=/.test(html), 'no style attribute');
  assert.ok(/aria-pressed="true"/.test(html.match(/data-value="USB"[^>]*>/)[0]), 'the current input is pressed');
  const muted = drawDash(dashPage({ volume: { text: '-23.5 dB', unit: 'dB', muted: true, raw: 40, mute: 'mute' } }), {}, 1000);
  assert.ok(/aria-pressed="true"/.test(buttonOf(muted, 'nd-mute')));
  // no volume on the device: no volume keys
  assert.deepEqual(acts(drawDash(dashPage({ volume: null }), {}, 1000)).slice(0, 2), ['input', 'input']);
});

ok('three_inputs_are_chips_and_more_are_one_chip_with_a_list', () => {
  const opts = (n, on) => Array.from({ length: n }, (_, i) => ({ label: `In ${i + 1}`, on: i === on, json: '{}' }));
  const withInputs = (n, on = 1) => dashPage({ controls: [{ key: 'input', kind: 'choice', label: 'Input', options: opts(n, on) }] });
  const three = drawDash(withInputs(3), {}, 1000);
  assert.equal(acts(three).filter((a) => a === 'input').length, 3);
  assert.ok(!/data-list/.test(three));
  const four = drawDash(withInputs(4), {}, 1000);
  assert.equal(acts(four).filter((a) => a === 'input').length, 1);
  assert.ok(/data-list="1"/.test(four) && /aria-haspopup="listbox"/.test(four) && /aria-expanded="false"/.test(four));
  assert.ok(four.includes('In 2 ▾'), 'the chip names the current input');
  assert.equal(acts(drawDash(dashPage({ controls: [] }), {}, 1000)).includes('input'), false);
});

ok('the_transport_is_svg_not_glyphs', () => {
  const html = drawDash(dashPage(), {}, 1000);
  for (const act of ['prev', 'play', 'next']) {
    const inner = html.match(new RegExp(`data-act="${act}"[^>]*>(.*?)</button>`))[1];
    assert.ok(/^<svg [^>]*><path d="[^"]+"\/><\/svg>$/.test(inner), `${act} is one svg with a path: ${inner}`);
    assert.ok(!/[\u23ee\u23ed\u23f8\u25b6\u23f5\u23f4]|&#/.test(inner), `${act} has no glyph`);
  }
  const names = [...html.matchAll(/data-button="([^"]+)"/g)].map((m) => m[1]);
  assert.deepEqual(names, ['previous', 'pause', 'next']);
  const paused = drawDash(dashPage({ playing: { ...player({}, { ...TRACK, status: 'paused' }), path: pathOf(false) } }), {}, 1000);
  assert.deepEqual([...paused.matchAll(/data-button="([^"]+)"/g)].map((m) => m[1]), ['previous', 'play', 'next']);
  assert.ok(/aria-label="Play"/.test(paused) && /aria-label="Pause"/.test(html));
});

ok('a_guests_dash_is_drawn_inert', () => {
  const guest = dashPage({}, { viewer: { guest: true } });
  assert.equal(inertOf(guest), true);
  const html = drawDash(guest, {}, 1000);
  for (const id of ['nd-vdown', 'nd-mute', 'nd-vup', 'nd-tr-0']) {
    const b = buttonOf(html, id);
    assert.ok(/aria-disabled="true"/.test(b) && /class="[^"]*quiet/.test(b), `${id} is inert: ${b}`);
  }
  assert.ok(/data-value="USB"[^>]*aria-disabled="true"/.test(html), 'the chips are inert');
  assert.ok(!/id="nd-sheet"[^>]*aria-disabled/.test(html), 'the sheet key still opens');
  assert.equal(inertOf(dashPage({}, { viewer: { writes: false } })), true);
  assert.equal(inertOf(dashPage()), false);
  // a guest is not given the player's buttons
  const none = drawDash(dashPage({ playing: { ...player({ buttons: false }), path: pathOf(false) } }, { viewer: { guest: true } }), {}, 1000);
  assert.ok(!/data-act="(prev|play|next)"/.test(none));
});

ok('the_track_reads_title_artist_album_and_time', () => {
  const html = drawDash(dashPage(), {}, 1000);
  assert.ok(html.includes('<b class="nd-title">Teardrop</b>'));
  assert.ok(html.includes('Massive Attack · Mezzanine · <span class="nd-time">1:42 / 5:29</span>'), html);
  assert.ok(/<span class="nd-prog"><i data-at="0\.31003"><\/i><\/span>/.test(html), 'the fill is the elapsed share');
  // a track with no album or artist leaves them out; no progress: no time or bar fill words
  const bare = drawDash(dashPage({ playing: { ...player({}, { headline: 'Song', status: 'playing' }) } }), {}, 1000);
  assert.ok(bare.includes('<small class="nd-sub"></small>'));
  assert.ok(!bare.includes('nd-time'));
  // it plays on: 2 s later the time moves
  const els = { fill: { style: {} }, time: { textContent: '' } };
  const root = { querySelector: (sel) => (sel === '.nd-prog i' ? els.fill : els.time) };
  moveProgress(root, dashPage(), 3000, 1000);
  assert.equal(els.time.textContent, '1:44 / 5:29');
  assert.equal(els.fill.style.width, '31.61%');
  // a paused track does not move
  els.time.textContent = 'same';
  const paused = dashPage({ playing: { ...player({}, { ...TRACK, status: 'paused' }) } });
  moveProgress(root, paused, 9000, 1000);
  assert.equal(els.time.textContent, 'same');
  // nothing playing
  const idle = drawDash(dashPage({ playing: { ...player({}, null) } }), {}, 1000);
  assert.ok(idle.includes('Nothing playing'));
});

ok('the_cover_sits_between_the_input_and_the_title_and_a_track_with_none_has_no_box', () => {
  const withCover = { ...TRACK, cover: 'ab12cd34ef56ab12' };
  const html = drawDash(dashPage({ playing: { ...player({}, withCover), path: pathOf(false) } }), {}, 1000);
  const img = html.match(/<span class="nd-cover" aria-hidden="true"><img src="([^"]*)" alt="" width="30" height="30"><\/span>/);
  assert.ok(img, 'a cover box with an empty alt, hidden from the reader');
  assert.equal(img[1], 'api/v1/host/cover?v=ab12cd34ef56ab12', 'a same-origin relative address with the cover\'s version');
  assert.ok(html.indexOf('data-act="input"') < html.indexOf('nd-cover'), 'after the input');
  assert.ok(html.indexOf('nd-cover') < html.indexOf('class="nd-title"'), 'before the title');
  assert.ok(html.indexOf('class="nd-title"') < html.indexOf('data-act="prev"'), 'the transport after the track');
  assert.ok(!html.includes('style='), 'no style attribute');
  // another track is another address; none, null or a missing field leave no box at all
  const other = drawDash(dashPage({ playing: { ...player({}, { ...withCover, cover: 'ffff0000ffff0000' }) } }), {}, 1000);
  assert.ok(other.includes('cover?v=ffff0000ffff0000') && !other.includes('ab12cd34ef56ab12'));
  for (const bare of [{ ...TRACK, cover: null }, TRACK, null]) {
    const none = drawDash(dashPage({ playing: { ...player({}, bare) } }), {}, 1000);
    assert.ok(!none.includes('nd-cover') && !none.includes('<img'), 'no box, no picture');
  }
});

ok('a_negative_gain_with_the_models_minus_reads_as_a_number_not_a_dash', () => {
  // the model writes a negative gain with U+2212; a hyphen is read too
  assert.equal(gainWords('\u22124.6 dB'), '\u22124.6');
  assert.equal(gainWords('-4.6 dB'), '\u22124.6');
  assert.equal(gainWords('+6.0 dB'), '+6.0');
  assert.equal(gainWords('0.0 dB'), '0.0');
  assert.equal(gainWords('\u22120.0 dB'), '0.0');
  assert.equal(gainWords('\u2013'), '\u2014', 'an unread band stays a dash');
});

ok('an_amp_has_power_and_readouts_where_the_input_would_be', () => {
  const amp = {
    ...page({ shown: AMP.url }),
    device: {
      name: 'Asgard 3', url: AMP.url, controls: [],
      power: { key: 'power', on: true, word: 'On', json: '{}', toggles: [], readouts: [{ key: 'temp', label: 'Temperature', text: '41 °C' }] },
    },
  };
  const html = drawDash(amp, {}, 1000);
  assert.deepEqual(acts(html), ['power', 'sheet']);
  assert.ok(html.includes('Temperature') && html.includes('41 °C'));
});

/** The cursor's rest on a fake clock: events on a fake document, the scene or something else as the target. */
function resting(extra = {}) {
  const timers = [];
  let t = 0;
  const listeners = {};
  const classes = new Set();
  const doc = {
    addEventListener: (type, fn) => { (listeners[type] ??= []).push(fn); },
    removeEventListener: (type, fn) => { listeners[type] = (listeners[type] ?? []).filter((f) => f !== fn); },
  };
  const root = { classList: { add: (c) => classes.add(c), remove: (c) => classes.delete(c), contains: (c) => classes.has(c) } };
  const schedule = (fn, ms) => {
    const timer = { at: t + ms, fn };
    timers.push(timer);
    return () => { timers.splice(timers.indexOf(timer), 1); };
  };
  const rest = createRest({
    root, doc, schedule, keepOut: () => [{ getBoundingClientRect: () => ({ left: 100, right: 200, top: 10, bottom: 30 }) }], ...extra,
  });
  const scene = { closest: (q) => (q === '.nd-scene' ? {} : null) };
  const dash = { closest: () => null };
  return {
    rest,
    classes,
    listeners,
    scene,
    dash,
    input(type, target, more = {}) {
      (listeners[type] ?? []).forEach((fn) => fn({ type, target, buttons: 0, pointerType: 'mouse', ...more }));
    },
    advance(ms) {
      const end = t + ms;
      for (;;) {
        timers.sort((a, b) => a.at - b.at);
        if (!timers.length || timers[0].at > end) break;
        const next = timers.shift();
        t = Math.max(t, next.at);
        next.fn();
      }
      t = end;
    },
  };
}

ok('the_cursor_rests_after_2_s_still_over_the_scene_and_a_move_brings_it_back', () => {
  const r = resting();
  r.input('pointermove', r.scene);
  r.advance(REST_MS - 1);
  assert.equal(r.classes.has('nd-rest'), false, 'not before 2 s');
  r.advance(2);
  assert.equal(r.classes.has('nd-rest'), true);
  r.input('pointermove', r.scene);
  assert.equal(r.classes.has('nd-rest'), false, 'the next move brings it back');
  r.advance(REST_MS + 1);
  assert.equal(r.classes.has('nd-rest'), true, 'and it rests again');
  assert.equal(REST_MS, 2000);
});

ok('a_move_restarts_the_2_s', () => {
  const r = resting();
  r.input('pointermove', r.scene);
  r.advance(1500);
  r.input('pointermove', r.scene);
  r.advance(1500);
  assert.equal(r.classes.has('nd-rest'), false);
  r.advance(600);
  assert.equal(r.classes.has('nd-rest'), true);
});

ok('the_cursor_stays_over_the_dash_and_the_menu', () => {
  const r = resting();
  r.input('pointermove', r.dash);
  r.advance(10000);
  assert.equal(r.classes.has('nd-rest'), false);
});

ok('the_cursor_stays_over_the_huds_words_though_they_are_click_through', () => {
  const r = resting();
  r.input('pointermove', r.scene, { clientX: 150, clientY: 20 });
  r.advance(10000);
  assert.equal(r.classes.has('nd-rest'), false, 'inside the box of a HUD word');
  r.input('pointermove', r.scene, { clientX: 150, clientY: 60 });
  r.advance(REST_MS + 1);
  assert.equal(r.classes.has('nd-rest'), true, 'just below it, the scene');
});

ok('the_cursor_stays_while_a_button_is_held_and_rests_2_s_after_the_release', () => {
  const r = resting();
  r.input('pointerdown', r.scene, { buttons: 1 });
  r.advance(10000);
  assert.equal(r.classes.has('nd-rest'), false, 'a drag on the mountains keeps it');
  r.input('pointerup', r.scene, { buttons: 0 });
  r.advance(REST_MS - 1);
  assert.equal(r.classes.has('nd-rest'), false);
  r.advance(2);
  assert.equal(r.classes.has('nd-rest'), true);
});

ok('a_finger_has_no_cursor_to_hide', () => {
  const coarse = resting({ fine: false });
  assert.deepEqual(coarse.listeners, {});
  const touch = resting();
  touch.input('pointerdown', touch.scene, { pointerType: 'touch', buttons: 1 });
  touch.input('pointerup', touch.scene, { pointerType: 'touch', buttons: 0 });
  touch.advance(10000);
  assert.equal(touch.classes.has('nd-rest'), false);
});

ok('stop_brings_the_cursor_back_and_lets_the_document_go', () => {
  const r = resting();
  r.input('pointermove', r.scene);
  r.advance(REST_MS + 1);
  assert.equal(r.classes.has('nd-rest'), true);
  r.rest.stop();
  assert.equal(r.classes.has('nd-rest'), false);
  assert.deepEqual(Object.values(r.listeners).flat(), []);
});

// ---- the computer's side ----
const buttonsOf = (html) => [...html.matchAll(/<button[^>]*data-act="side"[^>]*>([^<]*)<\/button>/g)];

ok('two_keys_name_the_device_and_this_computer', () => {
  const { core } = computerRig({ side: 'dac' });
  let comp = computerView(core);
  const html = switchKeys(comp.sw);
  const keys = buttonsOf(html);
  assert.deepEqual(keys.map((m) => m[1]), ['Gungnir 2', 'This computer']);
  assert.ok(/id="nd-side-dac"[^>]*aria-pressed="true"/.test(html) && /id="nd-side-computer"[^>]*aria-pressed="false"/.test(html));
  core.computer.flip('computer');
  comp = computerView(core);
  const flipped = switchKeys(comp.sw);
  assert.ok(/id="nd-side-computer"[^>]*aria-pressed="true"/.test(flipped) && /id="nd-side-dac"[^>]*aria-pressed="false"/.test(flipped));
  // a guest may flip: the keys are never drawn inert
  const guest = computerView(computerRig({ viewer: GUEST }).core);
  assert.ok(!/quiet|aria-disabled/.test(switchKeys(guest.sw)));
  // where the computer cannot be used the reason is in the switch's own place and the device's side is shown
  const away = computerView(computerRig({ computer: modelOf({ unavailable: 'pipewire is not running' }) }).core);
  assert.equal(away.side, 'dac');
  assert.ok(/<span class="nd-why" role="status">computer controls unavailable: pipewire is not running<\/span>/.test(switchKeys(away.sw)));
  assert.equal(switchKeys(null), '');
  // no computer in the model, no switch
  assert.equal(computerView(computerRig({ computer: null }).core), null);
  // a desktop draws them by the menu button, not in the dash
  const page = computerRig().core.model.page();
  assert.equal(buttonsOf(drawDash(page, { phone: () => false }, 1000, comp)).length, 0);
});

ok('the_phone_dash_has_no_switch_and_the_switch_is_drawn_for_the_scene', () => {
  const { core } = computerRig({ side: 'dac' });
  const comp = computerView(core);
  const page = core.model.page();
  const html = drawDash(page, { phone: () => true }, 1000, comp);
  assert.ok(!html.includes('data-act="side"') && !html.includes('nd-sw'), 'the dash is its own rows');
  assert.ok(html.includes('<span class="nd-brk"'), 'and ends its first row where the stylesheet wants one');
  assert.deepEqual(buttonsOf(switchKeys(comp.sw)).map((m) => m[1]), ['Gungnir 2', 'This computer']);
  // no computer, no keys
  assert.equal(switchKeys(null), '');
});

ok('the_figures_read_percent', () => {
  const { core } = computerRig();
  const comp = computerView(core);
  const page = core.model.page();
  const v = volumeOf(page, comp);
  assert.deepEqual([v.figures, v.unit, v.aria, v.muted], ['42', '%', 'Volume 42 %', false]);
  const svg = outrunVolume(v);
  assert.ok(svg.includes('>%</text>') && svg.includes('>42</text>'), 'the figure and the unit are drawn');
  // the DAC's own figure stays in dB
  assert.equal(volumeOf(page, computerView(computerRig({ side: 'dac' }).core)).unit, 'dB');
  // muted keeps the number, dimmed
  const muted = volumeOf(computerRig({ computer: modelOf({ volume: { pct: 42, muted: true } }) }).core.model.page(),
    computerView(computerRig({ computer: modelOf({ volume: { pct: 42, muted: true } }) }).core));
  assert.deepEqual([muted.figures, muted.muted, muted.dim], ['42', true, true]);
  assert.ok(outrunVolume(muted).includes('nd-dim'));
  // the first paint is a dash, never a live 0 %
  const unread = computerRig({ computer: modelOf({ volume: null }) });
  const first = volumeOf(unread.core.model.page(), computerView(unread.core));
  assert.deepEqual([first.figures, first.unit], ['—', '']);
  assert.ok(!outrunVolume(first).includes('>0</text>') && !outrunVolume(first).includes('>%</text>'));
  // the dash's keys step the computer's volume and mute it; unread they are drawn quiet
  const keys = drawDash(page, { phone: () => false }, 1000, comp);
  assert.deepEqual(acts(keys).slice(0, 3), ['volume-down', 'mute', 'volume-up']);
  assert.ok(!/quiet/.test(buttonOf(keys, 'nd-vdown')));
  const quiet = drawDash(unread.core.model.page(), { phone: () => false }, 1000, computerView(unread.core));
  assert.ok(/quiet/.test(buttonOf(quiet, 'nd-vdown')) && /quiet/.test(buttonOf(quiet, 'nd-mute')));
  const mutedKeys = drawDash(page, {}, 1000, computerView(computerRig({ computer: modelOf({ volume: { pct: 42, muted: true } }) }).core));
  assert.ok(/id="nd-mute"[^>]*aria-pressed="true"/.test(mutedKeys));
});

ok('the_eq_line_reads_off_or_eq_and_the_preset', () => {
  const line = (computer, side = 'computer') => {
    const { core } = computerRig({ computer, side });
    return eqLine(core.model.page(), computerView(core));
  };
  const eq = (over) => modelOf({ eq: { ...modelOf().eq, ...over } });
  assert.deepEqual(line(eq({})), { lead: '', name: 'EQ', gains: [] });
  assert.deepEqual(line(eq({ preset: 'Warm' })), { lead: 'EQ', name: 'WARM', gains: [] });
  assert.deepEqual(line(eq({ on: false })), { lead: '', name: 'OFF', gains: [] });
  // no gains list: the line is the word and the preset
  assert.equal(eqMarkup(line(eq({ preset: 'Warm' }))), 'EQ <b>WARM</b>');
  // a state only a file says is not shown as confirmed
  assert.deepEqual(line(eq({ running_unknown: true })).gains, ['unconfirmed']);
  // no EQ model, no line; the DAC side keeps the device's own
  assert.equal(line(modelOf({ eq: null })), null);
  const dacSide = computerRig({ side: 'dac', device: { ...RIG_DAC, tone: toneOf() } });
  assert.deepEqual(eqLine(dacSide.core.model.page(), computerView(dacSide.core)).gains.length, 5);
});

console.log(`nightdrive-hud.mjs: ${n} passed`);
