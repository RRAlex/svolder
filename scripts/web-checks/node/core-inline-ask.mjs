/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for core/inline-ask.js, the form for a name and the question that asks once, which
// every theme and the popups draw in a place of their own: the field takes its typing, the form
// submits and cancels, a refused answer leaves it open with the text kept, a question asks once,
// the classes and words passed are the ones drawn, one ask replaces the one before, and the keys
// are handed in (inside a modal of the real stack, outside by `listen`). Needs jsdom (apt
// `node-jsdom`). Run: node core-inline-ask.mjs
import assert from 'node:assert/strict';
import { page } from './dom.mjs';
import { claim, createModals } from '../../../bins/svolderd/web/core/modal.js';
import {
  askForm, askQuestion, closeAsk, FRAME_CLASSES, listen, openAsks,
} from '../../../bins/svolderd/web/core/inline-ask.js';

let n = 0;
const ok = async (name, fn) => { await fn(); n++; console.log('ok', name); };

const WORDS = {
  presetName: 'Preset name', newName: 'New name',
  answers: { save: 'Save', rename: 'Rename', delete: 'Delete', keep: 'Keep', cancel: 'Cancel' },
};
const OWN = {
  box: 'x-box', form: 'x-form', input: 'x-input', button: 'x-btn', set: 'x-set',
  question: 'x-q', danger: 'danger',
};

function rig() {
  const p = page('<div id="within"></div><button id="opener">open</button>');
  const { document, window } = p;
  const within = document.getElementById('within');
  const opener = document.getElementById('opener');
  return { ...p, within, opener };
}
const type = (input, text, window) => {
  input.value = text;
  input.dispatchEvent(new window.Event('input', { bubbles: true }));
};
const submitForm = (box) => box.querySelector('form').requestSubmit();
const key = (window, name, on) => {
  const event = new window.KeyboardEvent('keydown', { key: name, bubbles: true, cancelable: true });
  on.dispatchEvent(event);
  return event;
};

await ok('a_form_draws_the_classes_and_words_it_is_given', async () => {
  const r = rig();
  const ask = askForm({
    within: r.within, kind: 'rename', value: 'Old', submit: async () => ({ ok: true }),
    classes: OWN, words: WORDS, between: ' &middot; ',
  });
  assert.equal(ask.box.className, 'x-box');
  assert.equal(ask.box.innerHTML,
    '<form class="x-form"><input class="x-input" type="text" maxlength="40" required=""'
    + ' autocomplete="off" aria-label="New name" aria-invalid="false" value="Old">'
    + ' <button type="submit" class="x-btn"'
    + ' data-answer="rename">Rename</button> · <button type="button" class="x-btn"'
    + ' data-answer="cancel">Cancel</button></form>');
  ask.close();
});

await ok('a_form_without_classes_is_dressed_by_the_frames_classes', async () => {
  const r = rig();
  const ask = askForm({ within: r.within, kind: 'save', submit: async () => ({ ok: true }), words: WORDS });
  assert.equal(ask.box.className, FRAME_CLASSES.box);
  assert.ok(ask.box.querySelector(`input.${FRAME_CLASSES.input}`));
  assert.equal(ask.box.querySelector('input').getAttribute('aria-label'), 'Preset name');
  ask.close();
});

await ok('the_field_has_the_focus_and_its_text_selected_and_takes_typing', async () => {
  const r = rig();
  const ask = askForm({
    within: r.within, kind: 'save', value: 'abc', submit: async () => ({ ok: true }), words: WORDS,
  });
  const input = ask.box.querySelector('input');
  assert.equal(r.document.activeElement, input);
  assert.equal(input.selectionStart, 0);
  assert.equal(input.selectionEnd, 3);
  type(input, 'Warm', r.window);
  assert.equal(input.value, 'Warm');
  ask.close();
});

await ok('the_form_submits_its_text_and_closes_on_ok_giving_the_focus_back', async () => {
  const r = rig();
  const sent = [];
  let closed = 0;
  r.opener.focus();
  const ask = askForm({
    within: r.within, kind: 'save', opener: r.opener, words: WORDS, onClose: () => closed++,
    submit: async (text) => { sent.push(text); return { ok: true }; },
  });
  type(ask.box.querySelector('input'), 'Warm', r.window);
  submitForm(ask.box);
  await r.settle();
  assert.deepEqual(sent, ['Warm']);
  assert.equal(r.within.children.length, 0, 'closed');
  assert.equal(closed, 1);
  assert.equal(r.document.activeElement, r.opener);
});

