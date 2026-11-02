/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for the theme Ember's drawing of the page's parts (draw/parts.js), its screen
// (draw/screen.js) and its shell (draw/shell.js): each drawn from the page's JSON model, held to
// the golden copy of the DOM the server's askama templates drew for the same models,
// fixtures/ember-parts.json (frozen: no test writes it), compared as the browser would
// read the two (html-same.mjs). The harness checks its own comparison first: it must
// see a changed attribute, a missing space between tags and a number off by more than its
// tolerance, and pass reordered attributes and the attributes htmx reads. Run: node ember-parts.mjs
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { PARTS } from '../../../bins/svolderd/web/themes/ember/draw/parts.js';
import { screen } from '../../../bins/svolderd/web/themes/ember/draw/screen.js';
import { bodyClass, shell } from '../../../bins/svolderd/web/themes/ember/draw/shell.js';
import { DEFAULT_ORDER, resolveOrder } from '../../../bins/svolderd/web/themes/ember/order.js';
import {
  arch, RED_FROM, SEGMENTS, SWEEP,
} from '../../../bins/svolderd/web/themes/ember/draw/meter-arch.js';
import {
  angle, arc, LABEL_GAP, labels, point, spoke, SWEEP as DIAL_SWEEP, TICKS, ticks,
} from '../../../bins/svolderd/web/themes/ember/draw/dial.js';
import { curve } from '../../../bins/svolderd/web/themes/ember/draw/curve.js';
import { xOf, yOf } from '../../../bins/svolderd/web/core/tone-curve.js';
import { attr, esc } from '../../../bins/svolderd/web/themes/ember/draw/html.js';
import { sameHtml } from './html-same.mjs';

let n = 0;
let failed = 0;
const ok = (name, fn) => {
  n++;
  try {
    fn();
    console.log('ok', name);
  } catch (e) {
    failed++;
    console.log('FAIL', name, '\n    ', String(e.message).split('\n').slice(0, 4).join('\n     '));
  }
};
const differs = (ours, theirs) => {
  const result = sameHtml(ours, theirs);
  assert.equal(result.same, false, 'the comparison saw no difference');
  return result.at;
};
const same = (ours, theirs) => {
  const result = sameHtml(ours, theirs);
  assert.equal(result.same, true, result.at);
};

ok('the_comparison_catches_a_changed_attribute', () => {
  assert.match(
    differs('<i class="a" data-key="k"></i>', '<i class="b" data-key="k"></i>'),
    /class/,
  );
  assert.match(differs('<i class="a"></i>', '<i class="a" disabled></i>'), /disabled/);
  assert.match(differs('<b>a</b>', '<i>a</i>'), /token 0/);
  assert.match(differs('<b>a</b>', '<b>b</b>'), /text/);
  differs('<b>a</b>', '<b>a</b><b></b>');
  differs('<input data-json="{&#34;v&#34;:1}">', '<input data-json="{&#34;v&#34;:2}">');
});

ok('the_comparison_catches_a_missing_space_between_tags', () => {
  differs('<a>1</a> <b>2</b>', '<a>1</a><b>2</b>');
  differs('<a>1</a><b>2</b>', '<a>1</a> <b>2</b>');
  differs('<p>one two</p>', '<p>onetwo</p>');
  // a no-break space is a character of the text, not whitespace to collapse
  differs('<p>one\u00a0two</p>', '<p>one two</p>');
  differs('<a>1</a>\u00a0<b>2</b>', '<a>1</a> <b>2</b>');
  same('<a>1</a>\n  \n<b>2</b>', '<a>1</a> <b>2</b>');
});

ok('the_comparison_catches_a_number_off_by_0_02', () => {
  differs('<line x1="1.00" y1="2.00"/>', '<line x1="1.02" y1="2.00"/>');
  differs('<line x2="50.00%"/>', '<line x2="50.02%"/>');
  differs('<path d="M 1.00 2.00 L 3.00 4.00"/>', '<path d="M 1.00 2.00 L 3.02 4.00"/>');
  same('<line x1="1.00" y1="2.00"/>', '<line x1="1.005" y1="1.995"/>');
  // a number elsewhere is not one the tolerance covers
  differs('<i data-value="1.00"></i>', '<i data-value="1.005"></i>');
  // the shape around the numbers must match
  differs('<line x2="50.00%"/>', '<line x2="50.00"/>');
});

ok('the_comparison_reads_a_data_block_as_json', () => {
  const block = (json) => `<script type="application/json" id="d">${json}</script>`;
  same(block('{"a":1.0,"b":[2,3]}'), block('{"b":[2,3],"a":1}'));
  same(block('{"t":"\\u003cb>&amp;"}'), block('{"t":"<b>&amp;"}'));
  differs(block('{"a":1}'), block('{"a":2}'));
  differs(block('{"a":1}'), block('{"a":1,"b":null}'));
  differs(block('{}'), '<script id="d">{}</script>');
  // a script that is not a data block is text
  differs('<script>1.0</script>', '<script>1</script>');
});

ok('the_comparison_passes_reordered_attributes_and_hx_post', () => {
  same(
    '<button type="button" class="a" data-key="k" hx-post="api/x" hx-ext="json" hx-target="#t"'
      + ' hx-swap="outerHTML" hx-trigger="change" hx-get="g" hx-push-url="true"'
      + ' disabled>Go</button>',
    '<button disabled data-key="k"   class="a" type="button">Go</button>',
  );
  same('<i title="a &amp; b &lt; &quot;c&quot; &#x27;d&#39;"></i>',
    '<i title=\'a &amp; b &lt; "c" &#39;d&#39;\'></i>');
  same('<p>\n  a   b\n</p>\n', '<p> a b </p>');
  same('<i data-json="{&#34;a&#34;:[1,2]}"></i>', '<i data-json=\'{"a": [1, 2]}\'></i>');
  // hx-swap-oob and hx-preserve are read by the page and stay
  differs('<i hx-swap-oob="true"></i>', '<i></i>');
  differs('<i hx-preserve="true"></i>', '<i></i>');
});

ok('html_escapes_what_a_model_carries', () => {
  assert.equal(esc('a & b < c > "d" \'e\''), 'a &amp; b &lt; c &gt; &quot;d&quot; &#39;e&#39;');
  assert.equal(esc(7), '7');
  assert.equal(attr('class', 'a "b"'), ' class="a &quot;b&quot;"');
  assert.equal(attr('disabled', true), ' disabled');
  assert.equal(attr('disabled', false), '');
  assert.equal(attr('data-raw', null), '');
  assert.equal(attr('data-raw', ''), ' data-raw=""');
});

