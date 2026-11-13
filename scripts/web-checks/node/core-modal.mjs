/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for the core's modal layer (core/modal.js) and its two clients, the picker of
// themes (core/picker.js) and the question (core/ask.js): the top of the stack owns every key
// (a theme's listeners on the document and the window hear none, `T` and `?` included), Escape
// closes the top and Tab stays in it, the page under the top is inert but the popups, the focus
// goes in and comes back to the place chosen, and `closeAll` empties the stack. A part inside a
// modal (a field, a list) attaches no key listener: the browser's own typing is left to the field
// and the list gets its keys from the modal's `onKey`. The keys go through a window-capture
// listener as the frame's do (core/frame.js). A jsdom page stands in for the browser's (dom.mjs):
// it has no Tab of its own, so a Tab the stack leaves to the browser moves nothing here.
// Run: node core-modal.mjs
import assert from 'node:assert/strict';
import { createAsk } from '../../../bins/svolderd/web/core/ask.js';
import { reservedKey } from '../../../bins/svolderd/web/core/frame.js';
import { createModals, returnTarget } from '../../../bins/svolderd/web/core/modal.js';
import { createPicker } from '../../../bins/svolderd/web/core/picker.js';
import { page } from './dom.mjs';

let n = 0;
const ok = async (name, fn) => { await fn(); n++; console.log('ok', name); };

const BODY = '<div id="frame"><button id="fx-button">Menu</button></div>'
  + '<div id="theme"><button id="knob">Volume</button><button id="chip">Warm</button></div>'
  + '<div id="toasts"></div>';

/** A page with the stack wired as the frame wires it, and a theme's listeners that note what they
 * hear: on the document in both phases and on the window. */
function rig() {
  const p = page(BODY);
  const { window, document } = p;
  const modal = createModals({ document, fallback: () => document.getElementById('fx-button') });
  const heard = [];
  let frameKeys = 0;
  window.addEventListener('keydown', (event) => {
    if (modal.key(event)) return;
    // what the frame does with a key no modal took: `T`, Space
    frameKeys += 1;
  }, { capture: true });
  // a theme's listeners: on the window in the capture phase too, added after the frame's
  window.addEventListener('keydown', (event) => heard.push(`late ${event.key}`), { capture: true });
  document.addEventListener('keydown', (event) => heard.push(`capture ${event.key}`), { capture: true });
  document.addEventListener('keydown', (event) => heard.push(`document ${event.key}`));
  window.addEventListener('keydown', (event) => heard.push(`window ${event.key}`));
  /** A dialog with the buttons `ids`. */
  const dialog = (...ids) => {
    const el = document.createElement('div');
    for (const id of ids) {
      const button = document.createElement('button');
      button.id = id;
      el.append(button);
    }
    return el;
  };
  const at = () => document.activeElement?.id || document.activeElement?.tagName;
  return { ...p, modal, heard, dialog, at, frameKeys: () => frameKeys };
}

await ok('the_top_modal_owns_every_key_and_t_and_question_mark_do_nothing', () => {
  const r = rig();
  r.document.getElementById('knob').focus();
  r.key('ArrowLeft');
  assert.deepEqual(r.heard, ['late ArrowLeft', 'capture ArrowLeft', 'document ArrowLeft', 'window ArrowLeft'],
    'with no modal the theme hears the key');
  assert.equal(r.frameKeys(), 1);
  r.heard.length = 0;
  const own = [];
  r.modal.open({ el: r.dialog('one', 'two'), onKey: (event) => { own.push(event.key); return false; } });
  const keys = ['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown', 'm', '0', 'f', 'p', 'v', 'a',
    'A', '?', 't', 'T', ' ', 'Enter', 'q', 'Home'];
  for (const name of keys) {
    const event = r.key(name);
    assert.equal(event.defaultPrevented, false, `${name}: the browser's own action is left`);
  }
  // on the page behind too (a key sent to an element the modal covers)
  r.key('ArrowLeft', { on: r.document.getElementById('knob') });
  r.key('m', { on: r.document.body });
  r.key('t', { ctrlKey: true });
  assert.deepEqual(r.heard, [], 'no listener of a theme hears a key while a modal is open');
  assert.equal(r.frameKeys(), 1, 'the frame does nothing with T, ? or Space');
  assert.deepEqual(own, [...keys, 'ArrowLeft', 'm', 't'], 'the top modal is handed every key');
  assert.equal(r.modal.opened(), true, 'none of them closes it');
});