await ok('a_refused_form_stays_open_with_its_text_and_can_be_sent_again', async () => {
  const r = rig();
  let calls = 0;
  const ask = askForm({
    within: r.within, kind: 'save', words: WORDS,
    submit: async () => { calls++; return calls === 1 ? { ok: false } : { ok: true }; },
  });
  const input = ask.box.querySelector('input');
  type(input, 'Warm', r.window);
  submitForm(ask.box);
  await r.settle();
  assert.equal(r.within.children.length, 1, 'still open');
  assert.equal(input.value, 'Warm');
  assert.equal(input.disabled, false);
  assert.equal(r.document.activeElement, input);
  submitForm(ask.box);
  await r.settle();
  assert.equal(r.within.children.length, 0);
});

await ok('an_empty_name_is_not_sent_and_a_throwing_submit_leaves_the_form_open', async () => {
  const r = rig();
  let calls = 0;
  const ask = askForm({
    within: r.within, kind: 'save', words: WORDS,
    submit: async () => { calls++; throw new Error('x'); },
  });
  type(ask.box.querySelector('input'), '', r.window);
  submitForm(ask.box);
  await r.settle();
  assert.equal(calls, 0, 'required');
  type(ask.box.querySelector('input'), 'a', r.window);
  submitForm(ask.box);
  await r.settle();
  assert.equal(calls, 1);
  assert.equal(r.within.children.length, 1);
  ask.close();
});

await ok('cancel_closes_the_form_and_sends_nothing', async () => {
  const r = rig();
  let calls = 0;
  const ask = askForm({
    within: r.within, kind: 'save', words: WORDS, submit: async () => { calls++; return { ok: true }; },
  });
  ask.box.querySelector('[data-answer="cancel"]').click();
  assert.equal(r.within.children.length, 0);
  assert.equal(calls, 0);
});

await ok('the_question_is_read_by_focus_both_answers_are_described_by_it', async () => {
  const r = rig();
  const make = () => askQuestion({
    within: r.within, text: 'Delete it?', yes: 'delete', go: async () => ({ ok: true }),
    classes: OWN, words: WORDS,
  });
  const ask = make();
  const question = ask.box.querySelector('.x-q');
  assert.equal(question.getAttribute('role'), null, 'no alert: the answer that has the focus reads it');
  assert.match(question.id, /^ask-question-\d+$/);
  assert.equal(question.textContent, 'Delete it?');
  const answers = [...ask.box.querySelectorAll('[data-answer]')];
  assert.deepEqual(answers.map((one) => one.getAttribute('aria-describedby')), [question.id, question.id]);
  assert.equal(r.document.activeElement, answers[1], 'the safe answer has the focus');
  // a theme's classes get the same markup, and the next ask an id of its own
  const next = make();
  const id = next.box.querySelector('.x-q').id;
  assert.notEqual(id, question.id, 'the id is the ask\'s own');
  assert.equal(r.document.querySelectorAll(`#${id}`).length, 1);
  next.close();
});

await ok('the_name_field_says_it_is_invalid_only_once_the_browser_does', async () => {
  const r = rig();
  const sent = [];
  const ask = askForm({
    within: r.within, kind: 'save', submit: async (text) => { sent.push(text); return { ok: false }; },
    classes: OWN, words: WORDS,
  });
  const input = ask.box.querySelector('input');
  assert.equal(input.getAttribute('aria-invalid'), 'false', 'an empty required field opens as not invalid');
  submitForm(ask.box);
  await r.settle();
  assert.deepEqual(sent, [], 'the empty name is not sent');
  assert.equal(input.getAttribute('aria-invalid'), 'true', "the field's invalid event says so");
  type(input, 'A', r.window);
  assert.equal(input.getAttribute('aria-invalid'), 'false', 'typing takes it back');
  type(input, '', r.window);
  assert.equal(input.getAttribute('aria-invalid'), 'true', 'emptied again, it is invalid again');
  type(input, 'A', r.window);
  assert.equal(input.className, 'x-input', "a theme's field gets the attribute, no class of the frame's");
  ask.close();
});

