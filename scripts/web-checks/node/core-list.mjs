/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for core/list.js, the panel of options next to a chip or a word that opens one: the
// classes passed are the ones drawn, the opener's `aria-expanded` follows it, a tap or Enter on an
// option chooses (the list closes first), a press outside closes it, one list at a time, the place
// is under the opener or, with `auto`, over it in the lower half of the box, and the keys are handed
// in (`key`, `listen`, a modal of the real stack). jsdom lays nothing out: the harness gives the
// elements their boxes. Needs jsdom (apt `node-jsdom`). Run: node core-list.mjs
import assert from 'node:assert/strict';
import { page } from './dom.mjs';
import { claim, createModals } from '../../../bins/svolderd/web/core/modal.js';
import {
  closeList, closeOf, FRAME_LIST_CLASSES, listen, openList, openLists,
} from '../../../bins/svolderd/web/core/list.js';

let n = 0;
const ok = async (name, fn) => { await fn(); n++; console.log('ok', name); };

const OWN = { list: 'x-list', option: 'x-opt' };
const OPTIONS = [
  { value: 'a', text: 'Alpha' }, { value: 'b', text: 'Beta', on: true }, { value: 'c', text: 'Gamma' },
];
const rect = (left, top, width, height) => ({
  left, top, width, height, right: left + width, bottom: top + height,
});

function rig({ chipTop = 40 } = {}) {
  const p = page('<div class="root" id="root"><button id="chip">chip</button></div><button id="out">out</button>');
  // jsdom's listeners take only its own signals
  globalThis.AbortController = p.window.AbortController;
  const root = p.document.getElementById('root');
  const chip = p.document.getElementById('chip');
  root.getBoundingClientRect = () => rect(0, 0, 400, 600);
  chip.getBoundingClientRect = () => rect(20, chipTop, 80, 20);
  return { ...p, root, chip };
}
const key = (window, name, on) => {
  const event = new window.KeyboardEvent('keydown', { key: name, bubbles: true, cancelable: true });
  on.dispatchEvent(event);
  return event;
};

await ok('a_list_draws_the_classes_and_options_it_is_given_and_marks_the_opener_expanded', async () => {
  const r = rig();
  const h = openList({
    opener: r.chip, label: 'Mode', options: OPTIONS, choose() {}, classes: OWN, root: '.root',
  });
  assert.equal(h.list.className, 'x-list');
  assert.equal(h.list.parentNode, r.root, 'inside the nearest root');
  assert.equal(h.list.getAttribute('role'), 'listbox');
  assert.equal(h.list.getAttribute('aria-label'), 'Mode');
  const options = [...h.list.children];
  assert.deepEqual(options.map((o) => o.className), ['x-opt', 'x-opt on', 'x-opt']);
  assert.deepEqual(options.map((o) => o.getAttribute('aria-selected')), ['false', 'true', 'false']);
  assert.deepEqual(options.map((o) => o.dataset.value), ['a', 'b', 'c']);
  assert.equal(r.chip.getAttribute('aria-expanded'), 'true');
  assert.equal(r.document.activeElement, options[1], 'the chosen option has the focus');
  h.close();
  assert.equal(r.chip.getAttribute('aria-expanded'), 'false');
  assert.equal(r.root.querySelector('.x-list'), null);
});

await ok('the_default_classes_are_the_frames_and_the_body_is_the_default_box', async () => {
  const r = rig();
  const h = openList({ opener: r.chip, label: 'x', options: OPTIONS, choose() {} });
  assert.equal(h.list.className, FRAME_LIST_CLASSES.list);
  assert.equal(h.list.parentNode, r.document.body);
  h.close();
});

await ok('a_tap_chooses_after_the_list_has_closed_and_the_focus_is_back_on_the_opener', async () => {
  const r = rig();
  const heard = [];
  let h = null;
  h = openList({
    opener: r.chip, label: 'x', options: OPTIONS, classes: OWN, root: '.root',
    choose: (value) => heard.push([value, h.list.isConnected]),
  });
  h.list.children[2].click();
  assert.deepEqual(heard, [['c', false]]);
  assert.equal(r.document.activeElement, r.chip);
});

