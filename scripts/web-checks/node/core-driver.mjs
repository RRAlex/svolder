/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for core/driver.js, the core's test driver for the device's facts: a label of
// `deviceFactsOf` of the device the caller names is reached on the Devices popup's page of that
// device (the popup opened, or stepped there, through the router; a reading row through the All
// readings disclosure) and read off the page's cells. A theme's fact, an unknown label, a target,
// no device named, a device the page does not hold and a viewer turned away by the router are
// refused with `false` (reach) and `null` (shown), never a throw; no theme is asked. The router
// is a fake that logs and draws the real device page (draw.js, facts.js, readings.js) when it
// opens or steps.
// Run: node core-driver.mjs
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { page as domPage } from './dom.mjs';
import { deviceFactsOf, themeFactsOf } from '../../../bins/svolderd/web/core/contract.js';
import { createDriver } from '../../../bins/svolderd/web/core/driver.js';
import { DEVICES, devicePage } from '../../../bins/svolderd/web/core/popups/listed.js';
import { deviceHtml } from '../../../bins/svolderd/web/core/popups/devices/draw.js';
import { factsHtml, FACTS_ID } from '../../../bins/svolderd/web/core/popups/devices/facts.js';
import {
  readingsBodyHtml, readingsHtml, READINGS_BODY_ID,
} from '../../../bins/svolderd/web/core/popups/devices/readings.js';

let n = 0;
const ok = async (name, fn) => { await fn(); n++; console.log('ok', name); };

const fixture = JSON.parse(readFileSync(new URL('./fixtures/ember-parts.json', import.meta.url), 'utf8'));
const dac = fixture.cases.find((c) => c.name === 'dac').page.device;
const amp = fixture.cases.find((c) => c.name === 'amp').page.device;
const withCover = { ...dac, playing: { track: { cover: 'abc123' } } };

/** A driver over a page that holds `devices` (`{ url: model }`) and a router that logs. `turned`
 * makes the router refuse the popup, as it does a viewer who may not have it. The router draws
 * the device's page into the document, and opens the readings' body as the popup's wire does. */
function rig({ devices = { dac: withCover }, turned = false, hidden = false } = {}) {
  const p = domPage('<div id="host"></div>');
  // jsdom lays nothing out: every cell has the box a laid-out one would, unless `hidden` says not
  p.window.Element.prototype.getBoundingClientRect = function box() {
    const gone = hidden || this.closest('[data-nobox]');
    return gone ? { top: 0, bottom: 0, left: 0, right: 0, width: 0, height: 0 }
      : { top: 10, bottom: 30, left: 10, right: 100, width: 90, height: 20 };
  };
  const log = [];
  const at = { popup: null, page: null };
  const host = p.document.getElementById('host');
  const draw = (to) => {
    const url = to.slice(2);
    const model = devices[url];
    const device = { url, name: url, connected: true, since: null, model, series: null };
    host.innerHTML = deviceHtml(device, {
      after: `<div id="${FACTS_ID}">${factsHtml(device)}</div>${readingsHtml(device, { open: false })}`,
    });
    host.querySelector('details').addEventListener('toggle', (event) => {
      p.document.getElementById(READINGS_BODY_ID).innerHTML = readingsBodyHtml(device,
        { open: event.target.open });
    });
  };
  const router = {
    state: () => ({ ...at }),
    open(name, to) {
      log.push(['open', name, to]);
      if (turned) return;
      at.popup = name;
      at.page = to;
      draw(to);
    },
    step(to) {
      log.push(['step', to]);
      at.page = to;
      draw(to);
    },
  };
  const store = { device: (url) => devices[url] ?? null };
  return { ...p, log, host, driver: createDriver({ store, router, document: p.document }) };
}

await ok('a_device_fact_is_reached_by_opening_the_popup_at_the_devices_page_and_read_off_it', async () => {
  const r = rig();
  assert.equal(await r.driver.reach('Firmware', { device: 'dac' }), true);
  assert.deepEqual(r.log, [['open', DEVICES, devicePage('dac')]]);
  assert.equal(r.driver.shown('Firmware', { device: 'dac' }), withCover.facts.firmware);
  assert.match(r.driver.shown('Product', { device: 'dac' }), /product \d+/);
  r.window.close();
});