await ok('a_question_asks_once_and_closes_on_ok', async () => {
  const r = rig();
  let calls = 0;
  const ask = askQuestion({
    within: r.within, text: 'Delete it?', yes: 'delete', words: WORDS, classes: OWN,
    go: async () => { calls++; return { ok: true }; },
  });
  assert.equal(ask.box.querySelector('.x-q').textContent, 'Delete it?');
  assert.ok(ask.box.querySelector('[data-answer="delete"]').classList.contains('danger'));
  assert.equal(r.document.activeElement, ask.box.querySelector('[data-answer="keep"]'));
  const yes = ask.box.querySelector('[data-answer="delete"]');
  yes.click();
  yes.click();
  await r.settle();
  assert.equal(calls, 1, 'resolved once');
  assert.equal(r.within.children.length, 0);
});

await ok('a_refused_question_stays_open_and_a_save_is_not_dressed_as_danger', async () => {
  const r = rig();
  const ask = askQuestion({
    within: r.within, text: 'Replace?', yes: 'save', words: WORDS, classes: OWN,
    go: async () => ({ ok: false }),
  });
  assert.ok(!ask.box.querySelector('[data-answer="save"]').classList.contains('danger'));
  ask.box.querySelector('[data-answer="save"]').click();
  await r.settle();
  assert.equal(r.within.children.length, 1);
  ask.close();
});

await ok('no_closes_the_question_and_sends_nothing', async () => {
  const r = rig();
  let calls = 0;
  const ask = askQuestion({
    within: r.within, text: 'Delete it?', yes: 'delete', words: WORDS,
    go: async () => { calls++; return { ok: true }; },
  });
  ask.box.querySelector('[data-answer="keep"]').click();
  assert.equal(r.within.children.length, 0);
  assert.equal(calls, 0);
});

await ok('a_second_ask_in_the_same_box_replaces_the_first', async () => {
  const r = rig();
  let closed = 0;
  askForm({ within: r.within, kind: 'save', words: WORDS, submit: async () => ({}), onClose: () => closed++ });
  const second = askQuestion({ within: r.within, text: 'Q', yes: 'delete', words: WORDS, go: async () => ({}) });
  assert.equal(closed, 1);
  assert.equal(r.within.children.length, 1);
  assert.equal(r.within.firstChild, second.box);
  closeAsk();
  assert.equal(r.within.children.length, 0);
});

await ok('the_signal_aborting_closes_the_ask', async () => {
  const r = rig();
  const stop = new AbortController();
  askForm({ within: r.within, kind: 'save', words: WORDS, submit: async () => ({}), signal: stop.signal });
  stop.abort();
  assert.equal(r.within.children.length, 0);
});

await ok('the_part_attaches_no_key_listener_and_key_hears_escape_alone', async () => {
  const r = rig();
  let calls = 0;
  const ask = askForm({ within: r.within, kind: 'save', words: WORDS, submit: async () => { calls++; return {}; } });
  const input = ask.box.querySelector('input');
  key(r.window, 'Escape', input);
  assert.equal(r.within.children.length, 1, 'no listener of its own: Escape on the page does nothing');
  assert.equal(ask.key(new r.window.KeyboardEvent('keydown', { key: 'a' })), false);
  assert.equal(ask.key(new r.window.KeyboardEvent('keydown', { key: 'Enter' })), false);
  const escape = new r.window.KeyboardEvent('keydown', { key: 'Escape', cancelable: true });
  assert.equal(ask.key(escape), true);
  assert.equal(escape.defaultPrevented, true);
  assert.equal(r.within.children.length, 0, 'Escape cancels');
  assert.equal(calls, 0);
});

await ok('listen_hands_the_keys_of_the_box_to_key_and_ends_with_the_ask', async () => {
  const r = rig();
  const ask = askQuestion({ within: r.within, text: 'Q', yes: 'delete', words: WORDS, go: async () => ({}) });
  listen(ask);
  const event = key(r.window, 'Escape', ask.box.querySelector('[data-answer="keep"]'));
  assert.equal(event.defaultPrevented, true);
  assert.equal(r.within.children.length, 0);
});

