/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for the facts of a device's page in the Devices popup (core/popups/devices/
// facts.js), drawn once by the core whichever theme is mounted: every fact of `deviceFactsOf`
// (contract.js) of every device of the fixture is drawn, with the words the daemon gave; Unit only
// when the viewer may see it; the passphrase note only when there is one; *Identify* only when the
// profile offers it, `aria-disabled` with its reason on a device that is not connected or cannot
// now be asked; what a label and a value say is escaped; the readings table holds the rest of the
// ids. These cases held the four themes' Device fold-outs; the fold-outs draw none of this now.
// Run: node core-device-facts.mjs
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { deviceFactsOf } from '../../../bins/svolderd/web/core/contract.js';
import {
  FACTS_AGES, FACTS_ID, IDENTIFY_ID, factsHtml,
} from '../../../bins/svolderd/web/core/popups/devices/facts.js';
import {
  READINGS_AGES, readingsBodyHtml,
} from '../../../bins/svolderd/web/core/popups/devices/readings.js';

let n = 0;
const ok = (name, fn) => { fn(); n++; console.log('ok', name); };
const fixture = JSON.parse(readFileSync(new URL('./fixtures/ember-parts.json', import.meta.url), 'utf8'));
const clone = (value) => JSON.parse(JSON.stringify(value));
const cases = fixture.cases.filter((c) => c.page.device?.facts);
const deviceOf = (c, connected = true) => ({
  name: c.page.device.name, connected, model: clone(c.page.device),
});
const unescape = (text) => text.replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&quot;/g, '"')
  .replace(/&#39;/g, "'").replace(/&amp;/g, '&');
/** The cells of the facts' list by label, from the markup. */
const cellsOf = (html) => new Map([...html.matchAll(/<dd class="[^"]*" data-fact="([^"]*)"([^>]*)>([^<]*)<\/dd>/g)]
  .map((m) => [unescape(m[1]), { attrs: m[2], text: unescape(m[3]) }]));
const withFacts = (c, over) => ({ ...deviceOf(c), model: { ...deviceOf(c).model, facts: { ...c.page.device.facts, ...over } } });

ok('every_fact_of_every_device_is_drawn_with_the_daemons_words', () => {
  let checked = 0;
  for (const c of cases) {
    const device = deviceOf(c);
    const f = device.model.facts;
    const html = factsHtml(device);
    const cells = cellsOf(html);
    const readings = readingsBodyHtml(device, { open: true });
    const labels = deviceFactsOf(device.model);
    for (const label of labels) {
      checked++;
      assert.ok(cells.has(label) || readings.includes(`data-reading="${label}"`), `${c.name}: ${label}`);
    }
    assert.equal(cells.has('Unit'), Boolean(f.unit), `${c.name}: Unit only when present`);
    assert.equal(cells.has('Passphrase'), f.passphrase_note !== null && f.passphrase_note !== undefined, c.name);
    assert.equal(cells.get('Product').text, f.product ? `${f.product} (product ${f.product_id})` : `product ${f.product_id}`);
    assert.equal(cells.get('DSP').text, f.dsp == null ? '—' : String(f.dsp), c.name);
    for (const [label, key] of [['Firmware', 'firmware'], ['Profile', 'profile'], ['Link', 'link']]) {
      assert.equal(cells.get(label).text, f[key], `${c.name}: ${label}`);
    }
    assert.ok(cells.get('Signal').text.startsWith(f.signal_age?.head ?? f.signal), c.name);
    assert.ok(html.includes(`data-now="${f.now_ms}"`), 'the section carries the daemon\'s clock');
  }
  assert.ok(checked > 150, `${checked} facts`);
});

ok('a_guest_sees_no_unit_and_a_product_the_profile_does_not_name_reads_as_its_number', () => {
  const guest = cases.find((c) => c.name === 'dac-guest');
  assert.ok(!guest.page.device.facts.unit, 'the guest\'s model carries no unit');
  assert.ok(!cellsOf(factsHtml(deviceOf(guest))).has('Unit'));
  const heard = cases.find((c) => c.name === 'dac-facts-heard');
  assert.equal(cellsOf(factsHtml(deviceOf(heard))).get('Product').text, 'product 9');
});

ok('the_signal_carries_what_the_ticker_keeps_current_and_the_readings_carry_their_stamps', () => {
  const c = cases.find((x) => x.name === 'dac-facts-heard');
  const html = factsHtml(deviceOf(c));
  const signal = cellsOf(html).get('Signal').attrs;
  assert.match(signal, /data-signal/);
  assert.match(signal, /data-head="[^"]+" data-heard-at="\d+"/);
  assert.equal(FACTS_AGES.stamp, `#${FACTS_ID} section[data-now]`);
  assert.ok(FACTS_AGES.signal.includes('data-signal') && READINGS_AGES.ages.includes('data-heard-at'));
  const body = readingsBodyHtml(deviceOf(c), { open: true });
  assert.ok(/class="fpd-reading-age" data-heard-at="1700000010000"/.test(body), 'a heard age');
});

ok('identify_is_offered_only_where_the_profile_offers_it_and_the_gap_is_the_daemons', () => {
  const c = cases.find((x) => x.name === 'dac');
  const f = c.page.device.facts;
  assert.ok(f.identify_offered, 'the fixture device offers it');
  const html = factsHtml(deviceOf(c));
  assert.ok(html.includes(`id="${IDENTIFY_ID}"`) && html.includes(`data-gap="${f.identify_gap_secs}"`));
  assert.equal(html.includes('aria-disabled'), !f.identify, 'enabled while the device can be asked');
  assert.ok(html.includes('Asks the unit to show itself'));
  const none = factsHtml(withFacts(c, { identify_offered: false }));
  assert.ok(!none.includes('Identify') && !none.includes(IDENTIFY_ID), none);
});

ok('identify_stays_disabled_with_its_reason_on_a_device_that_is_away_or_cannot_be_asked', () => {
  const c = cases.find((x) => x.name === 'dac');
  const asked = factsHtml(withFacts(c, { identify_offered: true, identify: true }));
  assert.ok(!asked.includes('aria-disabled'), 'connected and askable: enabled');
  const away = factsHtml({ ...withFacts(c, { identify_offered: true, identify: true }), connected: false });
  assert.match(away, /aria-disabled="true"/);
  assert.match(away, /not connected/);
  assert.match(away, /aria-describedby="popup-devices-identify-note"/);
  const busy = factsHtml(withFacts(c, { identify_offered: true, identify: false }));
  assert.match(busy, /aria-disabled="true"/);
  assert.match(busy, /cannot be asked/);
});

ok('nothing_is_drawn_without_facts_and_what_a_label_or_a_value_says_is_escaped', () => {
  const c = cases.find((x) => x.name === 'dac');
  assert.equal(factsHtml({ name: 'A', connected: true, model: null }), '');
  assert.equal(factsHtml({ name: 'A', connected: true, model: { name: 'A' } }), '');
  const rows = [{ id: '2', label: '<script>alert("x")</script>', value: '<img src=x onerror="y">', heard_at_ms: null }];
  const device = withFacts(c, { rows, firmware: '<b>1</b>' });
  const html = factsHtml(device) + readingsBodyHtml(device, { open: true });
  assert.ok(!html.includes('<script>') && !html.includes('<img') && !html.includes('<b>1'), html);
  assert.ok(html.includes('&lt;script&gt;') && html.includes('&lt;img') && html.includes('&lt;b&gt;1'), html);
  assert.ok(html.includes('onerror=&quot;y&quot;') && !html.includes('onerror="y"'), html);
});

ok('no_passphrase_digits_reach_the_facts_of_any_device', () => {
  for (const c of cases) assert.ok(!factsHtml(deviceOf(c)).includes('>250<'), c.name);
});

console.log(`core-device-facts: ${n} cases passed`);