await ok('the_stack_keeps_its_order_and_the_key_goes_to_the_top_only', () => {
  const r = rig();
  const got = [];
  const first = r.modal.open({ el: r.dialog('a'), onKey: (e) => { got.push(`first ${e.key}`); return true; } });
  r.key('x');
  const second = r.modal.open({ el: r.dialog('b'), onKey: (e) => { got.push(`second ${e.key}`); return true; } });
  r.key('y');
  assert.equal(first.opened() && second.opened(), true);
  second.close();
  r.key('z');
  assert.deepEqual(got, ['first x', 'second y', 'first z']);
  assert.equal(first.opened(), true);
  assert.equal(second.opened(), false);
  first.close();
  assert.equal(r.modal.opened(), false);
  r.key('w');
  assert.deepEqual(r.heard, ['late w', 'capture w', 'document w', 'window w'], 'the keys are the page\'s again');
});

await ok('escape_closes_the_top_only_and_a_key_the_modal_takes_is_its_own', () => {
  const r = rig();
  const closed = [];
  r.modal.open({ el: r.dialog('a'), onClose: () => closed.push('first') });
  r.modal.open({
    el: r.dialog('b'),
    onClose: () => closed.push('second'),
    // a list open inside the modal: its Escape is not the modal's
    onKey: (event) => event.key === 'Escape' && event.shiftKey,
  });
  r.key('Escape', { shiftKey: true });
  assert.deepEqual(closed, [], 'an Escape the modal took closes nothing');
  const event = r.key('Escape');
  assert.equal(event.defaultPrevented, true);
  assert.deepEqual(closed, ['second']);
  r.key('Escape', { ctrlKey: true });
  assert.deepEqual(closed, ['second'], 'with a modifier it is not the modal\'s Escape');
  r.key('Escape');
  assert.deepEqual(closed, ['second', 'first']);
  assert.equal(r.modal.opened(), false);
  assert.deepEqual(r.heard, [], 'no Escape reached the theme');
});

await ok('tab_stays_inside_the_top_and_is_the_browsers_own_between_its_ends', () => {
  const r = rig();
  const el = r.dialog('one', 'two', 'three', 'four');
  el.querySelector('#two').disabled = true;
  const hidden = r.document.createElement('button');
  hidden.id = 'hidden';
  hidden.hidden = true;
  el.append(hidden);
  r.modal.open({ el });
  assert.equal(r.at(), 'one', 'the focus opens on the first control');
  // between the ends the browser's own Tab order is left alone
  assert.equal(r.key('Tab').defaultPrevented, false, 'forward from the first');
  r.document.getElementById('three').focus();
  assert.equal(r.key('Tab').defaultPrevented, false, 'forward from the middle');
  assert.equal(r.key('Tab', { shiftKey: true }).defaultPrevented, false, 'back from the middle');
  assert.equal(r.at(), 'three');
  // at the ends the stack turns it round, past what is disabled or hidden
  r.document.getElementById('four').focus();
  assert.equal(r.key('Tab').defaultPrevented, true);
  assert.equal(r.at(), 'one', 'round the end, past the hidden one');
  assert.equal(r.key('Tab', { shiftKey: true }).defaultPrevented, true);
  assert.equal(r.at(), 'four');
  assert.equal(r.key('Tab', { shiftKey: true }).defaultPrevented, false, 'back from the last');
  // the focus got out (a click on a popup's button): Tab brings it back in
  r.document.getElementById('toasts').append(r.dialog('toast-action'));
  r.document.getElementById('toast-action').focus();
  assert.equal(r.key('Tab').defaultPrevented, true);
  assert.equal(r.at(), 'one');
  r.document.getElementById('toast-action').focus();
  r.key('Tab', { shiftKey: true });
  assert.equal(r.at(), 'four');
  // on the modal itself (no control has it yet): into it, never out
  el.focus();
  r.key('Tab', { shiftKey: true });
  assert.equal(r.at(), 'four');
  // a modal with nothing to focus keeps the focus on itself
  const bare = r.document.createElement('div');
  r.modal.open({ el: bare });
  assert.equal(r.document.activeElement, bare);
  assert.equal(r.key('Tab').defaultPrevented, true);
  assert.equal(r.document.activeElement, bare);
});

