/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for core/plot.js, the shared SVG strings of the response plot and the sparkline:
// the frame and its lines against tone-curve.js's maps, a curve through known points, spans
// clipped to the view, a sparkline's gaps, single sample, empty and flat series, escaping; and
// that Signal's and Paper's tone plots, drawn on it, are what they drew before the move
// (fixtures/plot-move.json holds the old output). Run: node plot.mjs
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import {
  F_MAX, F_MIN, VIEW_HEIGHT, VIEW_WIDTH, logFreqs, POINTS, responseDb, xOf, yOf,
} from '../../../bins/svolderd/web/core/tone-curve.js';
import {
  FREQ_MARKS, bandSpans, curveD, fillD, frame, freqAt, gapSpans, fitWords, gridLines, overlayNote, overlayShapes, path, pathThrough, sparkline,
  zeroLine,
} from '../../../bins/svolderd/web/core/plot.js';
import * as sig from '../../../bins/svolderd/web/themes/signal/draw/tone.js';
import * as pap from '../../../bins/svolderd/web/themes/paper/draw/tone.js';
import { SECTIONS } from '../../../bins/svolderd/web/themes/paper/draw/sheet.js';
import * as papComputer from '../../../bins/svolderd/web/themes/paper/draw/computer.js';
import { computerView, modelOf, pageWith, point } from './paper-rig.mjs';

let n = 0;
const ok = (name, fn) => { fn(); n++; console.log('ok', name); };
const fixture = JSON.parse(readFileSync(new URL('./fixtures/ember-parts.json', import.meta.url), 'utf8'));
const old = JSON.parse(readFileSync(new URL('./fixtures/plot-move.json', import.meta.url), 'utf8'));
const view = { phone: false, coarse: false, state: 'open', now: 0, rulerPx: 1000 };
/** The numbers of a path string, in order. */
const nums = (d) => (d.match(/-?\d+(?:\.\d+)?/g) ?? []).map(Number);
const attr = (html, name) => [...html.matchAll(new RegExp(` ${name}="([^"]*)"`, 'g'))].map((m) => m[1]);

ok('the_frame_is_the_tone_curves_view_box', () => {
  const svg = frame('<g/>');
  assert.equal(svg, `<svg viewBox="0 0 ${VIEW_WIDTH} ${VIEW_HEIGHT}" preserveAspectRatio="none" aria-hidden="true"><g/></svg>`);
  assert.ok(frame('', { cls: 'a "b"' }).includes('class="a &quot;b&quot;"'));
});

ok('the_zero_line_is_at_0_db_across_the_plot', () => {
  assert.equal(zeroLine('z'), `<path class="z" d="M0 ${yOf(0)}H${VIEW_WIDTH}"/>`);
});

ok('the_grid_stands_at_the_frequencies_x', () => {
  const html = gridLines([100, 1000], 'g');
  assert.deepEqual(attr(html, 'd'), [`M${xOf(100).toFixed(1)} 0V${VIEW_HEIGHT}`, `M${xOf(1000).toFixed(1)} 0V${VIEW_HEIGHT}`]);
  assert.equal(gridLines([0, -5, NaN, 'x', F_MAX * 2], 'g'), '');
});

ok('the_frequency_marks_sit_at_fractions_of_the_plot', () => {
  assert.deepEqual(FREQ_MARKS.map(([, t]) => t), ['60', '250', '1k', '4k', '12k']);
  assert.equal(freqAt(F_MIN), 0);
  assert.ok(Math.abs(freqAt(F_MAX) - 1) < 1e-12);
  assert.equal(freqAt(1000), xOf(1000) / VIEW_WIDTH);
});

ok('a_curve_goes_through_known_points', () => {
  const d = pathThrough([{ hz: 20, db: 0 }, { hz: 1000, db: 13 }, { hz: 20000, db: -13 }]);
  assert.deepEqual(nums(d), [0, yOf(0), xOf(1000), 0, 1000, VIEW_HEIGHT].map((v) => Math.round(v * 10) / 10));
  assert.ok(d.startsWith('M') && d.split('L').length === 3);
  assert.equal(pathThrough([]), '');
  assert.equal(pathThrough([{ hz: 100, db: NaN }, { hz: 0, db: 1 }]), '');
  assert.deepEqual(nums(pathThrough([{ hz: 100, db: 99 }])), [Math.round(xOf(100) * 10) / 10, 0]);
});

