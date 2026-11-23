// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

import {equal, fixture, ok, patched, run, test} from './harness.js';
import {
    ageText, B17_LEGEND, barView, dbFromFraction, dbText, offMarkup, parseLine, popupView, sliderFraction,
    tintFor,
} from '../svolder-indicator@rralex.github.io/model.js';

const [untouched] = fixture('untouched');
const [dacChanged] = fixture('dac_changed');
const [away, awayBack] = fixture('away');
const [oldSignal] = fixture('old_signal');
// Three minutes after the old reading was heard.
const NOW = oldSignal.device.signal.heard_at_ms + 180000;

const UNIT_12 = '<span size="72%" rise="-2028" alpha="90%">dB</span>';
const gapped = (digits, last) => `${digits}<span letter_spacing="768">${last}</span>`;

test('untouched end to end: green LED, 44.1 tint, three bars, 0 dB', () => {
    const bar = barView(untouched);
    equal(bar.away, false);
    equal(bar.box, {outline: '#cfcfcf', led: 'dot', ledColor: '#8ff0a4'});
    equal(bar.signal, {filled: 3, color: '#ffffff', filledOpacity: 1, emptyOpacity: 0.3});
    equal(bar.figure, {kind: 'text', markup: `${gapped('', '0')}${UNIT_12}`});
});

test('a changed path (dac_changed): yellow LED, minus sign figure with the lowered unit', () => {
    const bar = barView(dacChanged);
    equal(bar.box, {outline: '#cfcfcf', led: 'dot', ledColor: '#f8e45c'});
    equal(bar.figure.markup, `${gapped('\u22122', '4')}${UNIT_12}`);
    ok(bar.accessibleName.includes('Bifrost 3'), 'the name carries the device');
});

test('resampled: 48 kHz is white, the computer segment is yellow', () => {
    const [resampled] = fixture('resampled');
    equal(barView(resampled).box, {outline: '#ffffff', led: 'dot', ledColor: '#f8e45c'});
    const path = popupView(resampled, NOW).path;
    equal(path.segments.map(s => s.color), ['#8ff0a4', '#f8e45c', '#8ff0a4']);
    equal(path.off, 'Computer: resampled');
});

test('the B17 table, rate by rate, and plain for any other rate', () => {
    const want = ['#cfcfcf', '#ffffff', '#b8d8ff', '#86b4ff', '#a58aff', '#d476f0', '#f27aa8', '#ff8c72'];
    equal([44100, 48000, 88200, 96000, 176400, 192000, 352800, 384000].map(tintFor), want);
    equal(tintFor(32000), null);
    equal(tintFor(22050), null);
    equal(tintFor(null), null);
    equal(B17_LEGEND.map(l => l.rate_text), ['44.1', '48', '88.2', '96', '176.4', '192', '352.8', '384']);
    const other = patched(untouched, {device: {rate_hz: 32000}});
    equal(barView(other).box.outline, null);
});

test('muted: the muted glyph replaces the figure; the Path says what is off', () => {
    const [muted] = fixture('muted');
    equal(barView(muted).figure, {kind: 'muted', markup: ''});
    equal(popupView(muted, NOW).volume.muted, true);
    equal(popupView(muted, NOW).path.off, 'DAC: muted');
});

test('away: no figure, no filled bar, grey, hollow ring, plain outline; then back', () => {
    const bar = barView(away);
    equal(bar.away, true);
    equal(bar.box, {outline: null, led: 'ring', ledColor: '#8a8a8a'});
    equal(bar.signal.filled, 0);
    equal(bar.signal.color, '#8a8a8a');
    equal(bar.figure.kind, 'none');
    equal(barView(awayBack).away, false);
    equal(barView(awayBack).figure.kind, 'text');
});

test('no view at all (failure) draws the away state', () => {
    const bar = barView(null);
    equal(bar.away, true);
    equal(bar.figure.kind, 'none');
    equal(barView({...untouched, device: null}).away, true);
});