await ok('a_press_outside_closes_it_without_the_focus_and_one_on_the_opener_does_not', async () => {
  const r = rig();
  const h = openList({ opener: r.chip, label: 'x', options: OPTIONS, classes: OWN, root: '.root', choose() {} });
  r.chip.dispatchEvent(new r.window.Event('pointerdown', { bubbles: true }));
  assert.equal(h.list.isConnected, true);
  r.document.getElementById('out').dispatchEvent(new r.window.Event('pointerdown', { bubbles: true }));
  assert.equal(h.list.isConnected, false);
});

await ok('a_second_list_replaces_the_first_and_closeof_toggles_the_one_of_its_opener', async () => {
  const r = rig();
  let closed = 0;
  openList({ opener: r.chip, label: 'x', options: OPTIONS, choose() {}, onClose: () => closed++ });
  const second = openList({ opener: r.chip, label: 'y', options: OPTIONS, choose() {} });
  assert.equal(closed, 1);
  assert.equal(closeOf(r.document.getElementById('out')), false);
  assert.equal(closeOf(r.chip), true);
  assert.equal(second.list.isConnected, false);
  openList({ opener: r.chip, label: 'z', options: OPTIONS, choose() {} });
  closeList();
  assert.equal(r.document.querySelector('.fi-list'), null);
});

await ok('below_puts_the_list_under_the_opener_and_auto_puts_it_over_one_in_the_lower_half', async () => {
  const high = rig({ chipTop: 40 });
  let h = openList({ opener: high.chip, label: 'x', options: OPTIONS, choose() {}, root: '.root', place: 'auto' });
  assert.equal(h.list.style.top, '64.0px', 'upper half: under (40 + 20 + 4)');
  h.close();
  const low = rig({ chipTop: 500 });
  low.chip.getBoundingClientRect = () => rect(20, 500, 80, 20);
  h = openList({ opener: low.chip, label: 'x', options: OPTIONS, choose() {}, root: '.root', place: 'auto' });
  assert.equal(h.list.style.top, '496.0px', 'lower half: over (500 - 0 - 4, the list has no height here)');
  h.close();
  h = openList({ opener: low.chip, label: 'x', options: OPTIONS, choose() {}, root: '.root', place: 'below' });
  assert.equal(h.list.style.top, '524.0px', 'below ignores the half');
  assert.equal(h.list.style.left, '20.0px');
  h.close();
});

await ok('the_part_attaches_no_key_listener_and_key_takes_the_arrows_home_end_and_escape', async () => {
  const r = rig();
  const h = openList({ opener: r.chip, label: 'x', options: OPTIONS, classes: OWN, root: '.root', choose() {} });
  const options = [...h.list.children];
  key(r.window, 'ArrowDown', options[1]);
  assert.equal(r.document.activeElement, options[1], 'no listener of its own');
  const ev = (name) => new r.window.KeyboardEvent('keydown', { key: name, cancelable: true });
  assert.equal(h.key(ev('ArrowDown')), true);
  assert.equal(r.document.activeElement, options[2]);
  assert.equal(h.key(ev('ArrowDown')), true);
  assert.equal(r.document.activeElement, options[0], 'wraps');
  assert.equal(h.key(ev('ArrowUp')), true);
  assert.equal(r.document.activeElement, options[2]);
  assert.equal(h.key(ev('Home')), true);
  assert.equal(r.document.activeElement, options[0]);
  assert.equal(h.key(ev('End')), true);
  assert.equal(r.document.activeElement, options[2]);
  assert.equal(h.key(ev('a')), false);
  assert.equal(h.key(ev('Enter')), false, 'Enter is the browser\'s: it clicks the focused option');
  const escape = ev('Escape');
  assert.equal(h.key(escape), true);
  assert.equal(escape.defaultPrevented, true);
  assert.equal(h.list.isConnected, false);
  assert.equal(r.document.activeElement, r.chip);
});

