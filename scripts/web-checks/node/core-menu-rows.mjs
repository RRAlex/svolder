/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for the menu's rows (core/menu-rows.js): the pure part of the menu. `menuRows`
// gives every row of the menu in order (the Theme row, the rows the theme gives, a rule, the
// frame's own), `frameRows` the frame's own for a viewer: the popups' rows, then Sign out or Sign
// in (no row names a device: the Devices popup shows another one). The cases pin the frame's rows
// for a writer, a reader and a guest, so a popup's row that is added later lands in a known place.
// Run: node core-menu-rows.mjs
import assert from 'node:assert/strict';
import * as menuRowsModule from '../../../bins/svolderd/web/core/menu-rows.js';
import { POPUPS, checkListed, rowId, seenBy } from '../../../bins/svolderd/web/core/popups/listed.js';

const { frameRows, menuRows } = menuRowsModule;

let n = 0;
const ok = (name, fn) => { fn(); n++; console.log('ok', name); };

const WRITER = { guest: false, writes: true, sign_out: true };
const READER = { guest: false, writes: false, sign_out: true };
const GUEST = { guest: true, writes: false };
const TWO = [{ url: 'A', name: 'Bifrost' }, { url: 'B', name: 'Kara <amp>' }];

/** The hands a row's `run` reaches, each counting its calls. */
function hands() {
  const calls = [];
  return {
    calls,
    run: {
      picker: () => calls.push('picker'),
      signOut: () => calls.push('signOut'),
      popup: (name) => calls.push(`popup ${name}`),
    },
  };
}

/** What a row says, without its functions. */
const said = (def) => {
  const out = {};
  for (const [name, value] of Object.entries(def)) {
    if (typeof value !== 'function') out[name] = value;
  }
  return out;
};

const SIGN_OUT = { kind: 'action', label: 'Sign out', elementId: 'menu-sign-out' };
const SIGN_IN = {
  kind: 'action', tag: 'a', label: 'Sign in', href: './?sign-in', elementId: 'menu-sign-in',
};

ok('the_frames_rows_for_a_writer_in_order', () => {
  const h = hands();
  const rows = frameRows({ viewer: WRITER, run: h.run });
  assert.deepEqual(rows.map(said), [SIGN_OUT]);
  rows[0].run();
  assert.deepEqual(h.calls, ['signOut']);
});

ok('the_frames_rows_for_a_reader_are_the_writers', () => {
  const h = hands();
  const rows = frameRows({ viewer: READER, run: h.run });
  assert.deepEqual(rows.map(said), [SIGN_OUT]);
});

ok('the_frames_rows_for_a_guest_end_in_sign_in', () => {
  const h = hands();
  const rows = frameRows({ viewer: GUEST, run: h.run });
  assert.deepEqual(rows.map(said), [SIGN_IN]);
  // the link is the browser's: its run does nothing
  rows[0].run();
  assert.deepEqual(h.calls, []);
});

ok('a_viewer_who_can_neither_sign_in_nor_out_has_no_such_row', () => {
  // no sign-in is set up: the page has no viewer to sign out
  const h = hands();
  assert.deepEqual(
    frameRows({ viewer: { guest: false, writes: true }, devices: [], shown: null, run: h.run }), [],
  );
  assert.deepEqual(frameRows({ viewer: undefined, devices: [], shown: null, run: h.run }), []);
});

ok('the_menu_is_the_theme_row_the_themes_rows_a_rule_and_the_frames', () => {
  const h = hands();
  const given = [{ kind: 'heading', text: 'Of the theme' }, { kind: 'sep' }];
  const rows = menuRows({
    theme: { label: 'Ember' }, given, viewer: WRITER, run: h.run,
  });
  assert.deepEqual(said(rows[0]), {
    kind: 'action', label: 'Theme: Ember', elementId: 'menu-theme', go: true, picture: true,
  });
  rows[0].run();
  assert.deepEqual(h.calls, ['picker']);
  assert.equal(rows[1], given[0], "the theme's rows are the very objects it gave");
  assert.equal(rows[2], given[1]);
  assert.deepEqual(rows[3], { kind: 'sep', own: true });
  assert.deepEqual(rows.slice(4).map((def) => def.elementId), ['menu-sign-out']);
  assert.equal(rows.length, 5);
});

ok('with_no_row_of_the_frames_own_there_is_no_rule', () => {
  const h = hands();
  const rows = menuRows({
    theme: undefined, given: [], viewer: { guest: false }, run: h.run,
  });
  assert.deepEqual(rows.map(said), [
    { kind: 'action', label: 'Theme: ', elementId: 'menu-theme', go: true, picture: true },
  ]);
});