await ok('tab_goes_round_by_the_controls_that_are_rendered', () => {
  const r = rig();
  const el = r.dialog('first', 'two', 'three', 'last');
  // what a sheet does with `display: none`: jsdom lays nothing out, so the two things a browser
  // does (it says the control is not rendered, and the control takes no focus) are put on it
  const unrendered = (id) => {
    const one = el.querySelector(`#${id}`);
    one.checkVisibility = () => false;
    one.focus = () => {};
  };
  unrendered('first');
  unrendered('last');
  r.modal.open({ el });
  assert.equal(r.at(), 'two', 'the focus opens on the first control that is rendered');
  assert.equal(r.key('Tab').defaultPrevented, false, 'between the rendered ends it is the browser\'s');
  r.document.getElementById('three').focus();
  assert.equal(r.key('Tab').defaultPrevented, true, 'the last rendered control is the end');
  assert.equal(r.at(), 'two', 'Tab goes round, though a control that is not rendered follows');
  assert.equal(r.key('Tab', { shiftKey: true }).defaultPrevented, true, 'and the first rendered one');
  assert.equal(r.at(), 'three', 'Shift+Tab goes round, though one that is not rendered comes before');
  // every control unrendered: the focus stays on the modal
  unrendered('two');
  unrendered('three');
  el.tabIndex = -1;
  el.focus();
  assert.equal(r.key('Tab').defaultPrevented, true);
  assert.equal(r.document.activeElement, el);
});

await ok('tab_reaches_the_summary_of_a_closed_details_at_the_end', () => {
  const r = rig();
  const el = r.document.createElement('div');
  el.innerHTML = '<button id="first">One</button><button id="before">Two</button>'
    + '<details id="more"><summary id="sum">More</summary><button id="inside">Hidden</button></details>';
  // what a browser does with a closed `<details>`: its content is not rendered, its summary is
  const inside = el.querySelector('#inside');
  inside.checkVisibility = () => false;
  inside.focus = () => {};
  r.modal.open({ el });
  r.document.getElementById('before').focus();
  assert.equal(r.key('Tab').defaultPrevented, false, 'from the control before it, Tab is the browser\'s');
  r.document.getElementById('sum').focus();
  assert.equal(r.key('Tab').defaultPrevented, true, 'the summary is the last end');
  assert.equal(r.at(), 'first', 'Tab from the summary goes round to the first control');
  assert.equal(r.key('Tab', { shiftKey: true }).defaultPrevented, true);
  assert.equal(r.at(), 'sum', 'and Shift+Tab goes back round to the summary');
});