await ok('inside_a_modal_of_the_real_stack_the_modal_hands_escape_to_the_ask_and_stays_open', async () => {
  const r = rig();
  r.document.body.insertAdjacentHTML('beforeend', '<div id="pop"><div id="in"></div><button id="b">x</button></div>');
  const modals = createModals({ document: r.document, fallback: () => r.opener });
  const pop = r.document.getElementById('pop');
  let ask = null;
  modals.open({
    el: pop, focus: () => r.document.getElementById('b'), onKey: (event) => ask?.key(event) === true,
  });
  let calls = 0;
  ask = askForm({
    within: r.document.getElementById('in'), kind: 'save', words: WORDS,
    submit: async () => { calls++; return { ok: true }; },
  });
  r.window.addEventListener('keydown', (event) => modals.key(event), true);
  const input = ask.box.querySelector('input');
  type(input, 'abc', r.window);
  assert.equal(key(r.window, 'a', input).defaultPrevented, false, 'typing is never prevented');
  const escape = key(r.window, 'Escape', input);
  assert.equal(escape.defaultPrevented, true);
  assert.equal(r.document.getElementById('in').children.length, 0, 'the ask is cancelled');
  assert.equal(modals.opened(), true, 'the modal stays open');
  assert.equal(calls, 0);
  key(r.window, 'Escape', r.document.getElementById('b'));
  assert.equal(modals.opened(), false, 'the next Escape closes the modal');
});

/** A page with a form open on it and a modal of the real stack with a place for an ask. */
function scopes() {
  const r = rig();
  closeAsk();
  r.document.body.insertAdjacentHTML('beforeend',
    '<div id="pop"><div id="in"></div><div id="in2"></div><button id="b">x</button></div>');
  const modals = createModals({ document: r.document, fallback: () => r.opener });
  const pop = r.document.getElementById('pop');
  const closed = { page: 0, pop: 0 };
  const form = (within, who) => askForm({
    within, kind: 'save', words: WORDS, submit: async () => ({ ok: true }),
    onClose: () => { closed[who]++; },
  });
  const onPage = form(r.within, 'page');
  const modal = modals.open({ el: pop });
  return { ...r, modals, modal, pop, closed, form, onPage, inPop: r.document.getElementById('in') };
}

await ok('an_ask_in_a_modal_and_one_on_the_page_do_not_close_each_other', async () => {
  const r = scopes();
  type(r.onPage.box.querySelector('input'), 'typed', r.window);
  const inPop = r.form(r.inPop, 'pop');
  assert.equal(r.closed.page, 0, "the modal's ask leaves the page's form open");
  assert.equal(r.onPage.box.isConnected, true);
  assert.equal(r.onPage.box.querySelector('input').value, 'typed', 'with what was typed');
  assert.equal(openAsks(), 2);
  const second = r.form(r.within, 'page');
  assert.equal(r.closed.page, 1, 'an ask on the page replaces the one on the page');
  assert.equal(r.closed.pop, 0, "and leaves the modal's open");
  assert.equal(inPop.box.isConnected, true);
  closeAsk();
  assert.equal(second.box.isConnected, false, "closeAsk() closes the page's");
  assert.equal(r.closed.pop, 0, "and not the modal's (a stream event on the page)");
  closeAsk(r.inPop);
  assert.equal(r.closed.pop, 1, "closeAsk(an element of the modal) closes the modal's");
  assert.equal(openAsks(), 0);
  r.modal.close();
});

await ok('two_asks_in_one_modal_replace_each_other', async () => {
  const r = scopes();
  const first = r.form(r.inPop, 'pop');
  const second = r.form(r.document.getElementById('in2'), 'pop');
  assert.equal(first.box.isConnected, false);
  assert.equal(r.closed.pop, 1);
  assert.equal(second.box.isConnected, true);
  assert.equal(r.closed.page, 0);
  second.close();
  r.onPage.close();
  r.modal.close();
  assert.equal(openAsks(), 0);
});

