/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for the theme Paper's tone (themes/paper/draw/tone.js) and for what every drawn
// section owes a guest: the tone section drawn from the fixtures' pages (the points at the bands'
// markers, the curve dashed in Flat, the heading's mode and preset, the loudness strength and the
// band table's steppers), no `style` attribute anywhere, and no underlined word on a guest's sheet.
// The markup is read with a small tag reader, no DOM. Run: node paper-draw.mjs
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { responseDb, logFreqs, POINTS, yOf } from '../../../bins/svolderd/web/core/tone-curve.js';
import { SECTIONS } from '../../../bins/svolderd/web/themes/paper/draw/sheet.js';
import {
  curvePath, heldGraph, pointsOf,
} from '../../../bins/svolderd/web/themes/paper/draw/tone.js';

let n = 0;
const ok = (name, fn) => { fn(); n++; console.log('ok', name); };

const fixture = JSON.parse(readFileSync(new URL('./fixtures/ember-parts.json', import.meta.url), 'utf8'));
const clone = (value) => JSON.parse(JSON.stringify(value));
const pageOf = (name) => clone(fixture.cases.find((c) => c.name === name).page);
const view = { phone: false, coarse: false, state: 'open', now: 0, rulerPx: 1000 };
const toneOf = (name) => {
  const page = pageOf(name);
  return { page, html: SECTIONS.tone(page, page.device, view), tone: page.device.tone };
};
const MINUS = '−';

/** The start tags of `html` named `tag`, each as its attributes (and `text`: what follows up to the
 * next tag). */