await ok('a_field_types_and_a_list_gets_its_keys_through_the_modals_onkey', () => {
  const r = rig();
  const { document } = r;
  // a part that lives in a modal attaches no key listener: it exports `key(event)`
  const el = document.createElement('div');
  el.innerHTML = '<input id="name"><button id="pick">Kind</button>'
    + '<ul id="kinds" hidden><li id="kind-0" tabindex="-1">DAC</li><li id="kind-1" tabindex="-1">Amp</li></ul>'
    + '<button id="save">Save</button>';
  const [field, pick, list] = ['name', 'pick', 'kinds'].map((id) => el.querySelector(`#${id}`));
  const deaf = [];
  field.addEventListener('keydown', (event) => deaf.push(event.key));
  const kinds = {
    chosen: null,
    saved: 0,
    open() { list.hidden = false; list.children[0].focus(); },
    /** A key for the list while it is open: the arrows move, Enter chooses, Escape shuts the
     * list only. `true` for a key it took. */
    key(event) {
      if (list.hidden) return false;
      const at = [...list.children].indexOf(document.activeElement);
      const step = { ArrowDown: 1, ArrowUp: -1 }[event.key];
      if (step) list.children[(at + step + list.children.length) % list.children.length].focus();
      else if (event.key === 'Enter') kinds.chosen = document.activeElement.textContent;
      else if (event.key !== 'Escape') return false;
      if (event.key !== 'ArrowDown' && event.key !== 'ArrowUp') {
        list.hidden = true;
        pick.focus();
      }
      event.preventDefault();
      return true;
    },
  };
  /** The form's own keys: Enter in the field saves. */
  const form = (event) => {
    if (event.key !== 'Enter' || event.target !== field) return false;
    kinds.saved += 1;
    return true;
  };
  r.modal.open({ el, onKey: (event) => kinds.key(event) || form(event) });
  assert.equal(r.at(), 'name');
  // the field: the browser types, moves the caret and deletes; the stack prevents none of it
  for (const name of ['a', 'T', '?', ' ', 'ArrowLeft', 'ArrowRight', 'Home', 'Backspace', 'Delete']) {
    assert.equal(r.key(name).defaultPrevented, false, `${name} in the field is the browser's`);
  }
  assert.equal(r.key('a', { ctrlKey: true }).defaultPrevented, false);
  assert.deepEqual(deaf, [], 'a listener on the field itself hears nothing: the keys go through onKey');
  assert.deepEqual(r.heard, [], 'and no key typed in the field reaches the page');
  r.key('Enter');
  assert.equal(kinds.saved, 1, 'Enter in the field is the form\'s, through onKey');
  // the list
  kinds.open();
  assert.equal(r.at(), 'kind-0');
  assert.equal(r.key('ArrowDown').defaultPrevented, true);
  assert.equal(r.at(), 'kind-1');
  r.key('ArrowDown');
  assert.equal(r.at(), 'kind-0', 'round the end');
  r.key('ArrowUp');
  assert.equal(r.at(), 'kind-1');
  r.key('Escape');
  assert.equal(list.hidden, true, 'Escape shuts the list');
  assert.equal(r.modal.opened(), true, 'and not the modal');
  assert.equal(r.at(), 'pick');
  kinds.open();
  r.key('ArrowDown');
  r.key('Enter');
  assert.equal(kinds.chosen, 'Amp');
  assert.equal(kinds.saved, 1, 'the list\'s Enter is not the form\'s');
  // with the list shut the arrows are no one's, Tab is the stack's and Escape closes the modal
  assert.equal(r.key('ArrowDown').defaultPrevented, false);
  r.document.getElementById('save').focus();
  r.key('Tab');
  assert.equal(r.at(), 'name', 'Tab from the last control, past the shut list');
  r.key('Escape');
  assert.equal(r.modal.opened(), false);
  assert.deepEqual(r.heard, []);
});

await ok('a_key_of_a_composition_is_left_to_the_field', () => {
  const r = rig();
  const el = r.document.createElement('div');
  el.innerHTML = '<input id="name"><button id="save">Save</button>';
  const own = [];
  r.modal.open({ el, onKey: (event) => { own.push(event.key); return false; } });
  r.document.getElementById('save').focus();
  for (const name of ['Escape', 'Tab', 'Enter']) {
    const event = r.key(name, { isComposing: true });
    assert.equal(event.defaultPrevented, false, `${name} while composing`);
  }
  assert.equal(r.modal.opened(), true, 'Escape that ends a composition does not close the modal');
  assert.equal(r.at(), 'save', 'and Tab moves nothing');
  assert.deepEqual(own, [], 'the modal is not handed it either');
  assert.deepEqual(r.heard, [], 'nor is the page');
});

await ok('the_page_under_the_top_is_inert_but_the_popups', () => {
  const r = rig();
  const { document } = r;
  const [frame, theme, toasts] = ['frame', 'theme', 'toasts'].map((id) => document.getElementById(id));
  // out of reach already, for a reason of its own
  const lost = document.createElement('div');
  lost.inert = true;
  document.body.append(lost);
  const a = r.dialog('a');
  const first = r.modal.open({ el: a, labelledBy: 'a-title' });
  assert.equal(a.parentNode, document.body, 'a modal is a child of the body');
  assert.equal(a.getAttribute('role'), 'dialog');
  assert.equal(a.getAttribute('aria-modal'), 'true');
  assert.equal(a.getAttribute('aria-labelledby'), 'a-title');
  assert.deepEqual([frame.inert, theme.inert, Boolean(toasts.inert), Boolean(a.inert)], [true, true, false, false]);
  const b = r.dialog('b');
  const second = r.modal.open({ el: b });
  assert.deepEqual([frame.inert, theme.inert, Boolean(toasts.inert), a.inert, Boolean(b.inert)],
    [true, true, false, true, false], 'the modal under the top is out of reach too');
  assert.equal(b.getAttribute('aria-modal'), 'true');
  second.close();
  assert.equal(b.isConnected, false, 'a closed modal leaves the page');
  assert.deepEqual([frame.inert, theme.inert, a.inert], [true, true, false]);
  first.close();
  assert.deepEqual([frame.inert, theme.inert, Boolean(toasts.inert)], [false, false, false]);
  assert.equal(lost.inert, true, 'what was out of reach before stays so');
});