ok('the_response_curve_and_its_fill_close_along_the_zero_line', () => {
  const r = responseDb([{ f0: 1000, gainDb: 6, q: 1 }], logFreqs(POINTS));
  assert.ok(fillD(r).startsWith(curveD(r)));
  assert.ok(fillD(r).endsWith(`L${VIEW_WIDTH} ${yOf(0)}L0 ${yOf(0)}Z`));
  assert.equal(curveD(r).split('L').length, POINTS);
});

ok('full_height_spans_are_clipped_to_the_view', () => {
  const html = bandSpans([{ from_hz: 100, to_hz: 200 }, { from_hz: 1, to_hz: 30 }, { from_hz: 10000, to_hz: 90000 },
    { from_hz: 500, to_hz: 400 }, { from_hz: 5, to_hz: 10 }, { from_hz: 'a', to_hz: 5 }, { from_hz: 30000, to_hz: 40000 }], 's');
  const rects = [...html.matchAll(/<rect class="s" x="([^"]*)" y="0" width="([^"]*)" height="260"\/>/g)];
  assert.equal(rects.length, 3);
  const [a, b, c] = rects.map((m) => [Number(m[1]), Number(m[2])]);
  assert.ok(Math.abs(a[0] - xOf(100)) < 0.06 && Math.abs(a[0] + a[1] - xOf(200)) < 0.06);
  assert.equal(b[0], 0);
  assert.ok(Math.abs(b[0] + b[1] - xOf(30)) < 0.06);
  assert.ok(Math.abs(c[0] - xOf(10000)) < 0.06 && Math.abs(c[0] + c[1] - VIEW_WIDTH) < 0.06);
});

const grid = logFreqs(POINTS);
const target = grid.map((hz) => ({ hz, db: 0 }));
const fit = grid.map((hz) => ({ hz, db: hz > 500 && hz < 2000 ? 3 : 0 }));

ok('a_gap_span_shades_between_the_two_curves_only_inside_its_range', () => {
  const html = gapSpans({ spans: [{ from_hz: 600, to_hz: 1500 }], target, fit, cls: 'gap' });
  const [d] = attr(html, 'd');
  const xs = nums(d).filter((_, i) => i % 2 === 0);
  const ys = nums(d).filter((_, i) => i % 2 === 1);
  assert.ok(Math.min(...xs) >= xOf(600) - 0.06 && Math.max(...xs) <= xOf(1500) + 0.06);
  assert.ok(Math.abs(Math.min(...xs) - xOf(600)) < 0.06 && Math.abs(Math.max(...xs) - xOf(1500)) < 0.06);
  assert.ok(ys.every((y) => y === yOf(0) || y === yOf(3)));
  assert.ok(d.startsWith('M') && d.endsWith('Z'));
  assert.ok(html.startsWith('<path class="gap"'));
});

ok('a_gap_span_outside_the_view_or_the_curves_is_dropped_and_one_edge_is_clipped', () => {
  assert.equal(gapSpans({ spans: [{ from_hz: 30000, to_hz: 40000 }, { from_hz: 5, to_hz: 10 }], target, fit, cls: 'gap' }), '');
  assert.equal(gapSpans({ spans: [{ from_hz: 600, to_hz: 1500 }], target: [], fit: [], cls: 'gap' }), '');
  assert.equal(gapSpans({ spans: [{ from_hz: 600, to_hz: 1500 }], target, fit: [], cls: 'gap' }), '');
  const wide = gapSpans({ spans: [{ from_hz: 1, to_hz: 1e9 }], target, fit, cls: 'gap' });
  const xs = nums(attr(wide, 'd')[0]).filter((_, i) => i % 2 === 0);
  assert.equal(Math.min(...xs), 0);
  assert.ok(Math.abs(Math.max(...xs) - VIEW_WIDTH) < 0.06);
});

const box = (html) => attr(html, 'viewBox')[0];
const spark = (points, o = {}) => sparkline({ points, width: 100, height: 20, pad: 2, cls: 'sp', ...o });
const segs = (html) => (attr(html, 'd')[0] ?? '').split('M').filter(Boolean);

