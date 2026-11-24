/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// What a press on the computer's side does on the theme Ember's page: the switch, the mode, the
// presets, the table of points (a row picked, a value typed or stepped, the type chosen, a point
// removed or added) and the curve under the mouse and the finger. The drag, the tap, the touch, the
// wheel, the table's scroll and the typed cell are the core's (`core.eq`): this module forwards
// the DOM's events to them and draws what they say (computer-page.js moves the curve while a drag
// or a stepper is on; computer-steps.js is the steppers). Every element carries `data-comp` and
// none carries `data-json`, so press.js leaves them alone.

import { ember, mine } from './bind.js';
import { moreWords } from './draw/computer.js';
import {
  followDrag, hideDrag, off, paintEditor, patch, placeLabel, showDrag, tone, usable,
} from './computer-page.js';
import {
  installSteps, pressed, pressedByKey, stepping,
} from './computer-steps.js';
import { holdTone } from './keep.js';
import { REACH, grab } from '../../core/tone-drag.js';
import { CROSSFEED, LOUDNESS, SOUND, SPEAKERS } from '../../core/popups/listed.js';

/** Draws the parts whose swap waited (live.js's), given to [`installComputer`]. */
let readWaiting = () => {};
/** The core's hand on the curve and its table, made when the theme is mounted. */
let hand = null;
let table = null;
/** The plot the cursor is over with the grab cursor on it. */
let hovered = null;

const core = () => ember.core;

/** The Sound popup's page for a stage word of the strip: the EQ library is the popup's first tab
 * (no page), and Space is the virtual speakers' tab while they play, else the crossfeed's. */
function soundPage(stage) {
  if (stage === 'loudness') return LOUDNESS;
  if (stage !== 'space') return null;
  return core().computer.levelsOf(core().model.page())?.stages.find((s) => s.key === 'space')?.kind
    === 'speakers' ? SPEAKERS : CROSSFEED;
}
const act = () => ember.core.actions.computer;

const plotOf = (target) => target.closest?.('.eqg.pts .plot') ?? null;
const pointsOf = (plot, rect) => core().eq.points.markersIn(tone()?.markers, rect);

// ------------------------------------------------------------------------------- the table

const scroller = () => document.querySelector('#part-tone .pscroll');

/** Light row `n` of the table and the editor on its point. */
function lit(n) {
  for (const row of document.querySelectorAll('#part-tone .pt')) {
    row.classList.toggle('on', Number(row.dataset.n) === n);
  }
  paintEditor();
}

/** Whether a point of the computer's curve is held. */
export const dragging = () => hand?.active === true;

/** A screen or the tone part is in place: the table keeps its scroll, the picked row shows. */
export function syncComputer() {
  table?.restore();
}

// ------------------------------------------------------------------------------ the dragging

const touch = { move: (event) => hand.touchMove(event), end: (event) => hand.touchEnd(event) };

/** What the core's hand tells the page: the drag begins, moves the point, ends and is answered. */
const heard = {
  pick: (n) => table.pick(n),
  begin(s) {
    holdTone(true);
    showDrag(s, touch);
  },
  held: placeLabel,
  ended: (s) => hideDrag(s, touch),
  finished() {
    if (ember.signal?.aborted) return;
    holdTone(false, true);
    readWaiting();
  },
  // a wheel, key or stepper burst holds the part's swap until its last value is written
  burst(b, on) {
    if (on) {
      holdTone(true);
      return;
    }
    holdTone(false, true);
    readWaiting();
  },
  // a tap on a full table says how many the EQ holds at most
  added(answer) {
    if (answer?.full) ember.core?.frame.toast(answer.error);
  },
  moved() {
    if (hand.session && !hand.session.over) followDrag(hand.session);
  },
};

function onDown(event) {
  if (pressed(event)) return;
  hand.finishBurst();
  const plot = plotOf(event.target);
  if (event.pointerType === 'touch') {
    // a finger on a point is the touch events' (the page does not scroll for it); elsewhere on the
    // plot it may be a tap
    if (!plot) return;
    const rect = plot.getBoundingClientRect();
    const at = { x: event.clientX - rect.left, y: event.clientY - rect.top };
    if (grab(pointsOf(plot, rect), at, { reach: REACH.touch }) === null) {
      hand.down(event, plot, { grabs: false });
    }
  } else if (event.button === 0 || event.button === 2) {
    hand.down(event, plot, { wide: event.button === 2 });
  }
}

function onMove(event) {
  if (hand.move(event)) return;
  const plot = event.pointerType === 'touch' ? null : plotOf(event.target);
  if (hovered && hovered !== plot) hovered.classList.remove('hot');
  hovered = plot;
  plot?.classList.toggle('hot', hand.canGrab(event, plot));
}

// ----------------------------------------------------------------------------------- the table

/** Turn a cell into a field: Enter writes the value typed (the core holds it to the hand ranges),
 * Esc or leaving puts the text back. */
function edit(cell) {
  if (cell.querySelector('input')) return;
  const input = document.createElement('input');
  input.inputMode = 'decimal';
  input.setAttribute('aria-label', cell.getAttribute('aria-label'));
  const shown = cell.textContent;
  input.value = shown.replace(/\s*(Hz|dB)$/, '');
  cell.replaceChildren(input);
  input.focus();
  input.select();
  core().eq.table.editCell(input, {
    n: Number(cell.dataset.n),
    param: cell.dataset.param,
    shown,
    cell: act().cell,
    restore: () => {
      if (cell.isConnected) cell.textContent = shown;
    },
    after: readWaiting,
  }).listen();
}

/** The id of the switch's half the focus is owed to after a keyboard flip. */
let wantFocus = null;
function giveFocus() {
  if (wantFocus && document.activeElement?.id !== wantFocus) {
    document.getElementById(wantFocus)?.focus({ preventScroll: true });
  }
}