await ok('the_focus_comes_back_to_returnTo_else_the_opener', () => {
  const r = rig();
  const { document } = r;
  const [knob, chip, menu] = ['knob', 'chip', 'fx-button'].map((id) => document.getElementById(id));
  // the opener
  knob.focus();
  r.modal.open({ el: r.dialog('a'), focus: null });
  assert.equal(r.at(), 'a');
  r.modal.close();
  assert.equal(r.at(), 'knob');
  // a function `focus` is asked once, when the modal is in the page: it can tell what is rendered
  const late = r.dialog('p', 'q');
  const asked = [];
  r.modal.open({
    el: late,
    focus: () => { asked.push(late.isConnected); return late.querySelector('#q'); },
  });
  assert.deepEqual(asked, [true], 'asked once, with the modal in the page');
  assert.equal(r.at(), 'q');
  r.modal.open({ el: late, focus: () => { asked.push('again'); return null; } });
  assert.deepEqual(asked, [true], 'a modal that stands is not asked again');
  r.modal.close();
  // `focus` names where the focus opens
  const el = r.dialog('yes', 'no');
  r.modal.open({ el, focus: el.querySelector('#no'), returnTo: 'chip' });
  assert.equal(r.at(), 'no');
  // the control is drawn again while the modal stands: the id finds the new one
  chip.remove();
  const twin = document.createElement('button');
  twin.id = 'chip';
  document.getElementById('theme').append(twin);
  r.modal.close();
  assert.equal(document.activeElement, twin, 'returnTo is found again by its id');
  // the opener is gone and nothing was named: the frame's button
  knob.focus();
  r.modal.open({ el: r.dialog('b') });
  knob.remove();
  r.modal.close();
  assert.equal(document.activeElement, menu);
  // the opener is drawn again while the modal stands and nothing was named: its twin by id
  const redrawn = document.createElement('button');
  redrawn.id = 'link';
  document.getElementById('theme').append(redrawn);
  redrawn.focus();
  r.modal.open({ el: r.dialog('b2') });
  redrawn.insertAdjacentHTML('afterend', '<button id="link">Device settings…</button>');
  redrawn.remove();
  r.modal.close();
  assert.equal(r.at(), 'link', 'the opener is found again by its id');
  assert.notEqual(document.activeElement, redrawn);
  // a place was named but is not there yet, and the opener is gone: the focus is left alone (the
  // client that named it watches for it)
  twin.focus();
  r.modal.open({ el: r.dialog('c'), returnTo: 'not-yet' });
  twin.remove();
  r.modal.close();
  assert.equal(document.activeElement, document.body);
  // the opener is still in the page but cannot take the focus (a row of the menu, hidden since):
  // the frame's button
  const row = document.createElement('button');
  document.getElementById('theme').append(row);
  row.focus();
  r.modal.open({ el: r.dialog('d') });
  row.disabled = true;
  r.modal.close();
  assert.equal(document.activeElement, menu);
  // opened with the focus nowhere: it goes back to nowhere, not to the button
  menu.blur();
  assert.equal(document.activeElement, document.body);
  r.modal.open({ el: r.dialog('e') });
  r.modal.close();
  assert.equal(document.activeElement, document.body);

  // the choice itself
  const live = { isConnected: true };
  const gone = { isConnected: false };
  const doc = { getElementById: (id) => (id === 'there' ? live : null) };
  assert.equal(returnTarget({ document: doc, returnTo: 'there', opener: gone }), live);
  assert.equal(returnTarget({ document: doc, returnTo: 'nowhere', opener: live, fallback: gone }), live);
  assert.equal(returnTarget({ document: doc, returnTo: 'nowhere', opener: gone, fallback: live }), null);
  assert.equal(returnTarget({ document: doc, returnTo: live, opener: gone }), live, 'an element names itself');
  assert.equal(returnTarget({ document: doc, opener: live, fallback: gone }), live);
  assert.equal(returnTarget({ document: doc, opener: gone, fallback: live }), live);
  assert.equal(returnTarget({ document: doc, opener: null, fallback: null }), null);
  assert.equal(returnTarget({ document: doc, opener: { ...gone, id: 'there' }, fallback: gone }), live,
    'an opener that left the page: the element that has its id now');
  assert.equal(returnTarget({ document: doc, opener: { ...gone, id: 'nowhere' }, fallback: live }), live);
  assert.equal(returnTarget({ document: doc, returnTo: 'nowhere', opener: { ...gone, id: 'there' } }), live,
    'under a named place that is not there, too');
});