test('weak signal: one bar in yellow', () => {
    const [weak] = fixture('weak_signal');
    equal(barView(weak).signal, {filled: 1, color: '#f8e45c', filledOpacity: 1, emptyOpacity: 0.3});
    equal(popupView(weak, NOW).header.signal.glyph.color, '#f8e45c');
});

test('old signal: the bars are dim and the popup says how long ago', () => {
    equal(barView(oldSignal).signal, {filled: 3, color: '#ffffff', filledOpacity: 0.5, emptyOpacity: 0.15});
    ok(popupView(oldSignal, NOW).header.signal.markup.endsWith(' \u00b7 3 min ago'), 'the age follows');
});

test('config_fail: the Path row is warn, so the LED is yellow though the sound is untouched', () => {
    const [bad] = fixture('config_fail');
    equal(barView(bad).box.ledColor, '#f8e45c');
    equal(popupView(bad, NOW).path.off, 'config 1');
});

test('no verdict (not_this_dac, other_input, player_unknown): a ring, the tint stays, the words show', () => {
    const words = {not_this_dac: '\u2192 Monitor HDMI', other_input: 'the DAC listens to Optical', player_unknown: ''};
    for (const [name, off] of Object.entries(words)) {
        const [view] = fixture(name);
        equal(barView(view).box, {outline: '#cfcfcf', led: 'ring', ledColor: '#8a8a8a'}, name);
        const path = popupView(view, NOW).path;
        equal(path.off, off, name);
        equal(path.offMarkup, off, name);
    }
    const [unknown] = fixture('player_unknown');
    equal(popupView(unknown, NOW).path.segments.map(s => s.color), ['#666666', '#8ff0a4', '#8ff0a4']);
});

test('no verdict: a ring in grey, the tint stays', () => {
    const none = patched(untouched, {path: {tone: 'plain'}});
    equal(barView(none).box, {outline: '#cfcfcf', led: 'ring', ledColor: '#8a8a8a'});
    equal(barView({...untouched, path: null}).box.led, 'ring');
});

test('another rate that has no tint is drawn plain', () => {
    equal(barView(patched(untouched, {device: {rate_hz: 32000}})).box.outline, null);
});

test('the popup header: name, rate, grey signal bars and dBm', () => {
    const header = popupView(untouched, NOW).header;
    equal(header.name, 'Bifrost 3');
    equal(header.rate.markup, `${gapped('44.', '1')}<span size="70%" rise="-2196" alpha="90%">kHz</span>`);
    equal(header.signal.markup, `${gapped('\u22125', '8')}<span size="70%" rise="-2196" alpha="90%">dBm</span>`);
    equal(header.signal.glyph.color, '#9a9a9a');
});

test('the popup volume row: slider position and the figure with its unit', () => {
    const vol = popupView(dacChanged, NOW).volume;
    equal(vol.muted, false);
    equal(vol.textMarkup, `${gapped('\u221224.', '1')}<span size="70%" rise="-2365" alpha="90%">dB</span>`);
    ok(Math.abs(vol.fraction - (96.33 - 24.082) / 96.33) < 1e-9, 'the fraction');
    equal(dbText(-24.1), vol.textMarkup);
});

test('slider mapping: fraction to dB at one decimal, clamped, never -0', () => {
    const v = {db: -50, min_db: -99, max_db: 0};
    equal(sliderFraction(v), 49 / 99);
    equal(dbFromFraction(0, v), -99);
    equal(dbFromFraction(1, v), 0);
    equal(dbFromFraction(2, v), 0);
    equal(dbFromFraction(0.5, v), -49.5);
    equal(Object.is(dbFromFraction(0.9999999, v), -0), false);
    equal(sliderFraction({db: null, min_db: -99, max_db: 0}), null);
    equal(sliderFraction({db: -1, min_db: null, max_db: 0}), null);
});