await ok('every_label_of_the_devices_facts_is_reached_and_shown_a_reading_row_through_the_disclosure', async () => {
  const r = rig();
  const labels = deviceFactsOf(withCover);
  assert.ok(labels.includes('12.3.3') && labels.length > 8, 'the fixture has reading rows');
  for (const label of labels) {
    assert.equal(await r.driver.reach(label, { device: 'dac' }), true, label);
    assert.notEqual(r.driver.shown(label, { device: 'dac' }), null, label);
  }
  const row = withCover.facts.rows.find((one) => one.id === '12.3.3');
  assert.equal(r.driver.shown('12.3.3', { device: 'dac' }), row.value);
  assert.equal(r.host.querySelector('details').open, true, 'a reading row opened the disclosure');
  assert.equal(r.log.filter(([kind]) => kind === 'open').length, 1, 'the popup is opened once');
  r.window.close();
});

await ok('a_fact_of_another_device_steps_the_popup_to_that_devices_page', async () => {
  const r = rig({ devices: { dac: withCover, amp } });
  assert.equal(await r.driver.reach('Firmware', { device: 'dac' }), true);
  assert.equal(await r.driver.reach('Firmware', { device: 'amp' }), true);
  assert.deepEqual(r.log, [['open', DEVICES, 'd-dac'], ['step', 'd-amp']]);
  assert.equal(r.driver.shown('Firmware', { device: 'amp' }), amp.facts.firmware);
  assert.equal(r.driver.shown('Firmware', { device: 'dac' }), null, 'the page is about the amp now');
  r.window.close();
});

await ok('a_theme_fact_is_refused_and_the_popup_is_not_opened', async () => {
  const r = rig();
  assert.deepEqual(themeFactsOf(withCover), ['Cover'], 'the device has a cover');
  assert.equal(await r.driver.reach('Cover', { device: 'dac' }), false);
  assert.equal(r.driver.shown('Cover', { device: 'dac' }), null);
  assert.deepEqual(r.log, []);
  r.window.close();
});

await ok('an_unknown_label_and_a_target_are_refused_without_a_throw', async () => {
  const r = rig();
  for (const label of ['Nope', '', 'firmware', { control: 'volume' }, ['Firmware'], 7, null,
    undefined]) {
    assert.equal(await r.driver.reach(label, { device: 'dac' }), false, JSON.stringify(label));
    assert.equal(r.driver.shown(label, { device: 'dac' }), null, JSON.stringify(label));
  }
  // a fact this device does not hold (no note on its passphrase) is not one of its facts
  assert.equal(deviceFactsOf(withCover).includes('Passphrase'), false);
  assert.equal(await r.driver.reach('Passphrase', { device: 'dac' }), false);
  assert.equal(r.driver.shown('Passphrase', { device: 'dac' }), null);
  assert.deepEqual(r.log, []);
  r.window.close();
});

await ok('the_device_is_the_callers_to_name', async () => {
  const r = rig({ devices: { dac: withCover, amp } });
  // no device named: the driver does not fall back on one
  assert.equal(await r.driver.reach('Firmware'), false);
  assert.equal(r.driver.shown('Firmware'), null);
  assert.equal(await r.driver.reach('Firmware', {}), false);
  assert.equal(r.driver.shown('Firmware', { device: null }), null);
  // a device the page does not hold
  assert.equal(await r.driver.reach('Firmware', { device: 'nobody' }), false);
  assert.equal(r.driver.shown('Firmware', { device: 'nobody' }), null);
  assert.deepEqual(r.log, []);
  // the labels are the named device's own: a reading row of the amp is no fact of the DAC
  const ampOnly = deviceFactsOf(amp).find((label) => !deviceFactsOf(withCover).includes(label));
  assert.ok(ampOnly, 'the amp has a row the DAC has not');
  assert.equal(await r.driver.reach(ampOnly, { device: 'dac' }), false);
  assert.equal(r.driver.shown(ampOnly, { device: 'dac' }), null);
  assert.equal(await r.driver.reach(ampOnly, { device: 'amp' }), true);
  r.window.close();
});

await ok('a_fact_that_has_no_box_in_the_window_is_not_reached', async () => {
  const r = rig({ hidden: true });
  assert.equal(await r.driver.reach('Firmware', { device: 'dac' }), false);
  r.window.close();
});

await ok('a_viewer_the_router_turns_away_gets_a_refusal', async () => {
  const r = rig({ turned: true });
  assert.equal(await r.driver.reach('Firmware', { device: 'dac' }), false);
  assert.equal(r.driver.shown('Firmware', { device: 'dac' }), null);
  assert.deepEqual(r.log, [['open', DEVICES, 'd-dac']]);
  r.window.close();
});

console.log(`${n} core-driver checks passed`);