ok('the_arch_has_its_segments_in_their_classes', () => {
  const segments = arch({ fraction: 0.5, class_a: 0.25 });
  assert.equal(segments.length, SEGMENTS);
  assert.equal(SWEEP, 220);
  assert.equal(RED_FROM, 0.93);
  assert.equal(segments.filter((s) => s.on).length, 16);
  assert.deepEqual(segments.slice(0, 8).map((s) => s.class), Array(8).fill('brass'));
  assert.deepEqual(segments.slice(-2).map((s) => s.class), ['red', 'red']);
  assert.equal(segments[8].class, 'ramp');
  assert.deepEqual(segments[0], {
    x1: '18.53', y1: '124.23', x2: '6.07', y2: '127.94', class: 'brass', on: true,
  });
  assert.equal(arch({ fraction: null, class_a: 0.25 }).some((s) => s.on), false);
  assert.equal(arch({ fraction: 1, class_a: 0 }).every((s) => s.on), true);
});

const fixture = JSON.parse(
  readFileSync(new URL('./fixtures/ember-parts.json', import.meta.url), 'utf8'),
);
const PART_NAMES = [
  'link', 'card', 'controls', 'power', 'meters', 'volume', 'tone', 'bands', 'presets',
  'device-presets', 'facts', 'playing',
];
// The wordmark as the page holds it for the theme: the SVG, with the licence comments of the file
// it is kept in left out (the comparison reads tags, and the browser shows no comment).
const uncommented = (html) => html.replace(/<!--[\s\S]*?-->/g, '');
const WORDMARK = uncommented(readFileSync(
  new URL('../../../bins/svolderd/web/wordmark.svg', import.meta.url),
  'utf8',
));
/** The body of the document the templates render: its class and what it holds. */
const bodyOf = (document) => {
  const body = /<body(?: class="([^"]*)")?>([\s\S]*)<\/body>/.exec(document);
  return { class: body[1] ?? '', inner: uncommented(body[2]) };
};
const deviceOf = (name) => fixture.cases.find((c) => c.name === name).page.device;
const partOf = (name, part) => PARTS[part](deviceOf(name), fixture.cases[0].page, { oob: true });

ok('the_dial_maps_a_step_to_its_angle_and_place', () => {
  assert.equal(DIAL_SWEEP, 270);
  assert.equal(TICKS, 49);
  assert.equal(LABEL_GAP, 10);
  assert.equal(angle(0, 0, 127), -135);
  assert.equal(angle(127, 0, 127), 135);
  assert.equal(angle(5, 5, 5), -135);
  assert.deepEqual(point(0, 80), ['100.00', '20.00']);
  assert.deepEqual(point(90, 80), ['180.00', '100.00']);
  assert.deepEqual(spoke(0, 86, 98), ['100.00', '14.00', '100.00', '2.00']);
  assert.equal(arc(-135, 135, 80), 'M43.43 156.57A80 80 0 1 1 156.57 156.57');
  assert.equal(arc(-135, 0, 80), 'M43.43 156.57A80 80 0 0 1 100.00 20.00');
});

ok('a_label_too_close_to_another_is_dropped', () => {
  const stand = 9 / (DIAL_SWEEP / 100);
  const at = (marks) => labels({ lo: 0, hi: 100, risk: null, marks }).map((l) => l.text);
  // 9 degrees from the top's label: the second goes, the first listed stays
  assert.deepEqual(at([
    { at: 0, text: '−∞' }, { at: 100, text: '−9' }, { at: 100 - stand, text: '−10' },
    { at: 50, text: '−20' },
  ]), ['−∞', '−20', '−9']);
  assert.deepEqual(at([
    { at: 100 - stand, text: '−10' }, { at: 100, text: '−9' },
  ]), ['−10']);
  // 11 degrees away both stay, in order round the dial
  assert.deepEqual(at([
    { at: 100, text: '−9' }, { at: 100 - 11 / (DIAL_SWEEP / 100), text: '−10' },
  ]), ['−10', '−9']);
});

ok('the_gungnir_dial_reads_its_labels_at_their_true_places', () => {
  const v = { ...deviceOf('dac').volume, risk: null, cap: null };
  const marks = labels(v);
  assert.deepEqual(marks.map((l) => l.text), ['−∞', '−60', '−40', '−20', '−10', '0']);
  assert.deepEqual(marks.map((l) => l.at), ['0.00', '16.10', '37.85', '75.14', '97.10', '127.00']);
  assert.ok(marks.every((l) => l.red === false));
  const [top] = [marks[5]];
  const [x1, y1] = spoke(angle(127, 0, 127), 101.5, 105);
  assert.deepEqual(top.tick.slice(0, 2), [x1, y1]);
  const dial = ticks(v);
  assert.equal(dial.length, TICKS);
  assert.deepEqual([dial[0].raw, dial[48].raw], [0, 127]);
  assert.ok(dial.filter((t) => t.raw <= 63).every((t) => t.class.startsWith('lit')));
  assert.ok(dial.filter((t) => t.raw > 63).every((t) => t.class === 'off'));
  assert.deepEqual(dial[0].class, 'lit r0');
  assert.deepEqual(Object.keys(dial[0]), ['raw', 'x1', 'y1', 'x2', 'y2', 'class']);
  // silence lights nothing, and a level in the risk zone is red
  assert.ok(ticks({ ...v, raw: 0 }).every((t) => t.class === 'off'));
  assert.equal(labels({ ...v, risk: 90 }).filter((l) => l.red).length, 2);
});

ok('the_risk_the_cap_and_a_climb_shape_the_knob', () => {
  const device = deviceOf('dac-climb-cap-risk');
  const { risk, cap } = device.volume;
  assert.ok(risk > 95 && risk <= 127, risk);
  const dial = ticks(device.volume);
  const classes = new Set(dial.map((t) => t.class));
  assert.ok(classes.has('off cap') && !classes.has('off zone'), 'the cap shadows the zone');
  assert.ok(dial.filter((t) => t.raw > cap).every((t) => t.class === 'off cap'));
  const html = partOf('dac-climb-cap-risk', 'volume');
  assert.match(html, /<line class="ttick" /);
  assert.match(html, /<line class="ctick" /);
  assert.match(html, /<path class="target" /);
  assert.match(html, /<path class="zone" /);
  assert.doesNotMatch(html, /note risk hot/);
  // a level past the risk step lights the zone red, turns its labels red and the note hot
  const hot = deviceOf('dac-risk-hot').volume;
  const lit = ticks(hot);
  assert.ok(lit.some((t) => t.class === 'lit red') && lit.some((t) => t.class === 'off zone'));
  assert.ok(lit.filter((t) => t.raw >= hot.risk)
    .every((t) => t.class === 'lit red' || t.raw > hot.raw));
  assert.ok(lit.filter((t) => t.raw < hot.risk).every((t) => !t.class.endsWith('red')));
  assert.deepEqual(labels(hot).filter((l) => l.red).map((l) => l.text), ['0']);
  assert.match(partOf('dac-risk-hot', 'volume'), /<p class="note risk hot">/);
  assert.match(partOf('dac-risk-hot', 'volume'), /<text class="lbl red"/);
  const calm = partOf('dac', 'volume');
  assert.doesNotMatch(calm, /ttick|ctick|class="target"/);
});