test('rows: input, computer EQ and scenes as the fixtures give them', () => {
    const pop = popupView(untouched, NOW);
    equal(pop.input.value, untouched.device.input.on);
    equal(pop.input.choices.map(c => c.run), untouched.device.input.choices.map(c => c.run));
    equal(pop.input.choices.filter(c => c.on).length, 1);
    equal(pop.computerEq.on, false);
    equal(pop.computerEq.choices.map(c => c.run), untouched.computer_eq.choices.map(c => c.run));
    const [several] = fixture('several_scenes');
    const scenes = popupView(several, NOW).scenes;
    equal(scenes.on, 'Night');
    equal(scenes.choices.length, 3);
    equal(scenes.choices.filter(c => c.on).map(c => c.name), ['Night']);
});

test('no computer EQ and no device', () => {
    const [noEq] = fixture('no_computer_eq');
    equal(popupView(noEq, NOW).computerEq, null);
    equal(popupView(noEq, NOW).path, null);
    const [none] = fixture('no_device');
    equal(barView(none).away, true);
    const pop = popupView(none, NOW);
    equal(pop.header.name, 'svolder');
    equal(pop.note, 'no device yet: svolder setup');
    equal(pop.scenes.choices, []);
});

test('the Path strip: tones to colours, the off-only words in markup, nothing when untouched', () => {
    const path = popupView(dacChanged, NOW).path;
    equal(path.segments.map(s => s.color), ['#8ff0a4', '#8ff0a4', '#f8e45c']);
    equal(path.offMarkup, 'DAC: volume \u221224.1<span size="70%" rise="-2028" alpha="90%">dB</span> \u00b7 tone control');
    const clean = popupView(untouched, NOW).path;
    equal(clean.off, '');
    equal(clean.offMarkup, '');
    equal(clean.segments.map(s => s.color), ['#8ff0a4', '#8ff0a4', '#8ff0a4']);
    const unknown = patched(untouched, {path: {segments: [
        {hop: 'player', tone: 'unknown'}, {hop: 'computer', tone: 'good'}, {hop: 'dac', tone: 'good'}]}});
    equal(popupView(unknown, NOW).path.segments[0].color, '#666666');
    equal(popupView({...untouched, path: null}, NOW).path, null);
});

test('away hides the DAC rows and keeps the computer EQ and the scenes', () => {
    const pop = popupView(away, NOW);
    equal([pop.volume, pop.input, pop.path], [null, null, null]);
    ok(pop.computerEq !== null && pop.scenes !== null, 'host rows stay');
    equal(pop.header.signal.markup, 'away');
});

test('names with markup characters are escaped, never drawn as markup', () => {
    const evil = patched(untouched, {
        device: {name: 'A<b>&"\'>'},
        scenes: {choices: [{name: '<i>x</i>', run: ['scene', 'load', '<i>x</i>']}]},
    });
    const pop = popupView(evil, NOW);
    equal(pop.header.nameMarkup, '<b>A&lt;b&gt;&amp;&quot;&apos;&gt;</b>');
    equal(offMarkup('DAC: <x> & y'), 'DAC: &lt;x&gt; &amp; y');
    equal(pop.scenes.choices[0].name, '<i>x</i>');
});

test('a failure shows only the failure', () => {
    const pop = popupView(null, NOW, {headline: 'the svolder daemon is not running', detail: 'x'});
    equal(pop.failure.headline, 'the svolder daemon is not running');
    equal([pop.header, pop.volume, pop.scenes], [null, null, null]);
});

test('age text: seconds, minutes, hours, days', () => {
    equal(ageText(1000, 1000), 'just now');
    equal(ageText(0, 30000), '30 s ago');
    equal(ageText(0, 180000), '3 min ago');
    equal(ageText(0, 7300000), '2 h ago');
    equal(ageText(0, 200000000), '2 d ago');
    equal(ageText(5000, 1000), 'just now');
});

test('parseLine: a line of the stream, garbage, another version', () => {
    const line = JSON.stringify(untouched);
    equal(parseLine(line).view.device.name, 'Bifrost 3');
    equal(parseLine('not json').view, null);
    equal(parseLine('7').view, null);
    const other = parseLine(JSON.stringify({...untouched, v: 2}));
    equal(other.view, null);
    ok(other.failure.headline.includes('panel version'), 'says why');
});

run('model');
