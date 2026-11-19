/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The Devices popup's *+ Add a device* page, drawn: model in, string out. Pure: no page, no core.
// The page draws the daemon's prompt and nothing of its own (its title, its body lines, its
// choices, a field of the kind it says it takes, the default an empty line stands for) and the
// words the daemon gave for a refusal; the few words below are the page's own chrome (what a
// button is called, that the daemon is working). Nothing here knows a step's logic, a default name
// or what a passphrase looks like.
//
// A view is `{ mode, state, end, sending, words, revealed, device, origin }`:
//   mode      `starting` (the start is on its way), `joining` (another walk runs and its state is
//             awaited), `live` (the walk this page drives), `foreign` (a walk another origin
//             drives: read-only), `ended` (this page's walk ended) or `idle` (no walk and none
//             starting: the start was refused)
//   state     the daemon's walk state (`phase`, `working`, `prompt`, `rows`, `can_go_back`,
//             `refusal`), for `live` and `foreign`
//   end       how the walk ended: `{ kind: 'done' | 'cancelled' | 'failed', payload, message }`
//   sending   a call of this page's is on its way: nothing more can be pressed
//   words     the page's refusal of a call (a walk that stopped waiting, a daemon that is away)
//   revealed  whether the secret field shows what is typed
//   device    the fleet's row of the device a done walk wrote: `{ name, url, connected, since }`
//   origin    who started a `foreign` walk, in the daemon's word
//
// The page is five places, each put alone by the wire so a field's focus and what is typed in it
// stay through a state that changes only another place: the heading (`HEAD_ID`), the rows heard
// (`ROWS_ID`), the controls (`CONTROLS_ID`), the status line (`STATUS_ID`) and the refusal
// (`REFUSAL_ID`). The status and refusal places are live regions that stay as elements: what is put
// into them is read, so the outcome (done, cancelled, failed) is said in the status line.

import { attr, esc } from '../../html.js';
import { dot } from '../frame.js';

/** The page's id in the popup, and the ids of its places and controls. */
export const ADD = 'add';
export const WALK_ID = 'popup-devices-walk';
export const HEAD_ID = 'popup-devices-walk-head';
export const ROWS_ID = 'popup-devices-walk-rows';
export const CONTROLS_ID = 'popup-devices-walk-controls';
export const STATUS_ID = 'popup-devices-walk-status';
export const REFUSAL_ID = 'popup-devices-walk-refusal';
export const BODY_ID = 'popup-devices-walk-body';
export const FIELD_ID = 'popup-devices-walk-field';
export const REVEAL_ID = 'popup-devices-walk-reveal';
export const BACK_ID = 'popup-devices-walk-back';
export const STOP_ID = 'popup-devices-walk-stop';
export const AGAIN_ID = 'popup-devices-walk-again';
export const OPEN_ID = 'popup-devices-walk-open';
export const DONE_ID = 'popup-devices-walk-done';

/** The step of the daemon's walk that warns of a link, whose last choice is the safe one. */
const WARNING_STEP = 'linked';

/** Who started a walk that is not this page's, as the daemon names it (its log's word) and as the
 * page says it. A terminal's walk is "the terminal", whichever client of the socket it is. */
const FOREIGN_ORIGINS = { socket: 'the terminal', mcp: 'the MCP server', mqtt: 'the MQTT client' };