ok('a_sparkline_has_a_fixed_box_and_the_scale_the_caller_gives', () => {
  const html = spark([{ t: 0, v: 10 }, { t: 10, v: 20 }, { t: 20, v: 30 }], { min: 0, max: 40 });
  assert.equal(box(html), '0 0 100 20');
  assert.ok(html.includes('class="sp"'));
  assert.deepEqual(nums(attr(html, 'd')[0]), [0, 14, 50, 10, 100, 6]);
});

ok('a_sparkline_without_a_scale_spans_the_data', () => {
  const ys = nums(attr(spark([{ t: 0, v: 5 }, { t: 1, v: 9 }, { t: 2, v: 7 }]), 'd')[0]).filter((_, i) => i % 2);
  assert.deepEqual(ys, [18, 2, 10]);
});

ok('missing_samples_break_the_line', () => {
  const html = spark([{ t: 0, v: 1 }, { t: 1, v: 2 }, { t: 2, v: null }, { t: 3, v: 2 }, { t: 4, v: 1 }, { t: 5, v: NaN },
    { t: 6, v: 3 }, { t: 7, v: 4 }]);
  assert.equal(segs(html).length, 3);
  assert.ok(segs(html).every((s) => !s.includes('NaN')));
});

ok('a_time_gap_over_the_limit_breaks_the_line', () => {
  const html = spark([{ t: 0, v: 1 }, { t: 1, v: 2 }, { t: 10, v: 3 }, { t: 11, v: 4 }], { gap: 5 });
  assert.equal(segs(html).length, 2);
  assert.equal(segs(spark([{ t: 0, v: 1 }, { t: 1, v: 2 }, { t: 10, v: 3 }])).length, 1);
});

ok('a_lone_sample_is_a_dot_and_an_empty_series_is_the_box_alone', () => {
  const one = spark([{ t: 5, v: 7 }]);
  assert.ok(one.includes('<circle'));
  assert.ok(!one.includes(' NaN') && !one.includes('Infinity'));
  const broken = spark([{ t: 0, v: 1 }, { t: 1, v: null }, { t: 2, v: 3 }]);
  assert.equal((broken.match(/<circle/g) ?? []).length, 2);
  for (const e of [[], null, undefined, [{ t: 1, v: null }]]) {
    const html = spark(e);
    assert.equal(box(html), '0 0 100 20');
    assert.ok(!html.includes('<path') && !html.includes('<circle'));
  }
});

ok('a_flat_series_sits_mid_height_with_no_division_by_zero', () => {
  const html = spark([{ t: 0, v: 4 }, { t: 1, v: 4 }, { t: 2, v: 4 }]);
  assert.deepEqual(nums(attr(html, 'd')[0]).filter((_, i) => i % 2), [10, 10, 10]);
  const same = spark([{ t: 3, v: 1 }, { t: 3, v: 2 }]);
  assert.ok(!same.includes('NaN') && !same.includes('Infinity'));
  const equalScale = spark([{ t: 0, v: 4 }, { t: 1, v: 5 }], { min: 4, max: 4 });
  assert.ok(!equalScale.includes('NaN'));
});

ok('a_window_the_caller_gives_places_the_samples_in_it_and_drops_those_outside', () => {
  const html = spark([{ t: -5, v: 1 }, { t: 50, v: 2 }, { t: 100, v: 3 }, { t: 500, v: 4 }], { from: 0, to: 100, min: 0, max: 4 });
  assert.deepEqual(nums(attr(html, 'd')[0]).filter((_, i) => i % 2 === 0), [50, 100]);
});

ok('the_values_the_caller_gives_are_clamped_to_its_scale', () => {
  const ys = nums(attr(spark([{ t: 0, v: -9 }, { t: 1, v: 99 }], { min: 0, max: 10 }), 'd')[0]).filter((_, i) => i % 2);
  assert.deepEqual(ys, [18, 2]);
});

ok('a_caller_supplied_string_is_escaped', () => {
  const evil = '"><script>x</script>';
  for (const html of [frame('', { cls: evil }), zeroLine(evil), gridLines([100], evil), bandSpans([{ from_hz: 100, to_hz: 200 }], evil),
    gapSpans({ spans: [{ from_hz: 600, to_hz: 1500 }], target, fit, cls: evil }), spark([{ t: 0, v: 1 }], { cls: evil })]) {
    assert.ok(!html.includes('<script'), html);
    assert.ok(html.includes('&quot;&gt;&lt;script&gt;'), html);
  }
});