ok('the_points_sit_at_each_bands_own_gain', () => {
  // The summed curve is 12 dB high everywhere: a point is at its own gain, not on it.
  const markers = [
    { band: 1, hz: 60, db: 0 }, { band: 2, hz: 1000, db: 6 }, { band: 3, hz: 1100, db: 9 },
  ];
  const drawn = curve({
    mode: 'eq', response: Array(181).fill(12), markers, peak: null,
  });
  assert.deepEqual(drawn.markers, markers.map((m) => ({
    band: m.band, x: xOf(m.hz).toFixed(1), y: yOf(m.db).toFixed(1),
  })));
  assert.deepEqual(drawn.markers.map((m) => m.y), ['130.0', '70.0', '40.0']);
  // the model says the level: loudness and an unread mode put every point at 0 dB
  const flat = markers.map((m) => ({ ...m, db: 0 }));
  for (const mode of ['loudness', null]) {
    const at = curve({
      mode, response: null, markers: flat, peak: null,
    });
    assert.ok(at.markers.every((m) => m.y === '130.0'));
  }
});

ok('a_band_not_read_in_full_draws_no_point', () => {
  const tone = deviceOf('dac-band-unread').tone;
  assert.deepEqual(curve(tone).markers.map((m) => m.band), [1, 2, 4, 5]);
  const html = partOf('dac-band-unread', 'tone');
  assert.equal((html.match(/class="mk"/g) ?? []).length, 4);
  assert.doesNotMatch(html, /data-band="3" cx/);
  assert.equal((html.match(/class="band"/g) ?? []).length, 5);
});

ok('the_curve_draws_the_line_the_area_and_the_ghost_for_each_mode', () => {
  const flatY = '130.0';
  const eq = curve(deviceOf('dac').tone);
  assert.equal(eq.ghost, null);
  assert.ok(eq.area.startsWith(eq.line) && eq.area.endsWith('L1000.0 130.0L0.0 130.0Z'));
  assert.equal(eq.ys.split(',').length, 181);
  assert.deepEqual(eq.peak, { y: '97.0', words: 'peak +3.3 dB' });
  const flat = curve(deviceOf('dac-flat').tone);
  assert.ok(flat.ghost.startsWith('M0.0 '));
  assert.ok(flat.ys.split(',').every((y) => y === flatY));
  assert.equal(flat.peak, null);
  const loud = curve(deviceOf('dac-loudness').tone);
  assert.equal(loud.line, '');
  assert.equal(loud.area, '');
  assert.equal(loud.ys, '');
  assert.ok(loud.ghost.length > 0);
  const unread = curve(deviceOf('dac-unread').tone);
  assert.equal(unread.ghost, null);
  assert.ok(unread.ys.split(',').every((y) => y === flatY));
});

for (const c of fixture.cases) {
  const device = c.page.device;
  for (const part of PART_NAMES) {
    if (!(part in c.parts)) continue;
    ok(`${c.name}/${part}`, () => {
      const ours = PARTS[part](device, c.page, { oob: true });
      const result = sameHtml(ours, c.parts[part]);
      assert.equal(
        result.same,
        true,
        `${result.at}\n  ours:   ${ours.slice(0, 300)}\n  theirs: ${c.parts[part].slice(0, 300)}`,
      );
    });
  }
  // A page with no device to show draws its card on its own, as the screen holds it.
  if (!device) {
    ok(`${c.name}/card`, () => {
      const ours = PARTS.card(null, c.page, { oob: false });
      const result = sameHtml(ours, c.screen);
      assert.equal(result.same, true, result.at);
    });
  }
  // The screen holds every part in its first-paint form, in the order the layout sets.
  ok(`${c.name}/screen`, () => {
    const ours = screen(c.page);
    const result = sameHtml(ours, c.screen);
    assert.equal(
      result.same,
      true,
      `${result.at}\n  ours:   ${ours.slice(0, 300)}\n  theirs: ${c.screen.slice(0, 300)}`,
    );
  });
  ok(`${c.name}/shell`, () => {
    const ours = shell(c.page, WORDMARK);
    const theirs = bodyOf(c.shell);
    const result = sameHtml(ours, theirs.inner);
    assert.equal(
      result.same,
      true,
      `${result.at}\n  ours:   ${ours.slice(0, 300)}\n  theirs: ${theirs.inner.slice(0, 300)}`,
    );
    assert.equal(bodyClass(c.page), theirs.class);
  });
}

ok('the_visualizer_card_is_the_same_on_every_screen_that_has_one', () => {
  for (const c of fixture.cases.filter((x) => x.page.device?.playing)) {
    const card = /<div class="p-viz"[\s\S]*?<\/div>/.exec(c.screen);
    assert.ok(card, `${c.name} has no visualizer card`);
    same(PARTS.visualizer(c.page.device, c.page, { oob: false }), card[0]);
    assert.ok(screen(c.page).includes('class="p-viz"'));
  }
  for (const c of fixture.cases.filter((x) => x.page.device && !x.page.device.playing)) {
    assert.ok(!screen(c.page).includes('p-viz'), `${c.name} draws a visualizer`);
    assert.ok(!screen(c.page).includes('p-band'), `${c.name} draws a band`);
  }
});

ok('a_device_name_with_a_space_and_a_slash_makes_css_identifiers', () => {
  const c = fixture.cases.find((x) => x.name === 'two-devices-odd-name');
  const { dom, url, name } = c.page.device;
  assert.equal(name, 'Desk A/B');
  assert.match(dom, /^[0-9a-f]+$/);
  const html = screen(c.page);
  assert.ok(html.includes(`id="band-editor-${dom}"`));
  assert.ok(html.includes(`id="slider-${dom}-balance"`));
  assert.ok(html.includes(`data-device="${url}"`) && url.includes('%2F') && url.includes('%20'));
  const ids = [...(html + shell(c.page, WORDMARK)).matchAll(/ id="([^"]*)"/g)].map((m) => m[1]);
  assert.ok(ids.length > 10);
  for (const id of ids) assert.match(id, /^[A-Za-z][A-Za-z0-9_-]*$/, `id ${JSON.stringify(id)}`);
  // the switcher goes to the other device by its URL form
  assert.ok(shell(c.page, WORDMARK).includes(`href="./?device=${url}"`));
  assert.ok(shell(c.page, WORDMARK).includes('href="./?device=Aegir%202"'));
});