/** The page's own words. */
export const WALK_WORDS = {
  entry: '+ Add a device',
  title: 'Add a device',
  starting: 'Starting…',
  joining: 'A device walk is running. Looking for where it stands…',
  joiningBlind: 'Another device walk is running, but its state has not reached this page. '
    + 'It looks again when the connection is back.',
  foreign: (origin) => (origin === 'web'
    ? 'A device walk started in another browser tab is running.'
    : `A device walk started from ${FOREIGN_ORIGINS[origin] ?? origin} is running.`),
  foreignOn: (title) => `It is at: ${title}`,
  // the daemon's WALK_IDLE_BOUND (10 minutes), said in words: the page does not count it
  foreignWait: 'Nothing here can answer it. It ends when whoever started it finishes it, or after '
    + 'ten minutes with no answer.',
  scanning: 'Listening for devices…',
  testing: 'Testing the connection…',
  writing: 'Saving…',
  working: 'Working…',
  nothingHeard: 'No device heard yet.',
  heardTitle: 'Heard so far',
  signal: (rssi) => `${rssi} dBm`,
  configured: (name) => `Configured as ${name}`,
  next: 'Continue',
  back: 'Back',
  stop: 'Stop adding',
  again: 'Start again',
  reveal: 'Show',
  open: 'Open its page',
  added: (name) => `${name} was added.`,
  linked: 'It is connected.',
  notLinked: 'It is not connected yet.',
  noneAdded: 'No device was added.',
  cancelled: 'The walk was cancelled.',
  failed: 'The walk stopped.',
  refusals: {
    signed_out: 'Sign in to add a device.',
    forbidden: 'Changes are turned off on this page.',
    no_answer: 'svolder did not answer.',
    not_waiting: 'The walk was not waiting for that. Its next step shows what it does.',
    no_such_job: 'The walk is no longer there.',
    stopping: 'svolder is stopping.',
    busy: 'Another device walk is running.',
    no_job: 'No walk is running.',
  },
};

/** The words of a refused call of this page: the page's own for the reasons it has words for,
 * else the daemon's (`error`), else that the walk did not take it. */
export function refusalWords(refusal, words = WALK_WORDS) {
  const reason = refusal?.reason;
  if (Object.hasOwn(words.refusals, reason)) return words.refusals[reason];
  return typeof refusal?.error === 'string' && refusal.error ? refusal.error
    : 'The walk did not take that.';
}

/** Whether the walk works on an answer now (nothing can be answered). */
export const busy = (state) => state?.phase === 'working';

/** What the walk is doing, in words, while it works. */
function workingWords(state, words) {
  return words[{
    scanning: 'scanning', testing: 'testing', writing: 'writing',
  }[state.working] ?? 'working'];
}

/** The unit's row of `rows` that a choice's text names: the daemon words each choice with its unit
 * id, and an answer is a number, so the row is found by the unit the words hold. */
function rowOf(choice, rows) {
  return (rows ?? []).find((row) => row.unit && String(choice).includes(row.unit)) ?? null;
}

/** The prompt of the state, with the page's own heading when there is none to show. */
const promptOf = (state) => state?.prompt ?? null;

/** The heading: the prompt's title and its body lines, the last of a text prompt's marked as
 * the default an empty line stands for. */
export function headHtml(view, words = WALK_WORDS) {
  const prompt = promptOf(view.state);
  if (!prompt || view.mode !== 'live') return `<h4 class="fp-heading">${esc(words.title)}</h4>`;
  const warning = prompt.step === WARNING_STEP ? ' fpw-warning' : '';
  const last = prompt.takes === 'text' || prompt.takes === 'secret' ? prompt.body.length - 1 : -1;
  const lines = prompt.body.map((line, i) => (
    `<p class="fpw-line${i === last ? ' fpw-default' : ''}">${esc(line)}</p>`)).join('');
  return `<div class="fpw-head${warning}"><h4 class="fp-heading">${esc(prompt.title)}</h4>`
    + `<div${attr('id', BODY_ID)}>${lines}</div></div>`;
}

/** The rows heard: while the walk scans, one per unit in the order they were heard, each with its
 * product, its unit, its signal and, for a configured unit, which device it is. */