ok('signals_and_papers_tone_plots_are_what_they_were_before_the_move', () => {
  const sets = {
    flat: [], one: [{ f0: 1000, gainDb: 6, q: 1 }],
    wide: [{ f0: 60, gainDb: -20, q: 0.5 }, { f0: 4000, gainDb: 12, q: 8 }, { f0: 12000, gainDb: 20, q: 2 }],
  };
  const F = logFreqs(POINTS);
  for (const [k, bands] of Object.entries(sets)) {
    const r = responseDb(bands, F);
    const want = old.curves[k];
    assert.equal(curveD(r), want.curvePath, k);
    assert.equal(fillD(r), want.shadePath, k);
    assert.equal(curveD(r), want.paperPath, k);
    assert.equal(sig.plot({ id: `x-${k}`, response: r, faint: false, points: '<b></b>' }), want.plot, k);
    assert.equal(sig.plot({ id: `y-${k}`, response: r, faint: true, points: '' }), want.plotFaint, k);
  }
  const e = old.curves.empty;
  assert.equal(curveD([]), e.curvePath);
  assert.equal(fillD([]), e.shadePath);
  assert.equal(sig.plot({ id: 'e', response: [], faint: false, points: '' }), e.plot);
  assert.equal(curveD([]), e.paperPath);
  let pages = 0;
  for (const c of fixture.cases) {
    const d = c.page?.device;
    if (!d?.tone) continue;
    assert.equal(sig.toneBox(d), old.pages[c.name].signal, c.name);
    assert.equal(SECTIONS.tone(c.page, d, view), old.pages[c.name].paper, c.name);
    pages++;
    if (!d.tone.readings?.bands) continue;
    assert.deepEqual(sig.heldGraph(d.tone, d.tone_steps, 1, { f: 3, g: 4, q: 2 }), old.held[c.name].signal, c.name);
    assert.deepEqual(pap.heldGraph(d.tone, d.tone_steps, 1, { f: 3, g: 4, q: 2 }), old.held[c.name].paper, c.name);
  }
  assert.equal(pages, Object.keys(old.pages).length);
});

ok('paper_computer_side_is_what_it_was_before_the_move', () => {
  const cases = {
    seven: modelOf(), off: modelOf({ on: false }), one: modelOf({ points: [point('PK', 1000, 0, 1)] }),
    edge: modelOf({ points: [point('LSC', 20, 6, 0.7), point('PK', 20000, -20, 8), point('HSC', 12000, 20, 1)] }),
    none: modelOf({ points: [] }),
  };
  for (const [k, m] of Object.entries(cases)) {
    const page = pageWith('dac', m);
    const { view: v } = computerView(page, 'computer');
    assert.equal(SECTIONS.tone(page, page.device, v), old.computer.tone[k], k);
    assert.equal(papComputer.heldCurve(m.points ?? []), old.computer.held[k], k);
  }
  assert.equal(curveD([1, -5, 30, -30]), old.computer.path);
});

ok('a_big_series_does_not_overflow_the_stack', () => {
  const big = Array.from({ length: 200000 }, (_, i) => ({ t: i, v: i % 100 }));
  const html = sparkline({ points: big, cls: 'sp' });
  assert.ok(html.includes('<path') && !html.includes('NaN'));
});

ok('a_size_that_is_not_a_number_takes_its_default_and_never_reaches_the_markup', () => {
  const evil = 'x"><b>';
  const html = sparkline({ points: [{ t: 0, v: 1 }, { t: 1, v: 2 }], width: evil, height: evil, pad: evil, cls: 'sp' });
  assert.ok(!html.includes('<b>') && !html.includes('NaN'));
  assert.equal(box(html), '0 0 120 32');
  assert.equal(box(sparkline({ points: [], width: -4, height: 0, pad: 99 })), '0 0 120 32');
  const huge = spark([{ t: 0, v: -1e308 }, { t: 1, v: 1e308 }]);
  assert.ok(!huge.includes('NaN'));
});

ok('a_path_d_is_escaped_and_no_class_writes_no_attribute', () => {
  assert.ok(!path('M0 0"><b>', 'c').includes('<b>'));
  assert.ok(!zeroLine().includes('class'));
  assert.ok(!spark([{ t: 0, v: 1 }, { t: 1, v: 2 }], { cls: undefined }).includes('class'));
});