await ok('close_all_empties_the_stack_from_the_top_and_tells_each_once', () => {
  const r = rig();
  const closed = [];
  r.document.getElementById('knob').focus();
  const first = r.modal.open({ el: r.dialog('a'), onClose: () => closed.push('first') });
  r.modal.open({ el: r.dialog('b'), onClose: () => closed.push('second') });
  r.modal.open({ el: r.dialog('c'), onClose: () => closed.push('third') });
  r.modal.closeAll();
  assert.deepEqual(closed, ['third', 'second', 'first']);
  assert.equal(r.modal.opened(), false);
  assert.equal(r.at(), 'knob', 'the focus is back where the first one was opened from');
  assert.equal(r.document.getElementById('theme').inert, false);
  first.close();
  r.modal.close();
  r.modal.closeAll();
  assert.deepEqual(closed, ['third', 'second', 'first'], 'a closed modal is told once');
});

await ok('an_onclose_that_throws_does_not_leave_the_rest_open', () => {
  const r = rig();
  const warned = [];
  const warn = console.warn;
  console.warn = (...args) => warned.push(args);
  try {
    const closed = [];
    r.document.getElementById('knob').focus();
    r.modal.open({ el: r.dialog('a'), onClose: () => closed.push('first') });
    r.modal.open({ el: r.dialog('b'), onClose: () => { throw new Error('boom'); } });
    r.modal.open({ el: r.dialog('c'), onClose: () => closed.push('third') });
    r.modal.closeAll();
    assert.deepEqual(closed, ['third', 'first']);
    assert.equal(r.modal.opened(), false);
    assert.equal(r.document.getElementById('theme').inert, false, 'the page is in reach');
    assert.equal(r.at(), 'knob');
    assert.equal(warned.length, 1, 'the throw is said once');
    assert.match(String(warned[0][1]), /boom/);
  } finally {
    console.warn = warn;
  }
});

await ok('a_modal_taken_out_of_the_page_is_off_the_stack', () => {
  const r = rig();
  const closed = [];
  r.document.getElementById('knob').focus();
  const el = r.dialog('a');
  const standing = r.modal.open({ el, onClose: () => closed.push('gone') });
  // something else removed it (a theme's root replaced, a redraw): no `close()` was called
  el.remove();
  assert.equal(r.modal.opened(), false, 'asked, the stack no longer counts it');
  assert.equal(standing.opened(), false);
  assert.deepEqual(closed, ['gone'], 'and its client was told');
  assert.equal(r.document.getElementById('theme').inert, false, 'the page is in reach again');
  // found at the next key too, which is then the page's
  const again = r.dialog('b');
  r.modal.open({ el: again });
  again.remove();
  r.key('m', { on: r.document.body });
  assert.deepEqual(r.heard, ['late m', 'capture m', 'document m', 'window m']);
  assert.equal(r.document.getElementById('frame').inert, false);
  // under another modal: the top stays
  const under = r.dialog('c');
  r.modal.open({ el: under });
  const top = r.dialog('d');
  r.modal.open({ el: top });
  under.remove();
  assert.equal(r.modal.opened(), true);
  assert.equal(Boolean(top.inert), false);
  assert.equal(r.at(), 'd');
});