ok('the_strip_and_the_tone_steps_are_data_blocks_that_cannot_close_their_script', () => {
  const c = fixture.cases.find((x) => x.name === 'dac');
  const strip = {
    kind: 'dac', bars: [], marks: [{ t: 1, text: '</script><b>', tint: 'acc' }],
  };
  const html = screen({ ...c.page, strip });
  const block = /<script type="application\/json" id="strip">(.*?)<\/script>/s.exec(html)[1];
  assert.deepEqual(JSON.parse(block), strip);
  assert.ok(!block.includes('<'));
  const steps = /<script type="application\/json" id="tone-steps">(.*?)<\/script>/s.exec(html)[1];
  assert.deepEqual(JSON.parse(steps), c.page.device.tone_steps);
  assert.equal(/id="strip">(.*?)<\/script>/s.exec(screen(c.page))[1], '{}');
});

ok('the_default_order_is_the_layout_the_page_has_today', () => {
  assert.deepEqual(DEFAULT_ORDER, {
    dac: {
      band: ['playing', 'visualizer'],
      left: ['volume', 'controls', 'power'],
      right: ['tone', 'presets', 'device-presets'],
    },
    amp: { band: ['playing', 'visualizer'], left: ['meters'], right: ['power', 'controls'] },
  });
  assert.deepEqual(resolveOrder(null, 'dac'), DEFAULT_ORDER.dac);
  assert.deepEqual(resolveOrder(undefined, 'amp'), DEFAULT_ORDER.amp);
});

ok('a_kept_order_goes_first_and_what_it_misses_goes_back_to_its_zone', () => {
  const kept = {
    band: ['visualizer'],
    left: ['power', 'nonsense', 'volume', 'volume'],
    right: ['presets', 'tone', 'meters'],
  };
  assert.deepEqual(resolveOrder(kept, 'dac'), {
    band: ['visualizer', 'playing'],
    left: ['power', 'volume', 'controls'],
    right: ['presets', 'tone', 'device-presets'],
  });
  // an id the kept order puts in another zone stays where it was put
  const moved = resolveOrder({ left: ['tone'], right: [] }, 'dac');
  assert.deepEqual(moved.left, ['tone', 'volume', 'controls', 'power']);
  assert.deepEqual(moved.right, ['presets', 'device-presets']);
  assert.deepEqual(moved.band, ['playing', 'visualizer']);
  // a kept order that is not an object is no order
  assert.deepEqual(resolveOrder('x', 'amp'), DEFAULT_ORDER.amp);
  assert.deepEqual(resolveOrder({ band: 'x' }, 'amp'), DEFAULT_ORDER.amp);
});

/** The ids of the sections a screen draws, in the order it draws them, by zone. */
const zonesOf = (html) => {
  const ids = (chunk) => [...chunk.matchAll(/ id="part-([a-z-]+)"|class="(p-viz)"/g)]
    .map((m) => (m[2] === 'p-viz' ? 'visualizer' : m[1]))
    .filter((id) => !['facts', 'link', 'card', 'bands'].includes(id));
  const [head, ...cols] = html.split('<div class="col"');
  const band = head.split('<div class="p-band">')[1] ?? '';
  return { band: ids(band), left: ids(cols[0] ?? ''), right: ids(cols[1] ?? '') };
};

ok('a_screen_drawn_in_a_kept_order_holds_the_same_parts_in_it', () => {
  const c = fixture.cases.find((x) => x.name === 'playing');
  const order = { band: ['visualizer', 'playing'], left: ['power', 'volume'], right: ['tone'] };
  const html = screen(c.page, resolveOrder(order, 'dac'));
  const at = (id) => html.indexOf(`id="part-${id}"`);
  assert.ok(html.indexOf('class="p-viz"') < at('playing'));
  assert.ok(at('volume') < at('controls') && at('controls') < at('presets'));
  assert.deepEqual(zonesOf(html), {
    band: ['visualizer', 'playing'],
    left: ['volume', 'controls'],
    right: ['tone', 'presets'],
  });
  assert.ok(at('facts') > at('presets'));
});

ok('a_screen_in_an_arranged_order_is_marked_and_the_default_is_not', () => {
  const c = fixture.cases.find((x) => x.name === 'playing');
  const mark = (order) => /class="view dac( arranged)?/.exec(screen(c.page, order))[1] ?? '';
  assert.equal(mark(undefined), '');
  assert.equal(mark(resolveOrder(null, 'dac')), '');
  assert.equal(mark(resolveOrder({ band: ['playing', 'tone', 'visualizer'] }, 'dac')), ' arranged');
  // a section the screen does not draw may stand anywhere without the screen being arranged
  const apart = { band: ['playing', 'visualizer'], left: ['volume', 'controls'],
    right: ['tone', 'presets', 'device-presets', 'power'] };
  assert.equal(c.page.device.power, null);
  assert.equal(mark(apart), '');
  // a screen with no player has no band to hold Now playing and the visualizer, but may hold
  // another section in it
  const quiet = fixture.cases.find((x) => x.page.device && !x.page.device.playing
    && x.page.device.layout === 'dac' && x.page.device.tone);
  const html = screen(quiet.page, resolveOrder({ band: ['visualizer', 'tone'] }, 'dac'));
  assert.deepEqual(zonesOf(html).band, ['tone']);
  assert.match(html, /<div class="p-band"> <section id="part-tone"/);
  assert.doesNotMatch(screen(quiet.page), /p-band/);
});

ok('every_screen_draws_the_sections_of_its_default_order_in_order', () => {
  const drawn = (c) => {
    const { layout } = c.page.device;
    const has = {
      volume: c.page.device.volume, meters: c.page.device.meters, power: c.page.device.power,
      tone: c.page.device.tone, presets: c.page.device.presets,
      'device-presets': c.page.device.device_presets,
    };
    const keep = (id) => !(id in has) || has[id] != null;
    const zone = (ids) => ids.filter(keep);
    const order = DEFAULT_ORDER[layout];
    return {
      band: c.page.device.playing ? order.band : [],
      left: zone(order.left),
      right: zone(order.right),
    };
  };
  for (const name of ['dac', 'playing', 'lokimax', 'amp', 'amp-warming', 'five', 'phono']) {
    const c = fixture.cases.find((x) => x.name === name);
    assert.deepEqual(zonesOf(screen(c.page)), drawn(c), name);
  }
});