ok('a_span_at_the_view_edges_and_unsorted_samples', () => {
  const [r] = [...bandSpans([{ from_hz: F_MIN, to_hz: F_MAX }], 's').matchAll(/x="([^"]*)" y="0" width="([^"]*)"/g)];
  assert.deepEqual([Number(r[1]), Number(r[2])], [0, VIEW_WIDTH]);
  const a = spark([{ t: 2, v: 3 }, { t: 0, v: 1 }, { t: 1, v: 2 }]);
  assert.equal(a, spark([{ t: 0, v: 1 }, { t: 1, v: 2 }, { t: 2, v: 3 }]));
  const g = gapSpans({ spans: [{ from_hz: 100, to_hz: 200 }], target: [{ hz: 0, db: 1 }, { hz: -5, db: 1 }], fit, cls: 'g' });
  assert.equal(g, '');
});

/** A tone view's overlay as the daemon serves it: a target and a fit on the 181 log points. */
const overlayOf = (over = {}) => {
  const hzs = logFreqs(POINTS);
  return {
    name: 'Imp', target: hzs.map(() => 4), fit: hzs.map((hz) => (hz > 500 && hz < 2000 ? 0 : 4)),
    gap: [[600, 1500]], gap_db: 2, rms_db: 0.93, worst_db: 4, worst_hz: 1000, ...over,
  };
};

ok('an_overlay_draws_the_target_and_the_daemons_gap_spans_only', () => {
  const shapes = overlayShapes(overlayOf(), { target: 't', gap: 'g' });
  assert.ok(shapes.target.startsWith('<path class="t"') && shapes.target.includes(`${yOf(4).toFixed(1)}`));
  assert.equal(shapes.gap.match(/<path/g).length, 1);
  assert.ok(shapes.gap.startsWith('<path class="g"'));
  // a miss under gap_db is not in the daemon's spans, so nothing is filled
  const none = overlayShapes(overlayOf({ gap: [], fit: overlayOf().target.map((v) => v + 1) }), { target: 't', gap: 'g' });
  assert.equal(none.gap, '');
  assert.ok(none.target.length > 0);
});

ok('an_overlay_that_is_absent_or_has_no_fit_draws_what_it_can', () => {
  assert.deepEqual(overlayShapes(undefined, { target: 't', gap: 'g' }), { target: '', gap: '' });
  assert.deepEqual(overlayShapes(null, { target: 't', gap: 'g' }), { target: '', gap: '' });
  const noFit = overlayShapes(overlayOf({ fit: null }), { target: 't', gap: 'g' });
  assert.equal(noFit.gap, '');
  assert.ok(noFit.target.length > 0);
});

ok('the_fit_is_worded_from_the_daemons_rms_alone', () => {
  assert.equal(fitWords(overlayOf()), 'fit 0.9 dB');
  assert.equal(fitWords(overlayOf({ rms_db: 12 })), 'fit 12.0 dB');
  assert.equal(fitWords(overlayOf({ rms_db: null })), '');
  assert.equal(fitWords(undefined), '');
});

ok('the_overlay_note_says_the_fit_the_worst_miss_and_what_the_shapes_are', () => {
  assert.equal(overlayNote(overlayOf()), 'fit 0.9 dB, worst 4.0 dB at 1.0 kHz. The dotted line is the target; '
    + 'the shade is where the bands miss it by more than 2 dB.');
  assert.match(overlayNote(overlayOf({ worst_hz: 85, worst_db: 2.25, gap_db: 1.5 })), /worst 2\.2 dB at 85 Hz\..*more than 1\.5 dB/);
  // one reading of a frequency in the overlay, the EQ library and the terminal
  assert.match(overlayNote(overlayOf({ worst_hz: 95.3 })), /at 95\.3 Hz\./);
  assert.match(overlayNote(overlayOf({ worst_hz: 20 })), /at 20 Hz\./);
  assert.match(overlayNote(overlayOf({ worst_hz: null })), /^fit 0\.9 dB\. The dotted/);
  assert.equal(overlayNote(overlayOf({ rms_db: null })), '');
  assert.equal(overlayNote(undefined), '');
});

console.log(`plot.mjs: ${n} checks passed`);