export function rowsHtml(view, words = WALK_WORDS) {
  const { mode, state } = view;
  if (mode !== 'live' || state?.prompt?.step !== 'scan') return '';
  const rows = state.rows ?? [];
  if (rows.length === 0) {
    return state.phase === 'working'
      ? `<p class="fp-note">${esc(words.nothingHeard)}</p>` : '';
  }
  const items = rows.map((row) => '<li class="fpw-row"'
    + `${attr('data-unit', row.unit)}${attr('data-configured', row.configured ?? null)}>`
    + `<span class="fpw-product">${esc(row.product ?? row.unit)}</span> `
    + `<span class="fpw-unit">${esc(row.unit)}</span>`
    + `${Number.isFinite(row.rssi)
      ? ` <span class="fpw-signal">${esc(words.signal(row.rssi))}</span>` : ''}`
    + `${row.configured ? ` <span class="fp-tag">${esc(words.configured(row.configured))}</span>`
      : ''}</li>`).join('');
  return `<h5 class="fpw-subheading">${esc(words.heardTitle)}</h5>`
    + `<ul class="fpw-rows">${items}</ul>`;
}

/** The unit's line of a choice that is not offered: a configured one. */
const notOffered = (choice, state) => Boolean(rowOf(choice, state.rows)?.configured);

/** The controls the prompt takes, drawn for the state: its choices as buttons (the safe answer of
 * the warning, else the first that can be chosen, marked to take the focus), a text field, or a
 * secret field never filled; Back when the walk can step back and *Stop adding* always. */
export function controlsHtml(view, words = WALK_WORDS) {
  const { mode, state, sending } = view;
  if (mode === 'ended' || mode === 'idle') return endedHtml(view, words);
  if (mode !== 'live') return '';
  const prompt = promptOf(state);
  if (!prompt) return '';
  const off = busy(state) || sending;
  const refused = Boolean(state.refusal);
  let main = '';
  if (prompt.takes === 'choice') {
    const focus = prompt.step === WARNING_STEP ? prompt.choices.length - 1
      : prompt.choices.findIndex((choice) => !notOffered(choice, state));
    main = '<div class="fpw-choices" role="group"'
      + `${attr('aria-label', prompt.title)}`
      + `${attr('aria-describedby', prompt.body.length > 0 ? BODY_ID : null)}>`
      + `${prompt.choices.map((choice, i) => {
        const away = notOffered(choice, state);
        return `<button type="button" class="fi-btn fpw-choice"${attr('data-walk-choice', i)}`
          + `${attr('data-walk-focus', i === focus)}`
          + `${attr('aria-disabled', off || away ? 'true' : null)}>${esc(choice)}</button>`;
      }).join('')}</div>`;
  } else {
    const secret = prompt.takes === 'secret';
    const revealed = secret && view.revealed === true;
    const hint = prompt.body.at(-1) ?? '';
    main = '<form class="fi-form fpw-form" data-walk-form>'
      + `<label${attr('for', FIELD_ID)} class="fpw-label">${esc(prompt.title)}</label> `
      + `<input class="fi-input fpw-field"${attr('id', FIELD_ID)}`
      + `${attr('type', secret && !revealed ? 'password' : 'text')} name="walk-answer"`
      + `${attr('autocomplete', secret ? 'new-password' : 'off')}`
      + ' autocapitalize="none" spellcheck="false" enterkeyhint="done"'
      + `${secret ? '' : attr('placeholder', hint)}`
      + `${attr('aria-invalid', String(refused))}`
      + `${attr('aria-describedby', refused ? REFUSAL_ID : null)}`
      + `${attr('readonly', off)}${attr('data-secret', secret)}${attr('data-step', prompt.step)}`
      + `${attr('data-walk-focus', true)}> `
      + `${secret ? `<button type="button" class="fi-btn"${attr('id', REVEAL_ID)}`
        + `${attr('aria-pressed', String(revealed))}${attr('aria-controls', FIELD_ID)}`
        + `>${esc(words.reveal)}</button> ` : ''}`
      + `<button type="submit" class="fi-btn"${attr('aria-disabled', off ? 'true' : null)}>`
      + `${esc(words.next)}</button></form>`;
  }
  const back = state.can_go_back
    ? `<button type="button" class="fi-btn"${attr('id', BACK_ID)}`
      + `${attr('aria-disabled', off ? 'true' : null)}>${esc(words.back)}</button> ` : '';
  const stop = `<button type="button" class="fi-btn"${attr('id', STOP_ID)}`
    + `${attr('aria-disabled', sending ? 'true' : null)}>${esc(words.stop)}</button>`;
  return `<div class="fpw-controls"${attr('aria-busy', String(off))}>${main}`
    + `<p class="fi-set fpw-more">${back}${stop}</p></div>`;
}