ok('an_order_that_names_what_ember_lacks_or_is_not_a_list_cannot_break_the_screen', () => {
  const c = fixture.cases.find((x) => x.name === 'playing');
  const odd = {
    band: ['nonsense', 'link', 'control', 'visualizer'],
    left: 'volume',
    right: ['tone', 7, null, 'bands', 'presets'],
  };
  const html = screen(c.page, odd);
  const found = zonesOf(html);
  assert.deepEqual(found, { band: ['visualizer'], left: [], right: ['tone', 'presets'] });
  assert.doesNotMatch(html, /part-link/);
  // a zone with nothing left to draw is an empty column, and an empty band is no band at all
  const bare = screen(c.page, { band: ['nonsense'], left: [], right: [] });
  assert.doesNotMatch(bare, /p-band|p-viz|part-playing/);
  assert.equal((bare.match(/<div class="col"/g) ?? []).length, 2);
  // an order with no zones at all draws the view and its fold-out
  const none = screen(c.page, {});
  assert.match(none, /class="view dac"/);
  assert.match(none, /id="part-facts"/);
});

ok('every_part_the_fixture_holds_of_these_has_a_case', () => {
  const held = new Set(fixture.cases.flatMap((c) => Object.keys(c.parts)));
  for (const part of PART_NAMES) assert.ok(held.has(part), `no case holds ${part}`);
  const names = fixture.cases.map((c) => c.name);
  assert.equal(new Set(names).size, names.length, 'a case name is used twice');
  assert.ok(names.length > 0);
});

// ---------------------------------------------------------------------------------------------
// The claims the Rust tests kept about the markup the page shows. Each is held to what Ember draws
// from the frozen fixture's models (the golden copy of the DOM the server's templates drew), under
// the name of the Rust test that holds the same claim of the model.
// ---------------------------------------------------------------------------------------------
const caseOf = (name) => fixture.cases.find((c) => c.name === name);
/** The part `part` of case `name` as Ember draws it, `patch` laid over the device's model. */
const drawn = (name, part, patch = {}) => {
  const c = caseOf(name);
  const html = PARTS[part]({ ...c.page.device, ...patch }, c.page, { oob: false });
  return html.replace(/\s+/g, ' ');
};
/** The screen of case `name` as Ember draws it. */
const screenOf = (name) => screen(caseOf(name).page).replace(/\s+/g, ' ');
/** The shell of case `name`, whitespace folded. */
const shellOf = (name) => shell(caseOf(name).page, WORDMARK).replace(/\s+/g, ' ');
const count = (html, text) => html.split(text).length - 1;

ok('the_knob_carries_each_steps_db', () => {
  const volume = drawn('dac', 'volume');
  assert.match(volume, /data-db="\[null,/);
  assert.equal(deviceOf('dac').volume.db.length, 128);
});

ok('the_controls_post_through_htmx_and_an_inert_one_posts_nothing', () => {
  // no part, screen or shell of any case carries an attribute htmx would post or swap from
  for (const c of fixture.cases) {
    const everything = [screen(c.page), shell(c.page, WORDMARK),
      ...PART_NAMES.filter((p) => c.page.device && c.parts[p]).map((p) => drawn(c.name, p))].join(' ');
    assert.doesNotMatch(everything, /hx-(post|get|trigger|target|ext|swap=|push-url)/, c.name);
  }
  // a press carries the JSON it posts
  const open = drawn('dac', 'controls');
  assert.ok(open.includes('data-json="{&quot;value&quot;:&quot;Optical&quot;}"'), open);
  assert.match(open, /data-key="phase" data-json="\{&quot;value&quot;:false\}"/);
  assert.doesNotMatch(open, /disabled/);
  // a list carries its values; a guest's list is disabled, and none of it posts
  const list = drawn('phono', 'controls');
  assert.match(list, /<select data-key="load_capacitance" name="value" aria-label="Load capacitance">/);
  const guest = drawn('phono-guest', 'controls');
  assert.match(guest, /<select [^>]* disabled aria-disabled="true">/);
  assert.doesNotMatch(guest, /data-json/);
  const dac = drawn('dac-guest', 'controls');
  assert.doesNotMatch(dac, /data-json/);
  assert.match(dac, /aria-disabled="true"/);
  // the power button posts the JSON it carries, and an amp's other switches live in the power part
  const power = drawn('amp', 'power');
  assert.match(power, /<button type="button" class="btn power" aria-pressed="true" data-key="power" data-json="\{&quot;value&quot;:false\}">/);
  assert.ok(count(power, 'role="switch"') >= 1, power);
  assert.equal(count(power, 'data-json='), 1 + count(power, 'role="switch"'));
  assert.doesNotMatch(drawn('amp', 'controls'), /role="switch"/);
});

ok('try_now_keeps_its_id_and_is_preserved_across_the_card_swap', () => {
  const card = drawn('lost', 'card');
  assert.ok(card.includes('id="try-now" hx-preserve="true" data-try'), card);
  assert.match(card, /<div class="countdown" data-secs="\d+">/);
});

ok('an_unread_volume_reads_a_dash_and_a_muted_one_says_so', () => {
  const unread = {
    ...deviceOf('dac').volume, raw: null, text: '–', readout: '–', unit: null, unread: true, risk: null, cap: null,
  };
  assert.ok(ticks(unread).every((t) => t.class === 'off'));
  const html = drawn('dac', 'volume', { volume: unread });
  assert.doesNotMatch(html, /class="handle"/);
  assert.match(html, /<span class="num">–<\/span>|class="num">–/);
  const muted = deviceOf('dac-muted').volume;
  assert.equal(muted.muted, true);
  assert.match(drawn('dac-muted', 'volume'), /data-muted="true"/);
  const silent = { ...deviceOf('dac').volume, raw: 0, risk: null };
  assert.ok(ticks(silent).every((t) => t.class === 'off'), 'silence lights nothing');
});

ok('a_dial_label_carries_the_step_it_stands_at', () => {
  const marks = labels({ ...deviceOf('dac').volume, risk: null });
  const at = marks.map((l) => l.at);
  assert.equal(at[0], '0.00');
  assert.equal(at.at(-1), '127.00');
  assert.ok(at.every((v, i) => i === 0 || Number(at[i - 1]) < Number(v)), at.join());
});

ok('the_bands_and_the_curve_read_the_tone_control', () => {
  const dac = deviceOf('dac');
  // the kept band editor's id is the device's own, so another device's late answer cannot move
  // this one's editor into a detached tree
  assert.ok(drawn('dac', 'bands').includes(`id="band-editor-${dac.dom}" hx-preserve`), drawn('dac', 'bands'));
  const note = (name) => /data-note="([^"]*)"/.exec(drawn(name, 'tone'))[1];
  assert.equal(note('dac'), '');
  assert.equal(
    note('dac-flat'),
    'Flat bypasses the bands; the faint line is what EQ would do. Step a band and the mode switches to EQ.',
  );
  assert.equal(
    note('dac-loudness'),
    'Loudness follows the volume: it adds more the lower you listen, and nothing at full volume.',
  );
  for (const name of ['dac', 'dac-flat', 'dac-loudness']) {
    assert.doesNotMatch(drawn(name, 'tone'), /gnote/);
  }
  assert.match(drawn('dac-flat', 'bands'), /class="bands faint"/);
});