await ok('listen_hands_the_keys_of_the_panel_to_key', async () => {
  const r = rig();
  const h = openList({ opener: r.chip, label: 'x', options: OPTIONS, classes: OWN, root: '.root', choose() {} });
  listen(h);
  key(r.window, 'End', h.list.children[1]);
  assert.equal(r.document.activeElement, h.list.children[2]);
  key(r.window, 'Escape', h.list.children[2]);
  assert.equal(h.list.isConnected, false);
});

await ok('inside_a_modal_of_the_real_stack_the_modal_hands_the_keys_to_the_list_and_stays_open', async () => {
  const r = rig();
  r.document.body.insertAdjacentHTML('beforeend', '<div id="pop"><button id="pchip">p</button></div>');
  const pop = r.document.getElementById('pop');
  const pchip = r.document.getElementById('pchip');
  pop.getBoundingClientRect = () => rect(0, 0, 300, 300);
  pchip.getBoundingClientRect = () => rect(10, 10, 60, 20);
  const modals = createModals({ document: r.document, fallback: () => r.chip });
  let h = null;
  modals.open({ el: pop, focus: () => pchip, onKey: (event) => h?.key(event) === true });
  r.window.addEventListener('keydown', (event) => modals.key(event), true);
  h = openList({ opener: pchip, label: 'x', options: OPTIONS, within: pop, choose() {} });
  const options = [...h.list.children];
  key(r.window, 'ArrowDown', options[1]);
  assert.equal(r.document.activeElement, options[2], 'the modal handed the arrow in');
  key(r.window, 'Escape', options[2]);
  assert.equal(h.list.isConnected, false, 'Escape closes the list');
  assert.equal(modals.opened(), true, 'and leaves the modal open');
  assert.equal(r.document.activeElement, pchip);
  key(r.window, 'Escape', pchip);
  assert.equal(modals.opened(), false, 'the next Escape closes the modal');
});

/** A page with a chip and a modal of the real stack with a chip of its own. */
function scopes() {
  const r = rig();
  closeList();
  r.document.body.insertAdjacentHTML('beforeend',
    '<div id="pop"><button id="pchip">p</button><button id="pchip2">q</button></div>');
  const pop = r.document.getElementById('pop');
  const pchip = r.document.getElementById('pchip');
  const pchip2 = r.document.getElementById('pchip2');
  pop.getBoundingClientRect = () => rect(0, 0, 300, 300);
  pchip.getBoundingClientRect = () => rect(10, 10, 60, 20);
  pchip2.getBoundingClientRect = () => rect(80, 10, 60, 20);
  const modals = createModals({ document: r.document, fallback: () => r.chip });
  const closed = { page: 0, pop: 0 };
  // No options: the focus stays where it is, so the scope alone decides what closes (a focus that
  // leaves a list closes it by the list's own rule).
  const list = (opener, who) => openList({
    opener, label: who, options: [], choose() {}, onClose: () => { closed[who]++; },
    within: who === 'pop' ? pop : null,
  });
  const onPage = list(r.chip, 'page');
  const modal = modals.open({ el: pop, focus: () => pchip });
  return { ...r, pop, pchip, pchip2, modals, modal, closed, list, onPage };
}

await ok('a_list_in_a_modal_and_one_on_the_page_do_not_close_each_other', async () => {
  const r = scopes();
  const inPop = r.list(r.pchip, 'pop');
  assert.equal(r.closed.page, 0, "the modal's list leaves the page's open");
  assert.equal(r.onPage.list.isConnected, true);
  assert.equal(openLists(), 2);
  assert.equal(closeOf(r.pchip2), false, 'closeOf asks about its own opener');
  const second = r.list(r.chip, 'page');
  assert.equal(r.closed.page, 1, 'a list on the page replaces the one on the page');
  assert.equal(r.closed.pop, 0);
  assert.equal(inPop.list.isConnected, true);
  closeList();
  assert.equal(second.list.isConnected, false, "closeList() closes the page's");
  assert.equal(r.closed.pop, 0, "and not the modal's");
  assert.equal(closeOf(r.pchip), true, "closeOf finds the modal's by its opener");
  assert.equal(r.closed.pop, 1);
  r.list(r.pchip, 'pop');
  closeList(r.pchip2);
  assert.equal(r.closed.pop, 2, "closeList(an element of the modal) closes the modal's");
  assert.equal(openLists(), 0);
  r.modal.close();
});

