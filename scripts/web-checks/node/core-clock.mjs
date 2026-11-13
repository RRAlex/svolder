/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for the ages' ticker of core/clock.js (`tickAges`), which every theme's Device
// section runs: the ages and the signal's age step as the clock steps, worked out on the daemon's
// clock the part carries; a row that left the page is touched no more; the timer runs only while
// the page is on show and goes when the ticker is stopped or its signal aborts (a theme's
// unmount). The page is jsdom's, the clock, the timers and the watch of the page are fakes.
// Run: node core-clock.mjs
import assert from 'node:assert/strict';
import { tickAges } from '../../../bins/svolderd/web/core/clock.js';
import { page } from './dom.mjs';

let n = 0;
const ok = (name, fn) => { fn(); n++; console.log('ok', name); };

/** The daemon's clock when the part was made, far from the browser's own. */
const DAEMON_NOW = 1_700_000_000_000;
/** The browser's clock at the start of a case. */
const LOCAL_START = 5_000;

/** A Device section as a theme draws one: a table that carries the daemon's clock and the signal,
 * and the reading rows, each with the stamp it was heard at. */
const section = ({ now = DAEMON_NOW, live = false, heard = [5, 100, 7200] } = {}) => `
  <div id="facts">
    <table id="spec" data-now="${now}"><tbody>
      <tr><th>Signal</th><td data-fact="Signal" data-head="−62 dBm advert" data-heard-at="${now - 12_000}"${live ? ' data-live' : ''}>drawn</td></tr>
      <tr><th>Firmware</th><td data-fact="Firmware">1.2</td></tr>
    </tbody></table>
    <table id="readings"><tbody>
      ${heard.map((secs, i) => `<tr id="row${i}"><td class="id">12.${i}</td><td class="val">v</td>`
    + `<td class="age" data-heard-at="${now - secs * 1000}">drawn</td></tr>`).join('')}
      <tr id="never"><td class="id">9</td><td class="val">v</td><td class="age">—</td></tr>
    </tbody></table>
  </div>`;

const SELECTORS = {
  stamp: '#spec',
  ages: '#readings td.age[data-heard-at]',
  signal: '#spec td[data-fact=Signal][data-heard-at]',
};

/** A ticker over a page whose body is `html`, with a clock the case moves (`at.ms`), timers it
 * fires (`fire()`: one turn of every running interval) and a watch of the page it shows and hides
 * (`show(on)`); `options` go to `tickAges` over the fakes. */
function rig(html = section(), options = {}, selectors = SELECTORS) {
  const { document } = page(html);
  const at = { ms: LOCAL_START };
  const running = new Map();
  let ids = 0;
  const timers = {
    setInterval(fn, ms) {
      running.set(++ids, { fn, ms });
      return ids;
    },
    clearInterval(id) {
      running.delete(id);
    },
  };
  const watches = [];
  const watchVisible = (given) => {
    const watch = { given, stopped: false, stop() { this.stopped = true; } };
    watches.push(watch);
    return watch;
  };
  const ticker = tickAges(document.body, selectors, {
    now: () => at.ms, timers, watchVisible, ...options,
  });
  return {
    document,
    at,
    running,
    watches,
    ticker,
    fire: () => [...running.values()].forEach(({ fn }) => fn()),
    show: (on) => watches.at(-1).given.onVisible(on),
    ages: () => [...document.querySelectorAll('#readings td.age')].map((cell) => cell.textContent),
    signal: () => document.querySelector('[data-fact=Signal]').textContent,
  };
}