ok('the_readings_carry_the_raw_bands_and_the_mode', () => {
  const html = drawn('dac', 'tone');
  assert.ok(html.includes('<figure class="eqg" data-readings="{&quot;bands&quot;:[['), html);
  const readings = /data-readings="([^"]*)"/.exec(html)[1].replaceAll('&quot;', '"');
  assert.deepEqual(JSON.parse(readings), deviceOf('dac').tone.readings);
});

ok('the_screen_carries_the_step_tables_where_the_tone_part_is', () => {
  const block = '<script type="application/json" id="tone-steps">';
  assert.equal(count(screenOf('dac'), block), 1);
  assert.ok(!screenOf('amp').includes('tone-steps'));
});

ok('a_guests_tone_part_is_marked_inert', () => {
  assert.ok(!drawn('dac', 'tone').includes('data-inert'));
  for (const name of ['dac-guest', 'dac-readonly']) {
    const html = drawn(name, 'tone');
    assert.ok(html.includes('<section id="part-tone"'), html);
    assert.ok(html.includes(' data-inert'), name);
  }
});

ok('loudness_steps_on_its_own_button_and_no_row_follows', () => {
  const loud = drawn('dac-loudness-strength', 'tone');
  assert.ok(loud.includes('aria-label="Lower the loudness"'), loud);
  assert.ok(loud.includes('aria-label="Raise the loudness"'), loud);
  assert.ok(loud.includes('data-json="{&quot;step&quot;:-1}"'), loud);
  assert.ok(loud.includes('data-json="{&quot;step&quot;:1}"'), loud);
  for (const name of ['dac', 'dac-flat', 'dac-loudness', 'dac-loudness-strength']) {
    const html = drawn(name, 'tone');
    assert.doesNotMatch(html, /r-loud|Loudness strength/, name);
  }
  for (const name of ['dac', 'dac-flat']) {
    const html = drawn(name, 'tone');
    assert.ok(!html.includes('class="rb"') && !html.includes('eq/loudness'), name);
  }
  // the strength's words show on the Loudness button once it is read, in Loudness only, and both
  // round buttons are described by them
  assert.ok(!drawn('dac-loudness', 'tone').includes('id="loud-strength"'));
  assert.ok(loud.includes('<span class="lv" id="loud-strength">strength <span class="n">40</span>'), loud);
  assert.equal(count(loud, 'aria-describedby="loud-strength"'), 2);
  for (const mode of ['flat', 'eq']) {
    const tone = { ...deviceOf('dac-loudness-strength').tone, mode, loud: false, strength: null };
    assert.doesNotMatch(drawn('dac', 'tone', { tone }), /strength/, mode);
  }
  // the mode button and the round buttons are siblings, never nested
  for (const piece of loud.split('</button>')) {
    assert.ok(count(piece, '<button') <= 1, piece);
  }
});

ok('the_tone_and_preset_forms_post_and_a_guest_sees_none', () => {
  const tone = drawn('dac-loudness-strength', 'tone');
  assert.ok(tone.includes('data-mode="flat" data-json="{&quot;mode&quot;:&quot;flat&quot;}"'), tone);
  assert.ok(tone.includes('data-post="eq/loudness"'), tone);
  const html = drawn('dac-user-preset', 'presets');
  assert.ok(count(html, 'data-json="{&quot;name&quot;:&quot;') >= 5, 'a recall from each row');
  assert.equal(count(html, 'data-form="rename"'), 1, html);
  assert.equal(count(html, 'data-form="delete"'), 1, html);
  assert.equal(count(html, 'data-form="save"'), 1, html);
  assert.doesNotMatch(html, /<b>|<B>/, 'a name is text');
  // a configuration that cannot be written keeps the recall and hides the forms
  const off = drawn('dac-presets-off', 'presets');
  assert.match(off, /data-json=/);
  assert.doesNotMatch(off, /data-form=|data-open/);
  // a guest sees none of it
  for (const name of ['dac-guest', 'dac-readonly']) {
    const guest = drawn(name, 'presets') + drawn(name, 'tone');
    assert.doesNotMatch(guest, /data-form=|data-json|data-open/, name);
    assert.match(guest, /aria-disabled="true"/, name);
  }
});

ok('an_amp_with_only_meters_is_lone', () => {
  for (const name of ['five', 'mono']) {
    assert.ok(screenOf(name).includes('class="view amp lone"'), name);
  }
  assert.ok(!screenOf('amp').includes('lone'));
});