await ok('two_lists_in_one_modal_replace_each_other', async () => {
  const r = scopes();
  const first = r.list(r.pchip, 'pop');
  const second = r.list(r.pchip2, 'pop');
  assert.equal(first.list.isConnected, false);
  assert.equal(r.closed.pop, 1);
  assert.equal(second.list.isConnected, true);
  assert.equal(r.closed.page, 0);
  second.close();
  r.onPage.close();
  r.modal.close();
  assert.equal(openLists(), 0);
});

await ok('closing_a_modal_closes_the_list_inside_it_and_leaves_nothing_behind', async () => {
  const r = scopes();
  const inPop = r.list(r.pchip, 'pop');
  r.modal.close();
  assert.equal(r.closed.pop, 1, "the list's onClose ran");
  assert.equal(inPop.list.isConnected, false);
  assert.equal(r.closed.page, 0, "the page's list stays");
  assert.equal(openLists(), 1, "only the page's is left");
  r.onPage.close();
  assert.equal(openLists(), 0);
});

await ok('auto_never_puts_the_list_above_the_box', async () => {
  const r = rig({ chipTop: 400 });
  r.chip.getBoundingClientRect = () => rect(20, 400, 80, 20);
  // a panel taller than the room over the chip: it would start at a negative top
  const proto = r.window.HTMLElement.prototype;
  Object.defineProperty(proto, 'offsetHeight', { configurable: true, get: () => 900 });
  const h = openList({ opener: r.chip, label: 'x', options: OPTIONS, root: '.root', place: 'auto' });
  assert.equal(h.list.style.top, '0.0px', 'clamped at the top of the box');
  h.close();
  const below = openList({ opener: r.chip, label: 'x', options: OPTIONS, root: '.root', place: 'below' });
  assert.equal(below.list.style.top, '424.0px', 'below is not clamped');
  below.close();
  delete proto.offsetHeight;
});

await ok('the_focus_leaving_for_another_control_closes_the_list_but_not_for_its_own_parts', async () => {
  const r = rig();
  const h = openList({ opener: r.chip, label: 'x', options: OPTIONS, classes: OWN, root: '.root', choose() {} });
  const options = [...h.list.children];
  const leave = (to) => options[1].dispatchEvent(new r.window.FocusEvent('focusout', { bubbles: true, relatedTarget: to }));
  leave(options[2]);
  assert.equal(h.list.isConnected, true, 'to another option: stays');
  leave(r.chip);
  assert.equal(h.list.isConnected, true, 'to the opener: stays');
  leave(null);
  assert.equal(h.list.isConnected, true, 'to nowhere (the window blurs): stays');
  leave(r.document.getElementById('out'));
  assert.equal(h.list.isConnected, false, 'to another control: closes');
});

/** A page with a list open on it and a popup that is not on the stack yet: its element is claimed
 * for the stack (as the router's `register` does) and detached (`claimed: false`: only detached), with a chip. */
function ahead({ claimed = true } = {}) {
  const r = rig();
  closeList();
  const modals = createModals({ document: r.document, fallback: () => r.chip });
  const pop = r.document.createElement('div');
  pop.innerHTML = '<button id="pchip">p</button>';
  const pchip = pop.firstChild;
  pop.getBoundingClientRect = () => rect(0, 0, 300, 300);
  pchip.getBoundingClientRect = () => rect(10, 10, 60, 20);
  if (claimed) claim(pop);
  const closed = { page: 0, pop: 0 };
  const list = (opener, who) => openList({
    opener, label: who, options: [], choose() {}, onClose: () => { closed[who]++; },
    within: who === 'pop' ? pop : null,
  });
  const onPage = list(r.chip, 'page');
  return { ...r, modals, pop, pchip, closed, list, onPage };
}