ok('an_age_steps_as_the_clock_steps', () => {
  const r = rig();
  assert.deepEqual(r.ages(), ['drawn', 'drawn', 'drawn', '—'], 'nothing is written before the page is on show');
  r.show(true);
  assert.deepEqual(r.ages(), ['5 s ago', '1 min ago', '2 h ago', '—'], 'written at once when shown');
  assert.equal(r.signal(), '−62 dBm advert, 12 s ago');
  assert.equal(r.running.size, 1, 'one timer');
  assert.equal([...r.running.values()][0].ms, 1000, 'a second apart');
  r.at.ms += 1000;
  r.fire();
  assert.deepEqual(r.ages(), ['6 s ago', '1 min ago', '2 h ago', '—']);
  assert.equal(r.signal(), '−62 dBm advert, 13 s ago');
  r.at.ms += 20_000;
  r.fire();
  assert.deepEqual(r.ages(), ['26 s ago', '2 min ago', '2 h ago', '—'], '100 s and 21 s are 2 min');
  r.ticker.stop();
});

ok('the_ages_are_on_the_daemons_clock_whatever_the_browsers_says', () => {
  // the same part on a browser whose clock is a day off: the same ages
  const r = rig();
  r.at.ms = LOCAL_START + 86_400_000;
  r.show(true);
  assert.deepEqual(r.ages().slice(0, 3), ['5 s ago', '1 min ago', '2 h ago']);
  r.ticker.stop();
});

ok('a_cell_is_written_only_when_its_words_change', () => {
  const r = rig();
  r.show(true);
  // setting a cell's text makes a new text node: the node in place tells a write from none
  const same = r.document.querySelector('#row1 td.age');
  const stepped = r.document.querySelector('#row0 td.age');
  const [sameText, steppedText] = [same.firstChild, stepped.firstChild];
  r.at.ms += 1000;
  r.fire();
  assert.equal(same.textContent, '1 min ago');
  assert.equal(same.firstChild, sameText, 'the same words leave the cell as it is');
  assert.equal(stepped.textContent, '6 s ago');
  assert.notEqual(stepped.firstChild, steppedText, 'new words are written');
  r.ticker.stop();
});

ok('a_live_signal_says_its_age_only_from_half_a_minute', () => {
  const r = rig(section({ live: true }));
  r.show(true);
  assert.equal(r.signal(), '−62 dBm advert', 'live and 12 s old');
  r.at.ms += 18_000;
  r.fire();
  assert.equal(r.signal(), '−62 dBm advert, 30 s ago');
  r.ticker.stop();
});

ok('a_row_removed_from_the_page_is_no_longer_touched', () => {
  const r = rig();
  r.show(true);
  const gone = r.document.querySelector('#row0 td.age');
  r.document.getElementById('row0').remove();
  r.at.ms += 3000;
  r.fire();
  assert.equal(gone.textContent, '5 s ago', 'the cell that left keeps the words it had');
  assert.deepEqual(r.ages(), ['1 min ago', '2 h ago', '—'], 'the rows in place go on');
  // the whole section taken away: nothing to write, and nothing thrown
  const spec = r.document.getElementById('spec');
  const cells = [...r.document.querySelectorAll('#readings td.age')];
  r.document.getElementById('facts').remove();
  r.at.ms += 60_000;
  r.fire();
  assert.deepEqual(cells.map((cell) => cell.textContent), ['1 min ago', '2 h ago', '—']);
  assert.equal(spec.querySelector('[data-fact=Signal]').textContent, '−62 dBm advert, 15 s ago');
  r.ticker.stop();
});

ok('a_section_drawn_again_is_aged_from_its_own_stamp', () => {
  const r = rig();
  r.show(true);
  r.at.ms += 50_000;
  r.fire();
  assert.equal(r.ages()[0], '55 s ago');
  // a new part, made 50 s later by the daemon's clock, in the place of the old one
  r.document.body.innerHTML = section({ now: DAEMON_NOW + 50_000, heard: [2] });
  r.fire();
  assert.deepEqual(r.ages(), ['2 s ago', '—'], 'aged from the new part\'s clock, not the old arrival');
  r.at.ms += 1000;
  r.fire();
  assert.deepEqual(r.ages(), ['3 s ago', '—']);
  r.ticker.stop();
});