ok('a_lost_link_dims_and_locks_the_view_and_a_fault_leaves_it_alone', () => {
  const lost = screenOf('lost');
  assert.ok(lost.includes('class="card warn"'), lost);
  assert.ok(/view dac away"/.test(lost), lost);
  assert.equal(count(lost, '<div class="col" inert>'), 2, lost);
  const fault = screenOf('fault');
  assert.ok(fault.includes('class="card fault"'), fault);
  assert.ok(!fault.includes(' away') && !fault.includes(' inert'), fault);
  const refused = screenOf('refused-amp');
  assert.ok(refused.includes(' away'), refused);
  assert.ok(!refused.includes(' inert'), refused);
});

ok('card_lists_every_active_fault', () => {
  const card = drawn('fault', 'card');
  assert.equal(count(card, '<li>'), 2, card);
  assert.ok(card.includes('<h2>2 faults</h2>'), card);
});

ok('an_active_fault_is_in_the_card_and_never_in_controls', () => {
  const controls = drawn('fault', 'controls') + drawn('fault', 'power');
  assert.doesNotMatch(controls, /data-key="fault_/);
  assert.ok(drawn('fault', 'card').includes('<li>'));
});

ok('facts_list_every_reading_named_or_not', () => {
  const facts = deviceOf('dac-facts-heard').facts;
  const rows = [
    { id: '2', label: 'Input', value: '3 (Optical)', heard_at_ms: 1700000093000 },
    { id: '210.2.2', label: '?', value: '9', heard_at_ms: 1700000040000 },
  ];
  const html = drawn('dac-facts-heard', 'facts', { facts: { ...facts, rows } });
  assert.ok(html.includes('data-now="1700000100000"'), html);
  assert.ok(html.includes('data-heard-at="1700000093000"'), html);
  assert.ok(html.includes('>210.2.2<') && html.includes('>?<'), html);
});

ok('the_passphrase_never_reaches_a_remote_view_in_the_facts', () => {
  for (const c of fixture.cases.filter((x) => x.page.device)) {
    assert.ok(!drawn(c.name, 'facts').includes('>250<'), c.name);
  }
});

ok('facts_show_firmware_and_dsp_when_read_and_dash_when_not', () => {
  const facts = deviceOf('dac').facts;
  const read = drawn('dac', 'facts', { facts: { ...facts, firmware: '11', dsp: 4 } });
  assert.ok(read.includes('<dd>11</dd>') && read.includes('<dd>4</dd>'), read);
  const unread = drawn('dac', 'facts', { facts: { ...facts, firmware: '3 (advertised)', dsp: null } });
  assert.ok(unread.includes('<dt>DSP</dt><dd>—</dd>'), unread);
});

ok('facts_say_the_device_the_link_and_the_signal', () => {
  const guest = drawn('dac-guest', 'facts');
  const { unit } = deviceOf('dac').facts;
  assert.ok(unit && !guest.includes(unit), 'a guest sees no unit id');
  assert.ok(!guest.includes('Passphrase'), guest);
  assert.ok(guest.includes('data-identify') && guest.includes('aria-disabled="true"'), guest);
  assert.ok(drawn('dac', 'facts').includes(unit));
});

ok('facts_are_closed_by_default', () => {
  const html = drawn('dac', 'facts');
  assert.ok(html.includes('<details class="device-facts">'), html);
  assert.ok(!html.includes(' open'), html);
  assert.ok(screenOf('dac').includes('id="part-facts"'));
  assert.ok(screenOf('amp').includes('id="part-facts"'));
});

ok('a_profile_that_offers_no_identify_has_no_identify_button', () => {
  const facts = deviceOf('dac').facts;
  const none = drawn('dac', 'facts', { facts: { ...facts, identify_offered: false } });
  assert.ok(!none.includes('data-identify') && !none.includes('Identify'), none);
  assert.ok(drawn('dac', 'facts').includes('data-identify'));
});

ok('the_identify_button_carries_the_daemons_gap', () => {
  const gap = deviceOf('dac').facts.identify_gap_secs;
  assert.ok(drawn('dac', 'facts').includes(`data-gap="${gap}"`));
});

ok('facts_escape_what_a_label_and_a_value_say', () => {
  const facts = deviceOf('dac').facts;
  const rows = [{
    id: '2', label: '<script>alert("x")</script>', value: '<img src=x onerror="y">', heard_at_ms: null,
  }];
  const html = drawn('dac', 'facts', { facts: { ...facts, rows } });
  assert.ok(!html.includes('<script>') && !html.includes('<img'), html);
  assert.ok(html.includes('&lt;script&gt;') && html.includes('&lt;img'), html);
  assert.ok(html.includes('onerror=&quot;y&quot;') && !html.includes('onerror="y"'), html);
});

ok('a_loki_max_screen_has_device_presets_and_no_tone_presets', () => {
  const html = drawn('lokimax', 'device-presets');
  assert.ok(html.includes('id="part-device-presets"'), html);
  assert.ok(html.includes('<h3>Presets</h3>'), html);
  assert.equal(count(html, 'data-json="{&quot;slot&quot;:'), 3, 'a recall from each slot');
  assert.equal(count(html, 'data-json="{&quot;name&quot;:'), 5, 'an apply from each setting');
  assert.equal(count(html, 'data-json="{&quot;confirm&quot;:true,&quot;slot&quot;:'), 3, 'a save form each');
  assert.ok(html.includes('Save the bands to Preset 2? What the device holds there is replaced.'));
  assert.ok(html.includes('Built in'));
  const whole = screenOf('lokimax');
  assert.ok(whole.includes('id="part-device-presets"'));
  assert.ok(!whole.includes('id="part-presets"'));
});

ok('a_guest_page_shows_no_save', () => {
  const open = drawn('lokimax', 'device-presets');
  assert.ok(open.includes('data-form="save"') && open.includes('data-open="save"'));
  const html = drawn('lokimax-guest', 'device-presets');
  assert.doesNotMatch(html, /data-json|data-form=|data-open/);
  assert.ok(html.includes('aria-disabled="true"'));
});

ok('the_builtin_the_bands_match_is_lit_and_no_slot_is', () => {
  for (const name of ['lokimax', 'lokimax-flat']) {
    const device = deviceOf(name);
    const lit = device.device_presets.builtins.filter((b) => b.on).length;
    const html = drawn(name, 'device-presets');
    assert.equal(count(html, 'aria-pressed="true"'), lit, name);
    const slots = html.split('<h4>Built in</h4>')[0];
    assert.ok(!slots.includes('aria-pressed="true"'), 'no slot is lit');
  }
  assert.equal(deviceOf('lokimax-flat').device_presets.builtins.filter((b) => b.on).length, 1);
});

ok('with_no_player_the_card_is_one_quiet_line', () => {
  const html = drawn('no-player', 'playing');
  assert.ok(html.includes('class="p-playing idle"'), html);
  assert.ok(html.includes('No player'), html);
  for (const gone of ['data-press', '<img', 'np-head', 'np-prog']) {
    assert.ok(!html.includes(gone), gone);
  }
});

ok('the_verdict_chip_comes_before_the_hops', () => {
  const html = drawn('playing-changed-route', 'playing');
  assert.ok(html.indexOf('np-verdict') < html.indexOf('np-hop'), html);
  assert.ok(count(html, 'np-hop') >= 3);
});

ok('a_preset_named_with_markup_is_shown_as_text', () => {
  const markup = '<img src=x onerror=alert(1)>';
  const presets = deviceOf('dac-user-preset').presets;
  const rows = presets.rows.map((r) => (r.built_in ? r : { ...r, name: markup }));
  const status = { ...presets.status, name: markup };
  const html = drawn('dac-user-preset', 'presets', { presets: { ...presets, rows, status } });
  assert.ok(!html.includes('<img src=x'), html);
  assert.ok(html.includes('&lt;img src=x onerror=alert(1)&gt;'), html);
  assert.ok(html.includes(`<strong>${esc(markup)}</strong>`), html);
});

ok('the_tone_controls_are_inert_for_a_viewer_who_may_not_write', () => {
  // how many of each control the page draws, and how many of those are inert
  const tone = (name) => {
    const html = drawn(name, 'tone') + drawn(name, 'presets'); // the tone part holds the bands
    return ['data-mode=', 'class="pre"', 'class="band"'].map((marker) => {
      const all = html.split(marker).slice(1);
      const inert = all.filter((rest) => rest.slice(0, rest.indexOf('>')).includes('aria-disabled="true"'));
      return [all.length, inert.length];
    });
  };
  assert.deepEqual(tone('dac'), [[3, 0], [4, 0], [5, 0]]);
  assert.deepEqual(tone('dac-readonly'), [[3, 3], [4, 4], [5, 5]]);
  assert.deepEqual(tone('dac-guest'), [[3, 3], [4, 4], [5, 5]]);
});

ok('the_link_part_holds_no_menu_button', () => {
  const html = drawn('dac', 'link');
  assert.ok(html.includes('id="part-link"'), html);
  assert.ok(!html.includes('<button') && !html.includes('haspopup'), html);
});

ok('a_guests_knob_is_marked_inert', () => {
  assert.ok(drawn('dac-guest', 'volume').includes('data-inert="true"'));
  assert.ok(!drawn('dac', 'volume').includes('data-inert'));
});

ok('a_negative_level_hangs_its_minus_in_its_own_span', () => {
  const html = drawn('dac', 'volume');
  const at = html.indexOf('class="num"');
  const number = html.slice(at, html.indexOf('</p>', at));
  assert.ok(number.startsWith('class="num"><span class="neg">−'), number);
  assert.equal(count(number, '−'), 1, number);
});

ok('every_id_of_a_screen_is_a_css_identifier_for_any_device_name', () => {
  for (const c of fixture.cases) {
    const html = `${screen(c.page)} ${shell(c.page, WORDMARK)}`;
    for (const [, id] of html.matchAll(/ id="([^"]*)"/g)) {
      assert.match(id, /^[A-Za-z][A-Za-z0-9_-]*$/, `${c.name}: id ${JSON.stringify(id)}`);
    }
  }
});

ok('the_switcher_links_a_device_by_its_encoded_name', () => {
  const html = shellOf('two-devices-odd-name');
  assert.ok(html.includes('href="./?device=Desk%20A%2FB" aria-current="page"'), html);
  assert.ok(html.includes('href="./?device=Aegir%202" aria-current="false"'), html);
});

ok('the_switcher_swaps_the_screen_through_htmx', () => {
  // each link names its device for the script that moves the page in place, keeps its `href` for
  // a page without script, and carries no attribute htmx would read
  const html = shellOf('two-devices-odd-name');
  assert.ok(html.includes('data-device="Desk%20A%2FB"'), html);
  assert.ok(html.includes('data-device="Aegir%202"'), html);
  assert.doesNotMatch(html, /hx-/);
});

/** The `.side` container of a shell: what the header holds to the right. */
const sideOf = (html) => {
  const side = html.split('<div class="side">')[1];
  return side.slice(0, side.indexOf('</div>'));
};
const guestCase = fixture.cases.find((c) => c.page.viewer.guest && c.page.device).name;
const readOnlyCase = fixture.cases.find(
  (c) => !c.page.viewer.guest && !c.page.viewer.writes && c.page.device,
).name;

ok('a_guests_link_pill_and_sign_in_share_one_container', () => {
  const side = sideOf(shellOf(guestCase));
  assert.ok(side.includes('id="part-link"') && side.includes('href="./?sign-in"'), side);
  assert.ok(!sideOf(shellOf('dac')).includes('sign-in'));
});

ok('the_menu_slot_sits_before_the_link_pill_for_everyone', () => {
  for (const [who, name] of [['signed in', 'dac'], ['guest', guestCase]]) {
    const html = shell(caseOf(name).page, WORDMARK);
    const side = html.split('<div class="side">')[1];
    const pill = side.indexOf('<span id="part-link"');
    const slot = side.indexOf('<span class="fxslot"></span>');
    assert.ok(pill >= 0 && slot >= 0 && slot < pill, `${who}: ${side}`);
    assert.equal(side.slice(slot + 28, pill).trim(), '', `${who}: nothing between them`);
    // The button and its menu are the frame's, never the theme's.
    for (const frame of ['fxbtn', 'fxpop', 'aria-haspopup', 'menuitem']) {
      assert.ok(!html.includes(frame), `${who}: the shell draws no ${frame}`);
    }
  }
});

ok('every_shell_has_one_slot_first_in_the_headers_side_even_with_no_device', () => {
  for (const c of fixture.cases) {
    const html = shell(c.page, WORDMARK);
    assert.equal(html.split('class="fxslot"').length - 1, 1, c.name);
    assert.ok(html.includes('<div class="side"> <span class="fxslot"></span>'), c.name);
  }
});

ok('the_strips_canvas_sits_inside_the_app_above_its_footer', () => {
  const html = shell(caseOf('dac').page, WORDMARK);
  const app = html.indexOf('class="app"');
  const canvas = html.indexOf('id="hist"');
  const footer = html.indexOf('<footer');
  assert.ok(app >= 0 && app < canvas && canvas < footer, html);
});

ok('the_keys_are_offered_to_a_viewer_who_may_write', () => {
  const writer = caseOf('dac').page;
  const html = shell(writer, WORDMARK);
  assert.equal(bodyClass(writer), 'writes');
  for (const wanted of [
    '<p class="kh2" aria-hidden="true">',
    '<div class="keys" id="keys" hidden>',
    '<dt><kbd>A</kbd></dt><dd>Arrange sections</dd>',
    '<dt><kbd>T</kbd></dt><dd>Themes</dd>',
    '<dt><kbd>?</kbd></dt><dd>This list</dd>',
    '<dd>Centre the balance</dd>',
  ]) {
    assert.ok(html.includes(wanted), wanted);
  }
  for (const name of [readOnlyCase, guestCase]) {
    const page = caseOf(name).page;
    const keys = shell(page, WORDMARK);
    assert.notEqual(bodyClass(page), 'writes', name);
    for (const absent of ['<dd>Centre the balance</dd>', '<dd>Volume one step</dd>', '<dd>Tone control to Flat</dd>']) {
      assert.ok(!keys.includes(absent), `${name}: ${absent}`);
    }
    for (const wanted of [
      '<div class="keys" id="keys" hidden>',
      '<kbd>v</kbd></dt><dd class="kviz">The visualizer\'s look</dd>',
      '<dt><kbd>A</kbd></dt><dd>Arrange sections</dd>',
      '<dt><kbd>T</kbd></dt><dd>Themes</dd>',
      '<dt><kbd>?</kbd></dt><dd>This list</dd>',
      '<p class="kh2" aria-hidden="true"><kbd>?</kbd> keys</p>',
    ]) {
      assert.ok(keys.includes(wanted), `${name}: ${wanted}`);
    }
  }
});

if (failed > 0) {
  console.log(`FAIL ${failed} of ${n} ember-parts checks failed`);
  process.exit(1);
}
console.log(`${n} ember-parts checks passed`);
