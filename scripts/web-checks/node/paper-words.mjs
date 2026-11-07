/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for the theme Paper's words (themes/paper/words.js): the title's line about the
// device, what a guest's and a read-only page's title says, the footer's date, and that every
// fixed word is written in characters Paper's fonts carry. The pages are the fixtures'. Run:
// node paper-words.mjs
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { SECTIONS } from '../../../bins/svolderd/web/themes/paper/draw/sheet.js';
import { computerView, modelOf, pageWith, wordsOf } from './paper-rig.mjs';
import {
  CHARS, WORDS, dateWords, titleWords,
} from '../../../bins/svolderd/web/themes/paper/words.js';

let n = 0;
const ok = (name, fn) => { fn(); n++; console.log('ok', name); };

const fixture = JSON.parse(readFileSync(new URL('./fixtures/ember-parts.json', import.meta.url), 'utf8'));
const byName = Object.fromEntries(fixture.cases.map((c) => [c.name, c.page]));
const clone = (value) => JSON.parse(JSON.stringify(value));
const title = (name) => titleWords(byName[name], byName[name].device);

ok('the_title_says_kind_link_input_and_rate', () => {
  const dac = title('dac');
  assert.equal(dac.name, 'Gungnir 2');
  assert.ok(dac.sub.startsWith('DAC, connected'), dac.sub);
  assert.ok(dac.sub.includes(' over USB 1'), dac.sub);
  assert.ok(dac.sub.includes(' at 48 kHz'), dac.sub);
  assert.ok(!dac.sub.includes(' · '), 'a signed-in writer has nothing to add');
  // No sample-rate readout: the DAC hop of the signal path says the rate.
  const page = byName.playing;
  const bare = {
    ...page.device,
    controls: page.device.controls.filter((c) => c.key !== 'sample_rate'),
  };
  assert.ok(page.device.controls.some((c) => c.key === 'sample_rate'), 'the fixture has one');
  const path = titleWords(page, bare);
  assert.ok(path.sub.endsWith(' at 48 kHz'), path.sub);
  assert.ok(path.sub.includes(' over USB 1'), path.sub);
  // A device that is not connected says so in the same place.
  assert.ok(title('lost').sub.startsWith('DAC, '), title('lost').sub);
  // No device at all: the title is the program's own name and says nothing else.
  assert.deepEqual(titleWords(byName['no-devices'], null), { name: 'svolder', sub: '' });
});

ok('a_rate_keeps_its_decimals_and_a_retry_at_nought_says_now', () => {
  const page = byName.playing;
  const bare = clone({
    ...page.device,
    controls: page.device.controls.filter((c) => c.key !== 'sample_rate'),
  });
  bare.playing.path.hops.find((h) => h.label === 'DAC').detail = '44.1 kHz, PCM';
  assert.ok(titleWords(page, bare).sub.endsWith(' at 44.1 kHz'), titleWords(page, bare).sub);
  assert.equal(WORDS.retryIn(0), 'trying again now');
  assert.equal(WORDS.retryIn(3), 'trying again in 3 s');
});

ok('a_guest_and_a_read_only_page_say_so_in_the_title', () => {
  assert.ok(title('dac-guest').sub.endsWith(' · read only: sign in to change this'),
    title('dac-guest').sub);
  assert.ok(title('dac-readonly').sub.endsWith(' · Read-only: [web] writes is off'),
    title('dac-readonly').sub);
  assert.equal(titleWords(byName['dac-guest'], null).sub, 'read only: sign in to change this');
});

ok('the_date_is_day_and_month', () => {
  assert.equal(dateWords(Date.UTC(2026, 9, 30, 12)), '30 October');
  assert.equal(dateWords(Date.UTC(2027, 0, 5, 12)), '5 January');
});

ok('paper_writes_only_the_characters_its_fonts_carry', () => {
  const strings = Object.entries(WORDS).filter(([, v]) => typeof v === 'string');
  assert.ok(strings.length >= 8, `${strings.length} fixed words`);
  for (const [key, text] of strings) assert.match(text, CHARS, `WORDS.${key}: ${text}`);
  assert.match(WORDS.keyHint({ key: 'T', words: 'Themes' }), CHARS);
  assert.equal(WORDS.keyHint({ key: 'T', words: 'Themes' }), 'T for themes');
  // The hand writes `to` for an arrow and the true minus sign for a minus.
  assert.equal(WORDS.handArrow, 'to');
  assert.equal(WORDS.handMinus, '−');
  assert.equal(WORDS.reconnecting, 'Reconnecting to svolder…');
  for (const bad of ['→', '✓', '⏮', '⏭', '▾', '\u{1F600}']) {
    assert.doesNotMatch(`a${bad}`, CHARS, `U+${bad.codePointAt(0).toString(16)} is not carried`);
  }
});

ok('every_word_passes_papers_chars', () => {
  // the computer's fixed words, the functions' words for a few values, and every word the
  // flipped sheet draws (the marks of the table, the line, the notes) in both kinds of device
  const said = [WORDS.showing, WORDS.theDevice('DAC'), WORDS.theDevice('amp'), WORDS.thisComputer,
    ...WORDS.pointHeads, WORDS.removeHead, WORDS.addPoint, WORDS.typeOf(3), WORDS.cellOf(3, 'f'),
    WORDS.cellOf(3, 'g'), WORDS.cellOf(3, 'q'), WORDS.removePoint(4), WORDS.moreBelow(1),
    WORDS.moreBelow(10), WORDS.typeList, WORDS.notConfirmed, WORDS.isOn, WORDS.noPresetOn, WORDS.byHand];
  for (const text of said) assert.match(text, CHARS, text);
  assert.equal(WORDS.addPoint, '+ Add point');
  assert.equal(WORDS.removePoint(4), 'Remove point 4');
  for (const name of ['dac', 'amp']) {
    const page = pageWith(name, modelOf());
    const { view } = computerView(page, 'computer');
    for (const section of ['title', 'volume', 'tone', 'presets', 'notes']) {
      const text = wordsOf(SECTIONS[section](page, page.device, view));
      // the unavailable sentence is the daemon's own; the others are Paper's
      assert.match(text.replace(/\s+/g, ' '), CHARS, `${name} ${section}`);
    }
  }
});

console.log(`paper-words: ${n} passed`);
