/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for the core's pinned aliases (core/core.js): a theme written against the old names
// `core.hold` and `core.drag` still reaches what `core.repeat` and `core.eq` hold now. Every member
// of an alias is the very function or constant of the current name, so a call through either does
// the same thing. The core is made over a jsdom page (apt `node-jsdom`); nothing is mounted and no
// stream is opened. Run: node core-names.mjs
import assert from 'node:assert/strict';
import { page } from './dom.mjs';
import { createCore } from '../../../bins/svolderd/web/core/core.js';
import { REPEAT_AFTER_MS, REPEAT_EVERY_MS } from '../../../bins/svolderd/web/core/repeat.js';

let n = 0;
const ok = (name, fn) => { fn(); n++; console.log('ok', name); };

const p = page('<div id="theme"></div><div id="toasts"></div>');
// the core's modules read the page's globals as a browser gives them
globalThis.window = p.window;
globalThis.document = p.document;
const core = createCore({ page: null, themes: [], window: p.window });

ok('core_hold_is_the_repeat_under_its_old_name', () => {
  assert.equal(core.hold.repeat, core.repeat);
  assert.equal(typeof core.repeat, 'function');
  assert.deepEqual(Object.keys(core.hold).sort(), ['AFTER_MS', 'EVERY_MS', 'repeat']);
  assert.equal(core.hold.AFTER_MS, REPEAT_AFTER_MS);
  assert.equal(core.hold.EVERY_MS, REPEAT_EVERY_MS);
});

ok('core_drag_holds_every_member_of_core_eq_under_its_old_name', () => {
  assert.equal(core.drag.pointDrag, core.eq.points, 'the point drag');
  assert.equal(core.drag.table, core.eq.table, 'the table');
  for (const [part, members] of Object.entries({
    bands: Object.keys(core.eq.bands),
    points: ['pointValues', 'markersIn', 'POINT_KEYS', 'touchWatch'],
    table: ['editCell', 'labelWords', 'POINT_TYPES', 'isLong'],
  })) {
    assert.ok(members.length > 0, part);
    for (const name of members) {
      assert.notEqual(core.eq[part][name], undefined, `core.eq.${part}.${name}`);
      assert.equal(core.drag[name], core.eq[part][name], `core.drag.${name} is core.eq.${part}.${name}`);
    }
  }
  assert.equal(typeof core.drag.createDrag, 'function', 'the band drag by its old path');
});

ok('core_drag_has_no_member_that_core_eq_does_not_hold', () => {
  const current = new Set([
    ...Object.keys(core.eq.bands), ...Object.keys(core.eq.points), ...Object.keys(core.eq.table),
    'pointDrag', 'table',
  ]);
  assert.deepEqual(Object.keys(core.drag).filter((name) => !current.has(name)), []);
});

p.window.close();
console.log(`core-names.mjs: ${n} cases`);