await ok('a_modal_closed_under_another_leaves_the_top_alone_and_hands_its_place_up', () => {
  const r = rig();
  r.document.getElementById('knob').focus();
  const under = r.modal.open({ el: r.dialog('a') });
  const el = r.dialog('b');
  r.modal.open({ el });
  under.close();
  assert.equal(r.at(), 'b', 'the focus stays in the top');
  assert.equal(Boolean(el.inert), false);
  assert.equal(r.document.getElementById('theme').inert, true);
  r.modal.close();
  assert.equal(r.at(), 'knob', 'and then goes where the one under it would have sent it');
});

await ok('the_same_element_is_one_modal', () => {
  const r = rig();
  const el = r.dialog('a');
  const closed = [];
  const one = r.modal.open({ el, onClose: () => closed.push(1) });
  const two = r.modal.open({ el, onClose: () => closed.push(2) });
  two.close();
  assert.equal(one.opened(), false);
  assert.deepEqual(closed, [1]);
  assert.equal(r.modal.opened(), false);
  // opened again while it stands under another: it stays where it is, with the options it has
  const under = r.dialog('b');
  const top = r.dialog('c');
  const got = [];
  r.modal.open({ el: under, onKey: () => { got.push('first'); return true; } });
  r.modal.open({ el: top, onKey: () => { got.push('top'); return true; } });
  r.modal.open({ el: under, onKey: () => { got.push('second'); return true; } });
  assert.equal(r.at(), 'c');
  assert.equal(under.inert, true);
  r.key('x');
  r.modal.close();
  r.key('y');
  assert.deepEqual(got, ['top', 'first']);
});

// ---- the picker of themes on the stack ----

/** A rig with the picker over two themes, `T` wired as the frame wires it. */
function withPicker() {
  const r = rig();
  const picks = [];
  const core = {
    theme: { id: 'ember' },
    registry: {
      list: () => [{ id: 'ember', label: 'Ember' }, { id: 'paper', label: 'Paper' }],
      loaded: () => ({ preview() {} }),
      load: async () => ({}),
    },
  };
  const picker = createPicker({
    document: r.document, core, pick: (id) => picks.push(id), modal: r.modal, toggles: reservedKey,
  });
  const cards = () => [...r.document.querySelectorAll('#picker .tcard')].map((el) => el.dataset.theme);
  const on = () => r.document.activeElement?.dataset?.theme ?? r.document.activeElement?.className;
  return { ...r, picker, picks, cards, on };
}

await ok('the_picker_is_a_modal_on_the_stack_with_its_own_keys', () => {
  const r = withPicker();
  r.document.getElementById('knob').focus();
  r.picker.open();
  const sheet = r.document.getElementById('picker');
  assert.equal(sheet.getAttribute('role'), 'dialog');
  assert.equal(sheet.getAttribute('aria-modal'), 'true');
  assert.equal(sheet.getAttribute('aria-labelledby'), 'picker-title');
  assert.equal(r.document.getElementById('theme').inert, true);
  assert.equal(r.document.getElementById('frame').inert, true);
  assert.deepEqual(r.cards(), ['ember', 'paper']);
  assert.equal(r.on(), 'ember', 'the focus opens on the theme in use');
  assert.equal(r.key('ArrowRight').defaultPrevented, true);
  assert.equal(r.on(), 'paper');
  r.key('ArrowDown');
  assert.equal(r.on(), 'pclose', 'the close button comes after the cards');
  r.key('Tab');
  assert.equal(r.on(), 'ember');
  r.key('Tab', { shiftKey: true });
  assert.equal(r.on(), 'pclose');
  r.key('Home');
  assert.equal(r.on(), 'ember');
  r.key('End');
  assert.equal(r.on(), 'paper');
  r.key('ArrowLeft');
  r.key('ArrowUp');
  assert.equal(r.on(), 'pclose');
  for (const name of ['m', '?', ' ', 'f']) r.key(name);
  assert.deepEqual(r.heard, [], 'no key reaches the page under the picker');
  assert.equal(r.picker.opened(), true);
  r.key('Escape');
  assert.equal(r.picker.opened(), false);
  assert.equal(r.document.getElementById('picker'), null);
  assert.equal(r.at(), 'knob');
  assert.equal(r.document.getElementById('theme').inert, false);
});