/** What stands in the controls' place once the walk ended or never started: *Open its page* for a
 * device that was added and its link is known, and *Start again*. How it ended is in the status
 * line, which is read. */
function endedHtml(view, words) {
  const { end, device, mode } = view;
  const again = `<button type="button" class="fi-btn"${attr('id', AGAIN_ID)}>`
    + `${esc(words.again)}</button>`;
  const open = mode === 'ended' && end?.kind === 'done' && end.payload && device
    ? `<button type="button" class="fi-btn"${attr('id', OPEN_ID)}${attr('data-open', device.url)}`
      + `>${esc(words.open)}</button> ` : '';
  return `<p class="fi-set"${attr('id', DONE_ID)}>${open}${again}</p>`;
}

/** How the walk ended, in words: the device that was added with its link, the daemon's words for
 * a failure, a cancel, or a quiet scan. */
function outcomeHtml(view, words) {
  const { end, device } = view;
  if (end?.kind === 'done' && end.payload) {
    const name = device?.name ?? end.payload.name;
    const link = device ? (device.connected ? words.linked : words.notLinked) : '';
    return `<span class="fpw-added">${device ? dot(device.connected ? 'on' : 'off') : ''} `
      + `${esc(words.added(name))} ${esc(link)}</span>`;
  }
  const said = end?.kind === 'failed' ? (end.message || words.failed)
    : end?.kind === 'cancelled' ? words.cancelled : words.noneAdded;
  return `<span class="fpw-ended${end?.kind === 'failed' ? ' fp-failed' : ''}">${esc(said)}</span>`;
}

/** The status line: its place is a live region (walkHtml) that is never drawn again, so a screen
 * reader hears what is put into it: what the walk is doing, or how it ended. */
export function statusHtml(view, words = WALK_WORDS) {
  const { mode, state } = view;
  let said = '';
  if (mode === 'ended') return `<p class="fp-note fpw-status">${outcomeHtml(view, words)}</p>`;
  if (mode === 'starting') said = words.starting;
  else if (mode === 'joining') said = view.blind ? words.joiningBlind : words.joining;
  else if (mode === 'foreign') {
    said = `${words.foreign(view.origin ?? state?.origin ?? '')} `
      + `${state?.prompt ? words.foreignOn(state.prompt.title) : ''} ${words.foreignWait}`;
  } else if (mode === 'live' && (busy(state) || view.sending)) {
    said = busy(state) ? workingWords(state, words) : words.working;
  }
  return `<p class="fp-note fpw-status">${esc(said.trim())}</p>`;
}

/** The refusal of the last answer, at the control it is about: the daemon's words for it, or the
 * page's own words for a call that was refused; empty when there is none. */
export function refusalHtml(view) {
  const said = view.words ?? view.state?.refusal?.message ?? '';
  return `<p class="fp-note${said ? ' fp-failed' : ''} fpw-refusal">${esc(said)}</p>`;
}

/** The whole page's body: its five places, each by its id. */
export function walkHtml(view, words = WALK_WORDS) {
  return `<section${attr('id', WALK_ID)} class="fpw"${attr('data-mode', view.mode)}>`
    + `<div${attr('id', HEAD_ID)}>${headHtml(view, words)}</div>`
    + `<div${attr('id', ROWS_ID)}>${rowsHtml(view, words)}</div>`
    + `<div${attr('id', CONTROLS_ID)}>${controlsHtml(view, words)}</div>`
    + `<div${attr('id', STATUS_ID)} role="status" aria-atomic="true">`
    + `${statusHtml(view, words)}</div>`
    + `<div${attr('id', REFUSAL_ID)} role="status" aria-atomic="true">${refusalHtml(view)}</div>`
    + '</section>';
}