await ok('closing_a_modal_closes_the_ask_inside_it_and_leaves_nothing_behind', async () => {
  const r = scopes();
  const inPop = r.form(r.inPop, 'pop');
  r.modal.close();
  assert.equal(r.closed.pop, 1, "the ask's onClose ran");
  assert.equal(inPop.box.isConnected, false);
  assert.equal(r.closed.page, 0, "the page's form stays");
  assert.equal(openAsks(), 1, "only the page's is left");
  assert.equal(r.document.activeElement, r.onPage.box.querySelector('input'),
    'the focus went where the modal gives it, not to the ask');
  // a modal that something else took out of the page
  const again = r.modals.open({ el: r.pop });
  r.form(r.inPop, 'pop');
  r.pop.remove();
  assert.equal(again.opened(), false);
  assert.equal(r.closed.pop, 2);
  r.onPage.close();
  assert.equal(openAsks(), 0);
});

await ok('a_sheet_a_theme_marks_modal_inside_its_page_is_the_pages_scope', async () => {
  const r = rig();
  closeAsk();
  r.within.innerHTML = '<div role="dialog" aria-modal="true"><div id="deep"></div></div>';
  let closed = 0;
  askForm({
    within: r.document.getElementById('deep'), kind: 'save', words: WORDS,
    submit: async () => ({ ok: true }), onClose: () => { closed++; },
  });
  closeAsk();
  assert.equal(closed, 1, 'only a modal of the stack (a child of the body) is a scope of its own');
});

await ok('enter_in_the_field_is_left_to_the_browser_which_submits_the_form', async () => {
  const r = rig();
  const sent = [];
  const ask = askForm({
    within: r.within, kind: 'save', words: WORDS, submit: async (t) => { sent.push(t); return { ok: true }; },
  });
  const input = ask.box.querySelector('input');
  type(input, 'Warm', r.window);
  const enter = new r.window.KeyboardEvent('keydown', { key: 'Enter', cancelable: true });
  assert.equal(ask.key(enter), false);
  assert.equal(enter.defaultPrevented, false, 'the part never prevents Enter');
  // what the browser does with that Enter: it fires the form's submit event
  const submit = new r.window.Event('submit', { bubbles: true, cancelable: true });
  ask.box.querySelector('form').dispatchEvent(submit);
  await r.settle();
  assert.deepEqual(sent, ['Warm']);
  assert.equal(submit.defaultPrevented, true, 'the page does not navigate');
});

await ok('the_default_between_is_one_space_and_a_question_has_it_too', async () => {
  const r = rig();
  const form = askForm({ within: r.within, kind: 'save', words: WORDS, submit: async () => ({}) });
  assert.ok(form.box.innerHTML.includes('Save</button> <button'), form.box.innerHTML);
  const q = askQuestion({ within: r.within, text: 'Q', yes: 'delete', words: WORDS, go: async () => ({}) });
  assert.ok(q.box.innerHTML.includes('Delete</button> <button'), q.box.innerHTML);
  closeAsk();
});

await ok('the_scroll_option_brings_the_ask_into_view_and_is_off_by_default', async () => {
  const r = rig();
  const seen = [];
  const proto = r.window.HTMLElement.prototype;
  proto.scrollIntoView = function scroll(options) { seen.push([this.className, options]); };
  askForm({ within: r.within, kind: 'save', words: WORDS, classes: OWN, submit: async () => ({}) });
  assert.deepEqual(seen, []);
  askQuestion({ within: r.within, text: 'Q', yes: 'delete', words: WORDS, classes: OWN, scroll: true, go: async () => ({}) });
  assert.deepEqual(seen, [['x-box', { block: 'nearest' }]]);
  askForm({ within: r.within, kind: 'save', words: WORDS, classes: OWN, scroll: true, submit: async () => ({}) });
  assert.equal(seen.length, 2);
  closeAsk();
});

await ok('the_focus_goes_back_to_the_opener_found_by_its_id_after_a_redraw', async () => {
  const r = rig();
  const ask = askQuestion({
    within: r.within, text: 'Q', yes: 'delete', words: WORDS, opener: r.opener, go: async () => ({}),
  });
  const twin = r.opener.cloneNode(true);
  r.opener.replaceWith(twin);
  assert.equal(r.opener.isConnected, false);
  ask.close();
  assert.equal(r.document.activeElement, twin);
});

await ok('a_closed_ask_hears_no_key', async () => {
  const r = rig();
  const first = askForm({ within: r.within, kind: 'save', words: WORDS, submit: async () => ({}) });
  askQuestion({ within: r.within, text: 'Q', yes: 'delete', words: WORDS, go: async () => ({}) });
  const escape = new r.window.KeyboardEvent('keydown', { key: 'Escape', cancelable: true });
  assert.equal(first.key(escape), false);
  assert.equal(escape.defaultPrevented, false);
  closeAsk();
});