await ok('t_closes_the_picker_and_a_pick_closes_it_first', () => {
  const r = withPicker();
  r.document.getElementById('knob').focus();
  r.picker.open();
  r.key('t', { repeat: true });
  assert.equal(r.picker.opened(), true, 'the key still held from the press that opened it');
  assert.equal(r.key('T').defaultPrevented, true);
  assert.equal(r.picker.opened(), false);
  r.picker.open();
  r.document.querySelector('#picker .tcard[data-theme=ember]').click();
  assert.deepEqual(r.picks, [], 'the theme in use is not picked again');
  assert.equal(r.picker.opened(), false);
  r.picker.open();
  r.document.querySelector('#picker .tcard[data-theme=paper]').click();
  assert.deepEqual(r.picks, ['paper']);
  assert.equal(r.picker.opened(), false);
  r.picker.open();
  r.document.querySelector('#picker .pclose').click();
  assert.equal(r.picker.opened(), false);
  r.picker.open();
  r.document.getElementById('picker').click();
  assert.equal(r.picker.opened(), false, 'a click outside the sheet closes it');
  // opened from a control that is drawn again meanwhile: the frame's button has the focus after
  r.document.getElementById('knob').focus();
  r.picker.open();
  r.document.getElementById('knob').remove();
  r.modal.closeAll();
  assert.equal(r.picker.opened(), false, 'closeAll closes the picker');
  assert.equal(r.at(), 'fx-button');
});

// ---- the question on the stack ----

await ok('the_question_is_a_modal_on_the_stack_and_closing_it_is_a_no', async () => {
  const r = rig();
  const ask = createAsk({ document: r.document, modal: r.modal });
  r.document.getElementById('chip').focus();
  let answer = ask('Play on both?', { returnTo: 'chip' });
  const card = () => r.document.querySelector('.ask');
  assert.equal(card().getAttribute('role'), 'dialog');
  assert.equal(card().getAttribute('aria-modal'), 'true');
  assert.equal(r.document.getElementById(card().getAttribute('aria-labelledby')).textContent, 'Play on both?');
  assert.equal(r.document.activeElement.className, 'ask-button ask-no', 'the focus is on No');
  assert.equal(r.document.getElementById('theme').inert, true);
  assert.equal(Boolean(r.document.getElementById('toasts').inert), false);
  for (const name of ['ArrowLeft', 'ArrowRight', 'm', '0', 'f', 'p', 't', '?', ' ']) r.key(name);
  assert.deepEqual(r.heard, [], 'no key writes behind the question');
  assert.equal(ask.opened(), true);
  assert.equal(r.key('Tab').defaultPrevented, true);
  assert.equal(r.document.activeElement.className, 'ask-button ask-yes', 'Tab from No, the last, is Yes');
  assert.equal(r.key('Tab').defaultPrevented, false, 'from Yes to No is the browser\'s own Tab');
  r.key('Tab', { shiftKey: true });
  assert.equal(r.document.activeElement.className, 'ask-button ask-no');
  // `y` and `n` answer only from inside the card, never with a modifier
  r.key('y', { on: r.document.body });
  r.key('y', { ctrlKey: true });
  assert.equal(ask.opened(), true);
  r.key('y');
  assert.equal(await answer, true);
  assert.equal(card(), null);
  assert.equal(r.at(), 'chip');
  assert.equal(r.document.getElementById('theme').inert, false);

  answer = ask('Again?');
  r.key('N');
  assert.equal(await answer, false);
  answer = ask('Again?');
  r.key('Escape');
  assert.equal(await answer, false, 'Escape is No');
  answer = ask('Again?');
  card().querySelector('.ask-yes').click();
  assert.equal(await answer, true);
  answer = ask('Again?');
  r.modal.closeAll();
  assert.equal(await answer, false, 'a question closed with the stack is answered No');
  answer = ask('First?');
  const second = ask('Second?');
  assert.equal(await answer, false, 'a second question answers the first No');
  assert.equal(r.document.querySelectorAll('.ask').length, 1);
  ask.close();
  assert.equal(await second, false);
  assert.equal(ask.opened(), false);
  assert.equal(r.modal.opened(), false);
});

console.log(`core-modal.mjs: ${n} passed`);