/** Two popups as the list holds them: one for everyone, one for a writer. */
const LISTED = [
  { name: 'help', title: 'Help', load: async () => ({}), row: { label: 'Help', sees: 'everyone' } },
  {
    name: 'devices', title: 'Devices', load: async () => ({}),
    row: { label: 'Devices <b>', sees: 'writer' },
  },
];

ok('a_listed_popups_row_stands_before_the_device_for_who_sees_it', () => {
  const h = hands();
  const of = (viewer) => frameRows({ viewer, devices: TWO, shown: 'A', run: h.run, popups: LISTED })
    .map((def) => def.elementId);
  assert.deepEqual(of(WRITER), ['menu-popup-help', 'menu-popup-devices', 'menu-sign-out']);
  assert.deepEqual(of(READER), ['menu-popup-help', 'menu-sign-out'],
    'signed in, with no right to write');
  assert.deepEqual(of(GUEST), ['menu-popup-help', 'menu-sign-in']);
  const rows = frameRows({ viewer: WRITER, devices: [], shown: null, run: h.run, popups: LISTED });
  assert.deepEqual(said(rows[1]), {
    kind: 'action', label: 'Devices <b>', elementId: 'menu-popup-devices', go: true,
  });
  rows[1].run();
  rows[0].run();
  assert.deepEqual(h.calls, ['popup devices', 'popup help'], 'a row opens its own popup');
  assert.equal(rowId('help'), 'menu-popup-help');
  assert.equal(seenBy(LISTED[1], { guest: true, writes: true }), false, 'a guest is no writer');
  // the core's own rule (`mayWrite`): a page with no sign-in set up writes
  assert.equal(seenBy(LISTED[1], { guest: false }), true);
  assert.equal(seenBy(LISTED[1], undefined), true);
  const all = menuRows({
    theme: { label: 'Ember' }, given: [], viewer: { guest: false, writes: false }, devices: [],
    shown: null, run: h.run, popups: LISTED,
  });
  assert.deepEqual(all.map((def) => def.elementId ?? def.kind), ['menu-theme', 'sep', 'menu-popup-help'],
    "a popup's row alone brings the frame's rule");
});

ok('the_list_refuses_a_popup_without_its_row_and_a_name_twice', () => {
  const one = LISTED[0];
  assert.doesNotThrow(() => checkListed(LISTED));
  assert.throws(() => checkListed([{ ...one, row: undefined }]), /help.*row/);
  assert.throws(() => checkListed([{ ...one, row: { label: '', sees: 'everyone' } }]), /help.*label/);
  assert.throws(() => checkListed([{ ...one, row: { label: 'Help', sees: 'admin' } }]), /help.*sees/);
  assert.throws(() => checkListed([{ ...one, load: undefined }]), /help.*load/);
  assert.throws(() => checkListed([{ ...one, title: '' }]), /help.*title/);
  assert.throws(() => checkListed([one, one]), /help.*twice/);
  assert.throws(() => checkListed([{ ...one, name: '' }]), /name/);
});

ok('the_pages_list_is_one_the_core_can_read_and_holds_help', () => {
  assert.doesNotThrow(() => checkListed(POPUPS));
  assert.deepEqual(POPUPS.map((popup) => [popup.name, popup.row.label, popup.row.sees]),
    [['devices', 'Devices…', 'everyone'], ['sound', 'Sound…', 'writer'],
      ['scenes', 'Scenes…', 'writer'], ['setup', 'PipeWire config…', 'everyone'],
      ['help', 'Help…', 'everyone']]);
  assert.equal(POPUPS[0].stays, true, 'Devices stays through a change of the shown device');
  assert.equal(POPUPS[2].stays, true, 'Scenes stays too: it is about every device');
  assert.ok(!POPUPS[1].stays && !POPUPS[3].stays && !POPUPS[4].stays,
    'Sound, PipeWire config and Help do not');
});

ok('no_row_of_the_frames_names_a_device', () => {
  // the Devices popup shows another device (Show on the page): the menu has no switcher
  const h = hands();
  for (const viewer of [WRITER, READER, GUEST]) {
    const ids = frameRows({ viewer, devices: TWO, shown: 'A', run: h.run, popups: LISTED })
      .map((def) => def.elementId);
    assert.ok(!ids.includes('menu-device'), `${ids}`);
  }
  assert.equal(typeof menuRowsModule.deviceRows, 'undefined', 'the device list rows are gone');
  assert.equal(h.run.devices, undefined);
});

console.log(`core-menu-rows.mjs: ${n} passed`);