/** A page with a form open on it and a popup that is not on the stack yet: its element is claimed
 * for the stack (as the router's `register` does) and detached (`claimed: false`: only detached), with a place for an ask. */
function ahead({ claimed = true } = {}) {
  const r = rig();
  closeAsk();
  const modals = createModals({ document: r.document, fallback: () => r.opener });
  const pop = r.document.createElement('div');
  pop.innerHTML = '<div id="in"></div><button id="b">x</button>';
  if (claimed) claim(pop);
  const closed = { page: 0, pop: 0 };
  const form = (within, who) => askForm({
    within, kind: 'save', words: WORDS, submit: async () => ({ ok: true }),
    onClose: () => { closed[who]++; },
  });
  const onPage = form(r.within, 'page');
  return { ...r, modals, pop, closed, form, onPage, inPop: pop.querySelector('#in') };
}

await ok('an_ask_drawn_before_the_stack_has_its_popup_is_the_popups', async () => {
  const r = ahead();
  // what a popup's show(page) does: it draws its ask, and only then the stack opens the popup
  const inPop = r.form(r.inPop, 'pop');
  assert.equal(r.closed.page, 0, "the page's form stays open");
  assert.equal(r.onPage.box.isConnected, true);
  assert.equal(openAsks(), 2);
  const modal = r.modals.open({ el: r.pop });
  closeAsk();
  assert.equal(r.closed.page, 1, "closeAsk() closes the page's");
  assert.equal(r.closed.pop, 0, "and not the popup's");
  modal.close();
  assert.equal(r.closed.pop, 1, 'the modal closing closes the ask drawn before it opened');
  assert.equal(inPop.box.isConnected, false);
  assert.equal(openAsks(), 0);
});

await ok('close_ask_from_a_modals_on_close_closes_the_popups_and_not_the_pages', async () => {
  // the stack's own `open` makes the element a scope: nothing claimed it ahead
  const r = ahead({ claimed: false });
  // an ask that outlives its modal's leaving: it was opened as the modal left
  let late = null;
  const modal = r.modals.open({
    el: r.pop,
    onClose: () => {
      late = r.form(r.inPop, 'pop');
      assert.equal(r.pop.isConnected, false, 'the stack has taken the popup out of the page');
      closeAsk(r.pop);
    },
  });
  modal.close();
  assert.equal(late.box.isConnected, false, "closeAsk(el) closed the popup's");
  assert.equal(r.closed.pop, 1);
  assert.equal(r.closed.page, 0, "the page's form stays open");
  assert.equal(r.onPage.box.isConnected, true);
  r.onPage.close();
  assert.equal(openAsks(), 0);
});

await ok('a_detached_section_of_a_theme_is_the_pages_scope', async () => {
  const r = ahead();
  const section = r.document.createElement('section');
  section.innerHTML = '<div id="s"></div>';
  r.form(section.firstChild, 'page');
  assert.equal(r.closed.page, 1, 'an ask in an element no stack was given replaces the page\'s');
  closeAsk();
  assert.equal(r.closed.page, 2);
  assert.equal(openAsks(), 0);
});

await ok('an_ask_closed_by_its_modal_or_its_signal_does_not_take_the_focus', async () => {
  const r = scopes();
  let focused = 0;
  r.opener.focus = () => { focused++; };
  askForm({
    within: r.inPop, kind: 'save', words: WORDS, submit: async () => ({ ok: true }), opener: r.opener,
  });
  r.modal.close();
  assert.equal(focused, 0, 'the modal gives the focus back, not the ask');
  const stop = new AbortController();
  askForm({
    within: r.within, kind: 'save', words: WORDS, submit: async () => ({ ok: true }),
    opener: r.opener, signal: stop.signal,
  });
  stop.abort();
  assert.equal(focused, 0, 'nor when the page goes');
  const mine = askForm({
    within: r.within, kind: 'save', words: WORDS, submit: async () => ({ ok: true }), opener: r.opener,
  });
  mine.close();
  assert.equal(focused, 1, 'closed by its own word it does');
  assert.equal(openAsks(), 0);
});

console.log(`${n} core-inline-ask checks passed`);