function tags(html, tag) {
  const out = [];
  const open = new RegExp(`<${tag}(\\s[^>]*)?>([^<]*)`, 'g');
  for (const m of html.matchAll(open)) {
    const attrs = { text: m[2] };
    for (const a of (m[1] ?? '').matchAll(/([\w-]+)="([^"]*)"/g)) attrs[a[1]] = a[2];
    out.push(attrs);
  }
  return out;
}
const unescape = (text) => text.replace(/&quot;/g, '"').replace(/&#39;/g, '\'').replace(/&lt;/g, '<')
  .replace(/&gt;/g, '>').replace(/&amp;/g, '&');

ok('the_points_sit_at_each_bands_marker', () => {
  const { html, tone } = toneOf('dac');
  const points = tags(html, 'button').filter((b) => (b.class ?? '').split(' ').includes('pt'));
  assert.equal(points.length, tone.markers.length);
  const want = pointsOf(tone);
  assert.equal(want.length, tone.markers.length);
  points.forEach((p, i) => {
    assert.equal(Number(p['data-band']), want[i].band);
    assert.ok(Math.abs(Number(p['data-x']) - want[i].x) < 1e-5, `x of ${i}`);
    assert.ok(Math.abs(Number(p['data-y']) - want[i].y) < 1e-5, `y of ${i}`);
    assert.equal(p.role, 'slider');
    assert.match(p['aria-label'], /^Band \d: /);
  });
  assert.ok(points[0]['aria-label'].includes('40.0 Hz') && points[0]['aria-label'].includes('3.10 dB'));
  const m = tone.markers[1];
  assert.ok(Math.abs(want[1].y - yOf(m.db) / 260) < 1e-9);
});

ok('a_band_not_read_in_full_draws_no_point', () => {
  const { html, tone } = toneOf('dac-band-unread');
  const points = tags(html, 'button').filter((b) => (b.class ?? '').split(' ').includes('pt'));
  assert.equal(points.length, tone.markers.length);
  assert.equal(points.length, 4);
  assert.ok(!points.some((p) => p['data-band'] === '3'), 'band 3 has no Q reading');
});

ok('the_curve_is_dashed_in_flat', () => {
  const flat = toneOf('dac-flat').html;
  assert.ok(/<path class="p-curve faint"/.test(flat), flat);
  assert.ok(/<path class="p-curve"/.test(toneOf('dac').html));
  assert.ok(toneOf('dac').html.includes('class="p-zero"'));
});

ok('the_curve_path_is_the_responses_line_in_the_plot', () => {
  const { tone } = toneOf('dac');
  const d = curvePath(tone.response);
  assert.ok(d.startsWith('M0.0 ') && d.split('L').length === 181, d.slice(0, 60));
});

ok('the_heading_names_the_mode_and_the_preset', () => {
  const { html } = toneOf('dac-user-preset');
  const heading = /<h2 class="p-label">([\s\S]*?)<\/h2>/.exec(html)[1];
  assert.ok(heading.startsWith('Tone &middot; ') || heading.startsWith('Tone · '), heading);
  const opens = tags(heading, 'button');
  assert.deepEqual(opens.map((b) => b['data-act']), ['mode-list', 'preset-list']);
  assert.equal(opens[0].text, 'EQ');
  assert.equal(unescape(opens[1].text), 'Mine "A" & <B>');
  assert.ok(!html.includes('Mine "A"'), 'the name is escaped');
  assert.equal(tags(toneOf('dac-flat').html, 'button').find((b) => b['data-act'] === 'preset-list').text,
    'no preset');
  // an edited preset reads as the Presets section says it
  assert.equal(tags(toneOf('dac-edited-preset').html, 'button').find((b) => b['data-act'] === 'preset-list').text,
    'Warm, edited');
  assert.equal(tags(toneOf('dac').html, 'button').find((b) => b['data-act'] === 'preset-list').text, 'Warm');
  const lo = tags(toneOf('dac-loudness').html, 'button').find((b) => b['data-act'] === 'mode-list');
  assert.equal(lo.text, 'Loudness');
});

ok('the_loudness_strength_has_its_two_words', () => {
  const { html } = toneOf('dac-loudness-strength');
  const loud = tags(html, 'button').filter((b) => b['data-act'] === 'loudness');
  assert.deepEqual(loud.map((b) => [b['data-step'], b.text]), [['-1', MINUS], ['1', '+']]);
  assert.ok(html.includes('strength') && />40</.test(html), html);
  assert.ok(!toneOf('dac-loudness').html.includes('data-act="loudness"'));
  assert.ok(!toneOf('dac').html.includes('data-act="loudness"'));
});

ok('every_band_parameter_has_its_two_steppers', () => {
  const { html, tone } = toneOf('dac');
  const steppers = tags(html, 'button').filter((b) => b['data-act'] === 'band');
  assert.equal(steppers.length, tone.bands.rows.length * 3 * 2);
  for (const row of tone.bands.rows) {
    for (const param of ['f', 'g', 'q']) {
      const two = steppers.filter((b) => b['data-band'] === String(row.band) && b['data-param'] === param);
      assert.deepEqual(two.map((b) => [b['data-step'], b.text]), [['-1', MINUS], ['1', '+']]);
    }
  }
  assert.ok(html.includes('id="p-bands"') && html.includes('40.0 Hz') && html.includes('+3.10 dB'));
  for (const head of ['band', 'frequency', 'gain', 'Q']) assert.ok(html.includes(`>${head}<`), head);
});

ok('a_held_graph_is_drawn_from_the_held_band', () => {
  const page = pageOf('dac');
  const { tone, tone_steps: steps } = page.device;
  const raws = [...tone.readings.bands[1]];
  raws[1] += 20;
  const held = heldGraph(tone, steps, 2, { f: raws[0], g: raws[1], q: raws[2] });
  const bands = tone.readings.bands.map((r, i) => ({
    f0: steps.bands[i].f[(i === 1 ? raws : r)[0]][0],
    gainDb: steps.gain[(i === 1 ? raws : r)[1]][0],
    q: steps.q[(i === 1 ? raws : r)[2]][0],
  }));
  const ys = responseDb(bands, logFreqs(POINTS)).map(yOf);
  assert.ok(held.d.startsWith('M0.0 '));
  assert.equal(held.d.split('L').length, ys.length);
  assert.ok(Math.abs(held.point.y - yOf(bands[1].gainDb) / 260) < 1e-9);
  assert.notEqual(held.d, curvePath(tone.response));
  assert.equal(heldGraph(tone, steps, 3, { f: 64, g: 98, q: null }), null, 'a band not in full');
});

const CONTROLLED = ['dac', 'lokimax', 'saga', 'phono', 'five', 'mono'];
const classesOf = (t) => (t.class ?? '').split(' ');

ok('every_control_of_every_fixture_has_its_written_word', () => {
  let checked = 0;
  for (const name of CONTROLLED) {
    const page = pageOf(name);
    const html = SECTIONS.controls(page, page.device, view);
    const found = { button: tags(html, 'button'), div: tags(html, 'div'), span: tags(html, 'span') };
    for (const c of page.device.controls ?? []) {
      checked++;
      const at = `${name} ${c.key}`;
      const mine = (list, act) => list.filter((t) => t['data-key'] === c.key && t['data-act'] === act);
      if (c.kind === 'choice' && c.style === 'list') {
        const open = mine(found.button, 'choice-list');
        assert.equal(open.length, 1, at);
        assert.equal(unescape(open[0].text), c.options.find((o) => o.on)?.label ?? '—', at);
      } else if (c.kind === 'choice') {
        const words = mine(found.button, 'choice');
        assert.deepEqual(words.map((t) => unescape(t['data-value'])), c.options.map((o) => o.label), at);
        words.forEach((t, i) => assert.equal(classesOf(t).includes('on'), c.options[i].on === true, at));
      } else if (c.kind === 'toggle') {
        const word = mine(found.button, 'toggle');
        assert.equal(word.length, 1, at);
        assert.equal(word[0].text, c.on === null || c.on === undefined ? '—' : c.on ? 'on' : 'off', at);
      } else if (c.kind === 'slider') {
        const scale = found.div.filter((t) => t['data-key'] === c.key && classesOf(t).includes('p-scale'));
        assert.equal(scale.length, 1, at);
        assert.equal(scale[0].role, 'slider', at);
        assert.equal(scale[0].tabindex, '0', at);
        assert.equal(scale[0]['aria-valuemin'], String(c.lo), at);
        assert.equal(scale[0]['aria-valuemax'], String(c.hi), at);
        assert.equal('aria-valuenow' in scale[0], c.raw !== null && c.raw !== undefined, at);
        const marks = found.span.filter((t) => classesOf(t).includes('p-mark')
          && t['data-key'] === c.key);
        assert.equal(marks.length, c.raw === null || c.raw === undefined ? 0 : 1, at);
        if (marks[0]) {
          assert.ok(Math.abs(Number(marks[0]['data-at']) - (c.raw - c.lo) / (c.hi - c.lo)) < 1e-4, at);
        }
        const centre = mine(found.button, 'centre');
        assert.equal(centre.length, c.centre === null || c.centre === undefined ? 0 : 1, at);
        if (centre[0]) assert.equal(unescape(centre[0].text), c.text, at);
      } else {
        const read = found.span.filter((t) => t['data-readout'] === c.key);
        assert.equal(read.length, 1, at);
        assert.equal(unescape(read[0].text), c.text, at);
        assert.equal(classesOf(read[0]).includes('faint'), c.tone === 'faint', at);
      }
    }
  }
  assert.ok(checked > 20, `${checked} controls`);
});

ok('the_presets_offer_save_rename_and_delete_only_when_editable', () => {
  const words = (html) => tags(html, 'button').map((b) => b['data-act']);
  const mine = pageOf('dac-user-preset');
  const html = SECTIONS.presets(mine, mine.device, view);
  assert.ok(html.includes('>Presets<'), html);
  const names = tags(html, 'button').filter((b) => b['data-act'] === 'preset');
  assert.deepEqual(names.map((b) => unescape(b['data-name'])),
    mine.device.presets.rows.map((r) => r.name));
  assert.equal(names.filter((b) => classesOf(b).includes('on')).length, 1);
  assert.ok(classesOf(names[4]).includes('own') && !classesOf(names[0]).includes('own'));
  assert.ok(!html.includes('Mine "A"'), 'the name is escaped');
  assert.ok(/class="p-status"[^>]*>On: <i>/.test(html), html);
  assert.deepEqual(words(html).slice(-2), ['preset-save', 'preset-edit']);
  assert.equal(tags(html, 'button').find((b) => b['data-act'] === 'preset-save').text, 'save as…');
  assert.equal(tags(html, 'button').find((b) => b['data-act'] === 'preset-edit').text,
    'rename or delete…');
  // only built-in presets: nothing to rename or delete
  const plain = pageOf('dac');
  const some = words(SECTIONS.presets(plain, plain.device, view));
  assert.ok(some.includes('preset-save') && !some.includes('preset-edit'), some.join());
  // a configuration that cannot be written keeps the recall and says why
  const off = pageOf('dac-presets-off');
  const none = SECTIONS.presets(off, off.device, view);
  assert.ok(words(none).includes('preset') && !words(none).includes('preset-save'), none);
  assert.ok(!words(none).includes('preset-edit'));
  assert.ok(none.includes('class="p-ronote"') && none.includes('Saving presets is off'), none);
  assert.ok(!html.includes('p-ronote'));
});

ok('the_device_presets_offer_slots_save_and_built_ins', () => {
  const page = pageOf('lokimax');
  const d = page.device.device_presets;
  const html = SECTIONS['device-presets'](page, page.device, view);
  assert.ok(html.includes('>On the device<') && html.includes('>Built in<'), html);
  const slots = tags(html, 'button').filter((b) => b['data-act'] === 'slot');
  assert.deepEqual(slots.map((b) => [Number(b['data-slot']), b.text]), d.slots.map((s) => [s.slot, s.name]));
  const built = tags(html, 'button').filter((b) => b['data-act'] === 'setting');
  assert.deepEqual(built.map((b) => b['data-name']), d.builtins.map((b) => b.name));
  assert.equal(tags(html, 'button').filter((b) => b['data-act'] === 'slot-save-list').length, 1);
  assert.equal(tags(html, 'button').find((b) => b['data-act'] === 'slot-save-list').text,
    'save the bands to…');
  const lit = pageOf('lokimax-flat');
  const flat = SECTIONS['device-presets'](lit, lit.device, view);
  assert.equal(tags(flat, 'button').filter((b) => b['data-act'] === 'setting' && classesOf(b).includes('on'))
    .length, 1);
  const guest = pageOf('lokimax-guest');
  const shown = SECTIONS['device-presets'](guest, guest.device, view);
  assert.ok(!shown.includes('slot-save-list') && !shown.includes('save the bands'), shown);
  assert.ok(shown.includes('data-act="slot"') && shown.includes('data-act="setting"'));
  const dac = pageOf('dac');
  assert.equal(SECTIONS['device-presets'](dac, dac.device, view), '');
});

const sectionOf = (name, section) => {
  const page = pageOf(name);
  return { page, device: page.device, html: SECTIONS[section](page, page.device ?? null, view) };
};

ok('every_fact_is_on_the_sheet', () => {
  let checked = 0;
  for (const c of fixture.cases) {
    const page = clone(c.page);
    const f = page.device?.facts;
    if (!f) continue;
    const html = SECTIONS.device(page, page.device, view);
    const facts = new Map(tags(html, 'td').filter((t) => 'data-fact' in t)
      .map((t) => [unescape(t['data-fact']), t]));
    const want = ['Product', 'Firmware', 'DSP', 'Profile', 'Link', 'Signal'];
    if (f.unit) want.push('Unit');
    if (f.passphrase_note !== null && f.passphrase_note !== undefined) want.push('Passphrase');
    for (const r of f.rows) want.push(r.id);
    for (const label of want) {
      checked++;
      assert.ok(facts.has(label), `${c.name}: ${label}`);
    }
    assert.equal(facts.has('Unit'), Boolean(f.unit), `${c.name}: Unit only when present`);
    assert.equal(facts.has('Passphrase'), f.passphrase_note !== null && f.passphrase_note !== undefined);
    assert.equal(unescape(facts.get('Product').text), f.product ?? `product ${f.product_id}`, c.name);
    assert.equal(unescape(facts.get('DSP').text), f.dsp == null ? '—' : String(f.dsp), c.name);
    assert.ok(html.includes('>The device<') && html.includes('id="p-spec"') && html.includes('>All readings<'));
    const ident = tags(html, 'button').filter((b) => b['data-act'] === 'identify');
    assert.equal(ident.length, f.identify_offered ? 1 : 0, c.name);
    assert.equal(html.includes('Asks the unit to show itself, so you can tell it from the others.'),
      Boolean(f.identify_offered), c.name);
  }
  assert.ok(checked > 150, `${checked} facts`);
  const heard = pageOf('dac-facts-heard');
  const hh = SECTIONS.device(heard, heard.device, view);
  assert.ok(/class="p-age"[^>]*data-heard-at="1700000010000"/.test(hh), 'heard age');
  assert.ok(hh.includes('product 9'), 'a product the app does not name');
});

ok('the_amp_draws_power_and_a_needle_per_meter', () => {
  for (const name of ['amp', 'five', 'mono']) {
    const { device, html } = sectionOf(name, 'meters');
    const marks = tags(html, 'span').filter((t) => classesOf(t).includes('p-mark'));
    assert.equal(marks.length, device.meters.meters.length, name);
    const zones = tags(html, 'div').filter((t) => classesOf(t).includes('p-zone') && classesOf(t).includes('classa'));
    assert.deepEqual(zones.map((z) => Number(z['data-at'])), device.meters.meters.map((m) => m.class_a), name);
    device.meters.meters.forEach((m, i) => {
      if (m.fraction === null) assert.ok(!('data-at' in marks[i]), `${name} ${m.key}: no needle`);
      else assert.ok(Math.abs(Number(marks[i]['data-at']) - m.fraction) < 1e-4, `${name} ${m.key}`);
      assert.ok(html.includes(`>${m.channel}<`) && html.includes(`>${m.text}<`), `${name} ${m.key}`);
    });
    assert.equal(tags(html, 'div').filter((t) => classesOf(t).includes('meter')).length, device.meters.meters.length);
    assert.ok(html.includes('>Meters<'));
  }
  const amp = sectionOf('amp', 'power');
  assert.ok(amp.html.includes('Power: '), amp.html);
  const pw = tags(amp.html, 'button').find((b) => b['data-act'] === 'power');
  assert.equal(pw['data-key'], 'power');
  assert.equal(pw.text, 'on');
  assert.equal(tags(sectionOf('amp-standby', 'power').html, 'button').find((b) => b['data-act'] === 'power').text, 'off');
  assert.ok(tags(amp.html, 'button').some((b) => b['data-act'] === 'toggle' && b['data-key'] === 'power_meters'));
  assert.ok(!amp.html.includes('Warming up'), 'readouts are notes, not lines');
  assert.ok(/<i>warming up<\/i>/i.test(sectionOf('amp-warming', 'power').html));
  assert.ok(!/<i>/.test(amp.html));
  const state = sectionOf('amp-warming', 'meters').html;
  assert.ok(/<p class="p-mstate"><i>Warming up<\/i><\/p>/.test(state), state);
  assert.equal(sectionOf('dac', 'power').html, '');
  assert.equal(sectionOf('dac', 'meters').html, '');
});

ok('a_lost_link_draws_the_card_as_the_red_note_with_try_now', () => {
  const { html } = sectionOf('lost', 'card');
  const note = tags(html, 'p').find((p) => classesOf(p).includes('pn'));
  assert.deepEqual(classesOf(note), ['pn', 'red', 'card']);
  assert.equal(note.text, 'Gungnir 2 is out of reach');
  const tryNow = tags(html, 'button').filter((b) => b['data-act'] === 'try');
  assert.equal(tryNow.length, 1);
  assert.equal(tryNow[0].text, 'try now');
  const retry = tags(html, 'span').find((s) => classesOf(s).includes('p-retry'));
  assert.equal(retry.text, 'trying again in 8 s');
  assert.equal(retry['data-secs'], '8');
  const looking = sectionOf('looking', 'card').html;
  assert.ok(looking.includes('Looking for Desk &quot;DAC&quot; &amp; &lt;amp&gt;'), looking);
  assert.ok(!sectionOf('refused', 'card').html.includes('data-act="try"'));
  const guest = pageOf('lokimax-guest');
  guest.device.card = clone(pageOf('lost').device.card);
  const g = SECTIONS.card(guest, guest.device, view);
  assert.ok(/data-act="try"[^>]*aria-disabled="true"/.test(g), g);
  const detail = sectionOf('no-devices', 'card').html;
  assert.ok(detail.includes('<code>svolder setup</code>'), detail);
});

ok('faults_are_listed_under_the_red_note', () => {
  const { device, html } = sectionOf('fault', 'card');
  const note = tags(html, 'p').find((p) => classesOf(p).includes('red'));
  assert.equal(note.text, '2 faults');
  assert.equal(tags(html, 'li').length, device.card.faults.length);
  for (const f of device.card.faults) {
    assert.ok(html.includes(`>${f.label}<`) && html.includes(f.advice), f.label);
  }
  assert.ok(html.indexOf('class="pn red card"') < html.indexOf('<ul'));
  assert.ok(!html.includes('data-act="try"') && !html.includes('p-retry'));
});

ok('no_device_draws_only_the_card', () => {
  for (const name of ['no-devices', 'unavailable']) {
    const page = pageOf(name);
    assert.equal(page.device, null);
    const drawn = Object.entries(SECTIONS).map(([k, draw]) => [k, draw(page, null, view)])
      .filter(([, html]) => html !== '').map(([k]) => k);
    assert.deepEqual(drawn, ['title', 'card', 'foot'], `${name}: ${drawn}`);
    const html = SECTIONS.card(page, null, view);
    assert.ok(html.includes('class="pn red card"'), name);
  }
});

ok('the_transport_is_drawn_for_a_writer_only', () => {
  const { device, html } = sectionOf('playing', 'playing');
  assert.ok(html.includes('>Playing<'));
  assert.ok(html.includes(`<i>${device.playing.track.headline}</i>, ${device.playing.track.artist}`), html);
  const words = tags(html, 'button').filter((b) => b['data-act'] === 'transport');
  assert.deepEqual(words.map((b) => [b['data-button'], b.text]),
    [['previous', 'back'], ['pause', 'pause'], ['next', 'next']]);
  const paused = sectionOf('playing-bare-amp', 'playing');
  assert.deepEqual(tags(paused.html, 'button').map((b) => [b['data-button'], b.text]),
    [['previous', 'back'], ['play', 'play'], ['next', 'next']]);
  assert.ok(!paused.html.includes('</i>,'), 'artist omitted when empty');
  const guest = sectionOf('playing-guest-other-sink', 'playing');
  assert.equal(tags(guest.html, 'button').length, 0, 'no buttons for a viewer without them');
  assert.ok(guest.html.includes('Tom &amp; &quot;Jerry&quot; &lt;live&gt;'), guest.html);
  const none = sectionOf('no-player', 'playing');
  assert.ok(/<i>nothing playing<\/i>/.test(none.html) && tags(none.html, 'button').length === 0, none.html);
  assert.equal(sectionOf('dac', 'playing').html, '');
});

ok('no_markup_carries_a_style_attribute', () => {
  let drawn = 0;
  for (const c of fixture.cases) {
    const page = clone(c.page);
    for (const [name, draw] of Object.entries(SECTIONS)) {
      const html = draw(page, page.device ?? null, view);
      drawn++;
      assert.ok(!/\sstyle\s*=/.test(html), `${c.name} ${name}`);
      assert.ok(!html.includes('→'), `${c.name} ${name}: no arrow`);
    }
  }
  assert.ok(drawn > 100);
});

ok('a_guests_sheet_has_no_underlined_word', () => {
  const cases = fixture.cases.filter((c) => /-guest$|^dac-readonly$/.test(c.name));
  assert.ok(cases.length >= 5, `${cases.length} guest cases`);
  let words = 0;
  for (const c of cases) {
    const page = clone(c.page);
    if (!page.device) continue;
    for (const [name, draw] of Object.entries(SECTIONS)) {
      const html = draw(page, page.device, view);
      for (const b of tags(html, 'button').filter((t) => 'data-act' in t)) {
        words++;
        assert.equal(b['aria-disabled'], 'true', `${c.name} ${name} ${b['data-act']}`);
        assert.ok((b.class ?? '').split(' ').includes('quiet'), `${c.name} ${name} ${b['data-act']}`);
      }
    }
  }
  assert.ok(words > 20, `${words} words checked`);
  const { html } = toneOf('dac-guest');
  assert.ok(html.includes('data-act="mode-list"') && html.includes('data-act="band"'));
});

console.log(`paper-draw: ${n} passed`);