/** A form of the presets' list is sent: save the points on show, rename or delete a preset (the
 * computer's list is read again after a success). */
async function submitted(event) {
  const form = event.target.closest?.('form[data-comp="form"]');
  if (!form) return;
  event.preventDefault();
  const value = (name) => form.elements[name]?.value ?? '';
  form.dataset.sending = '';
  let answer = null;
  try {
    if (form.dataset.form === 'rename') answer = await act().renamePreset(form.dataset.from, value('to'));
    else if (form.dataset.form === 'delete') answer = await act().deletePreset(form.dataset.from);
    else answer = await act().savePreset(value('name'));
  } finally {
    if (!answer?.ok) delete form.dataset.sending;
    else readWaiting();
  }
}

/** The table's and the controls' presses. */
function clicked(event) {
  const switcher = event.target.closest('#part-switch .opt[data-side]');
  if (switcher) {
    // a press from the keyboard keeps the focus on the chosen half across the redraw
    const side = switcher.dataset.side;
    wantFocus = event.detail === 0 ? `side-${side}` : null;
    core().computer.flip(side);
    giveFocus();
    setTimeout(() => {
      giveFocus();
      wantFocus = null;
    }, 60);
    return;
  }
  const control = event.target.closest('[data-comp]');
  const row = event.target.closest('#part-tone .pt');
  if (!control) {
    if (row && !event.target.closest('select, input')) table.pick(Number(row.dataset.n));
    return;
  }
  if (off(control) || !ember.core.actions.canWrite()) return;
  const n = Number(control.dataset.n);
  switch (control.dataset.comp) {
    case 'mode':
      if (control.getAttribute('aria-pressed') !== 'true') act().setMode(control.dataset.mode);
      break;
    case 'bypass':
      act().setBypass(control.getAttribute('aria-pressed') !== 'true');
      break;
    case 'stage':
      core().router.open(SOUND, soundPage(control.dataset.stage));
      break;
    case 'preset':
      // the question's focus goes back to this chip, found again by its id after a redraw
      act().recallPreset(control.dataset.name, {
        asker: (question) => core().ask(question, { returnTo: control.id }),
      });
      break;
    case 'add':
      act().addNext();
      break;
    case 'remove':
      act().removePoint(n);
      break;
    case 'cell':
      table.pick(n, { scroll: false });
      edit(control);
      break;
    case 'step':
      // a press from the keyboard (a pointer's press was heard as it went down): one step
      if (event.detail === 0) pressedByKey(control);
      break;
    default:
  }
}

/** A type chosen in a row. */
function chosen(event) {
  const select = event.target.closest?.('#part-tone select[data-comp="type"]');
  if (!select || off(select)) return;
  const n = Number(select.dataset.n);
  const { value } = select;
  select.blur();
  table.pick(n, { scroll: false });
  act().setPoint(n, { type: value }, { done: true });
}

/** Listen for the presses on the computer's controls and the mouse and the finger on its curve;
 * `given.readWaiting` is called when a hold ends; the listeners go with the mount's signal. */
export function installComputer(given) {
  readWaiting = given.readWaiting;
  hand = core().eq.points({ points: pointsOf, usable, on: heard });
  installSteps({ hand });
  table = core().eq.table({
    box: scroller,
    note: () => document.querySelector('#part-tone .more'),
    row: (n) => document.querySelector(`#part-tone .pt[data-n="${n}"]`),
    lit,
    moreWords,
    rowTop: (el, box) => el.offsetTop - box.offsetTop,
  });
  document.addEventListener('click', clicked, mine());
  document.addEventListener('change', chosen, mine());
  document.addEventListener('submit', submitted, mine());
  // any press at all, anywhere on the page (a second finger beside the graph), makes a waiting tap no tap:
  // before the graph's own press, which may begin one
  document.addEventListener('pointerdown', () => hand.dropTap(), mine({ capture: true }));
  document.addEventListener('pointerdown', onDown, mine());
  document.addEventListener('pointermove', onMove, mine());
  document.addEventListener('pointerup', (event) => hand.up(event), mine());
  for (const name of ['pointercancel', 'lostpointercapture']) {
    document.addEventListener(name, (event) => hand.cancel(event), mine());
  }
  document.addEventListener('touchstart', (event) => hand.touchStart(event, plotOf(event.target)),
    mine({ passive: false }));
  document.addEventListener('wheel', (event) => hand.wheel(event, plotOf(event.target)),
    mine({ passive: false }));
  document.addEventListener('keydown', (event) => {
    if (event.key === 'Escape') hand.escape();
  }, mine({ capture: true }));
  document.addEventListener('svolder-lost', (event) => {
    if (event.detail) hand.lost();
  }, mine());
  window.addEventListener('blur', () => hand.blur(), mine());
  document.addEventListener('scroll', (event) => {
    if (event.target === scroller()) table.scrolled();
  }, mine({ capture: true, passive: true }));

  // The table keeps its scroll across the part's swap; a drag in motion moves what is drawn.
  document.body.addEventListener('htmx:oobBeforeSwap', (event) => {
    if (event.detail.target?.id === 'part-tone') table.keep();
  }, mine());
  document.body.addEventListener('htmx:oobAfterSwap', (event) => {
    if (event.detail.target?.id === 'part-switch') giveFocus();
    if (event.detail.target?.id === 'part-tone') syncComputer();
  }, mine());
  const stop = core().computer.on(() => {
    if (hand.session || stepping()) patch();
  });
  ember.signal?.addEventListener('abort', () => {
    stop();
    hand.destroy();
    hovered = null;
  }, { once: true });
}