await ok('a_list_opened_before_the_stack_has_its_popup_is_the_popups', async () => {
  const r = ahead();
  const inPop = r.list(r.pchip, 'pop');
  assert.equal(r.closed.page, 0, "the page's list stays open");
  assert.equal(r.onPage.list.isConnected, true);
  assert.equal(openLists(), 2);
  const modal = r.modals.open({ el: r.pop, focus: () => r.pchip });
  closeList();
  assert.equal(r.closed.page, 1, "closeList() closes the page's");
  assert.equal(r.closed.pop, 0, "and not the popup's");
  modal.close();
  assert.equal(r.closed.pop, 1, 'the modal closing closes the list opened before it');
  assert.equal(inPop.list.isConnected, false);
  assert.equal(openLists(), 0);
});

await ok('close_list_from_a_modals_on_close_closes_the_popups_and_not_the_pages', async () => {
  // the stack's own `open` makes the element a scope: nothing claimed it ahead
  const r = ahead({ claimed: false });
  let late = null;
  const modal = r.modals.open({
    el: r.pop,
    focus: () => r.pchip,
    onClose: () => {
      late = r.list(r.pchip, 'pop');
      assert.equal(r.pop.isConnected, false, 'the stack has taken the popup out of the page');
      closeList(r.pop);
    },
  });
  modal.close();
  assert.equal(late.list.isConnected, false, "closeList(el) closed the popup's");
  assert.equal(r.closed.pop, 1);
  assert.equal(r.closed.page, 0, "the page's list stays open");
  r.onPage.close();
  assert.equal(openLists(), 0);
});

await ok('a_list_closed_by_its_modal_or_its_signal_does_not_take_the_focus', async () => {
  const r = scopes();
  let focused = 0;
  r.pchip.focus = () => { focused++; };
  r.list(r.pchip, 'pop');
  r.modal.close();
  assert.equal(focused, 0, 'the modal gives the focus back, not the list');
  r.onPage.close();
  let chip = 0;
  r.chip.focus = () => { chip++; };
  const stop = new r.window.AbortController();
  openList({ opener: r.chip, label: 'x', options: [], choose() {}, signal: stop.signal });
  stop.abort();
  assert.equal(chip, 0, 'nor when the page goes');
  openList({ opener: r.chip, label: 'x', options: [], choose() {} }).close();
  assert.equal(chip, 1, 'closed by its own word it does');
  assert.equal(openLists(), 0);
});

await ok('a_list_in_a_modal_whose_panel_would_land_outside_it_is_refused', async () => {
  const r = scopes();
  assert.throws(
    () => openList({ opener: r.pchip, label: 'x', options: OPTIONS, choose() {} }),
    /inside a modal.*within.*root/s,
    'no within and no root: the panel would go to the body, which the stack makes inert',
  );
  assert.throws(
    () => openList({ opener: r.pchip, label: 'x', options: OPTIONS, choose() {}, within: r.root }),
    /inside a modal/,
  );
  assert.equal(r.onPage.list.isConnected, true, 'a refused list closes nothing');
  assert.equal(openLists(), 1);
  assert.equal(r.pchip.getAttribute('aria-expanded'), null, 'and marks nothing');
  const fine = openList({ opener: r.pchip, label: 'x', options: [], choose() {}, root: '#pop' });
  assert.equal(fine.list.parentNode, r.pop);
  fine.close();
  r.onPage.close();
  r.modal.close();
  assert.equal(openLists(), 0);
});

/** A modal of the real stack whose pane is a place for lists (`data-list-place`, as a popup's
 * page pane is): 300 by 200 at (100, 50), 285 wide inside its scrollbar, scrolled down by 120. */