ok('stopping_it_stops_the_timer_and_the_watch', () => {
  const r = rig();
  r.show(true);
  assert.equal(r.running.size, 1);
  r.ticker.stop();
  assert.equal(r.running.size, 0, 'the timer is cleared');
  assert.equal(r.watches[0].stopped, true, 'the watch of the page is stopped');
  r.at.ms += 9000;
  r.ticker.tick();
  assert.equal(r.ages()[0], '5 s ago', 'a stopped ticker writes nothing more');
  // a late word from the watch starts nothing
  r.watches[0].given.onVisible(true);
  assert.equal(r.running.size, 0);
  r.ticker.stop();
});

ok('the_timer_runs_only_while_the_page_is_on_show', () => {
  const r = rig();
  assert.equal(r.running.size, 0, 'no timer before the page is on show');
  r.show(true);
  r.show(true);
  assert.equal(r.running.size, 1, 'shown twice is still one timer');
  r.show(false);
  assert.equal(r.running.size, 0, 'hidden: no timer');
  r.at.ms += 30_000;
  r.show(true);
  assert.equal(r.ages()[0], '35 s ago', 'back on show: written at once');
  assert.equal(r.running.size, 1);
  r.ticker.stop();
});

ok('an_aborted_signal_stops_it_as_a_themes_unmount_does', () => {
  const controller = new AbortController();
  const r = rig(section(), { signal: controller.signal });
  r.show(true);
  assert.equal(r.running.size, 1);
  controller.abort();
  assert.equal(r.running.size, 0, 'no timer is left behind for the next theme');
  assert.equal(r.watches[0].stopped, true);
  // a signal that had aborted before the ticker was made: nothing starts
  const late = rig(section(), { signal: controller.signal });
  assert.equal(late.watches.length === 0 || late.watches[0].stopped, true);
  assert.equal(late.running.size, 0);
  late.ticker.tick();
  assert.equal(late.ages()[0], 'drawn');
});

ok('a_gate_holds_the_writing_but_not_the_arrival', () => {
  // Ember's fold-out: closed, nothing is written; the part's arrival is noted all the same, so
  // the ages are right when it opens
  let open = false;
  const r = rig(section(), { when: () => open });
  r.show(true);
  assert.equal(r.ages()[0], 'drawn', 'closed: not written');
  r.at.ms += 40_000;
  r.fire();
  assert.equal(r.ages()[0], 'drawn');
  open = true;
  r.ticker.tick();
  assert.equal(r.ages()[0], '45 s ago', 'the 40 s it waited closed are counted');
  r.ticker.stop();
});

ok('the_words_and_the_clock_are_the_callers_to_give', () => {
  const r = rig(section(), { ago: (age) => `heard ${age} back` });
  r.show(true);
  assert.equal(r.ages()[0], 'heard 5 s back');
  assert.equal(r.signal(), '−62 dBm advert, 12 s ago', 'the signal\'s words are the daemon\'s');
  r.ticker.stop();
});

ok('the_stamp_may_be_the_root_of_the_ages_and_a_part_without_one_is_left_alone', () => {
  // Ember's part carries the clock on the section itself and marks the signal its own way
  const html = `<section id="part" data-now="${DAEMON_NOW}">
    <dd data-signal data-head="−70 dBm advert" data-heard-at="${DAEMON_NOW - 90_000}"></dd>
    <table><tbody><tr><td class="age" data-heard-at="${DAEMON_NOW - 7000}"></td></tr></tbody></table>
  </section>`;
  const r = rig(html, {}, {
    stamp: '#part', ages: '#part td.age[data-heard-at]', signal: '#part [data-signal][data-heard-at]',
  });
  r.show(true);
  assert.equal(r.document.querySelector('td.age').textContent, '7 s ago');
  assert.equal(r.document.querySelector('[data-signal]').textContent, '−70 dBm advert, 1 min ago');
  r.ticker.stop();
  const bare = rig('<table id="readings"><tbody><tr><td class="age" data-heard-at="5">x</td></tr></tbody></table>');
  bare.show(true);
  assert.deepEqual(bare.ages(), ['x'], 'no clock to age it by: left as drawn');
  bare.ticker.stop();
});

console.log(`${n} core-clock checks passed`);