function pane() {
  const r = rig();
  closeList();
  r.document.body.insertAdjacentHTML('beforeend',
    '<div id="pop"><section id="pane" data-list-place><button id="pchip">p</button></section></div>');
  const [pop, place, pchip] = ['pop', 'pane', 'pchip'].map((id) => r.document.getElementById(id));
  place.getBoundingClientRect = () => rect(100, 50, 300, 200);
  for (const [name, value] of Object.entries({
    clientWidth: 285, clientHeight: 200, clientLeft: 0, clientTop: 0, scrollLeft: 0, scrollTop: 120,
  })) Object.defineProperty(place, name, { configurable: true, get: () => value });
  const proto = r.window.HTMLElement.prototype;
  const size = (w, h) => {
    Object.defineProperty(proto, 'offsetWidth', { configurable: true, get: () => w });
    Object.defineProperty(proto, 'offsetHeight', { configurable: true, get: () => h });
  };
  size(160, 100);
  const modals = createModals({ document: r.document, fallback: () => r.chip });
  modals.open({ el: pop, focus: () => pchip });
  /** Open the list with the chip at (`left`, `top`) of the window; gives where the panel is. */
  const at = (left, top) => {
    pchip.getBoundingClientRect = () => rect(left, top, 60, 20);
    const h = openList({ opener: pchip, label: 'x', options: OPTIONS, choose() {} });
    const where = [h.list.parentNode, h.list.style.left, h.list.style.top];
    h.close();
    return where;
  };
  return { ...r, place, at, size, modals };
}

await ok('in_a_popups_pane_the_panel_is_placed_in_what_the_pane_scrolls_and_where_it_shows', async () => {
  const r = pane();
  // no `within`, no `root`: the pane the opener sits in is the place
  assert.deepEqual(r.at(150, 80), [r.place, '50.0px', '174.0px'],
    'under the chip (80 + 20 - 50 + 4), further down by what the pane is scrolled (120)');
  assert.deepEqual(r.at(150, 200), [r.place, '50.0px', '166.0px'],
    'no room under it in what the pane shows: over it (200 - 50 - 4 - 100, + 120)');
  assert.deepEqual(r.at(350, 80), [r.place, '125.0px', '174.0px'],
    'kept left of the scrollbar (285 - 160)');
  r.size(160, 190);
  assert.deepEqual(r.at(150, 140), [r.place, '50.0px', '130.0px'],
    'room neither under nor over: against the bottom of what the pane shows (200 - 190, + 120)');
  r.size(160, 260);
  assert.deepEqual(r.at(150, 140), [r.place, '50.0px', '120.0px'],
    'taller than the pane shows: from its top');
  r.modals.closeAll();
  delete r.window.HTMLElement.prototype.offsetWidth;
  delete r.window.HTMLElement.prototype.offsetHeight;
  assert.equal(openLists(), 0);
});

await ok('the_panel_is_measured_at_the_places_corner_out_of_sight_and_shown_once_placed', async () => {
  // measured where the pane's flow would put it (after its last child, under its bottom), the
  // panel gives a pane that did not scroll a scrollbar for that one layout
  const r = pane();
  const seen = [];
  const proto = r.window.HTMLElement.prototype;
  const note = (el) => {
    if (el.getAttribute('role') === 'listbox') {
      seen.push([el.style.left, el.style.top, el.style.visibility, el.isConnected]);
    }
  };
  Object.defineProperty(proto, 'offsetWidth', { configurable: true, get() { note(this); return 160; } });
  Object.defineProperty(proto, 'offsetHeight', { configurable: true, get() { note(this); return 100; } });
  const pchip = r.document.getElementById('pchip');
  pchip.getBoundingClientRect = () => rect(150, 80, 60, 20);
  const h = openList({ opener: pchip, label: 'x', options: OPTIONS, choose() {} });
  assert.ok(seen.length >= 2, 'the panel was measured');
  for (const one of seen) assert.deepEqual(one, ['0px', '0px', 'hidden', true]);
  assert.deepEqual([h.list.style.left, h.list.style.top, h.list.style.visibility],
    ['50.0px', '174.0px', ''], 'placed, and shown again');
  h.close();
  // a list outside a place for lists is measured the same way
  seen.length = 0;
  const page = openList({ opener: r.chip, label: 'x', options: OPTIONS, choose() {}, within: r.root });
  assert.ok(seen.length >= 1);
  for (const one of seen) assert.deepEqual(one, ['0px', '0px', 'hidden', true]);
  assert.equal(page.list.style.visibility, '');
  page.close();
  r.modals.closeAll();
  delete proto.offsetWidth;
  delete proto.offsetHeight;
});

console.log(`${n} core-list checks passed`);
