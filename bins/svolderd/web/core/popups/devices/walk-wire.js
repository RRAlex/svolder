/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The wire of the Devices popup's *+ Add a device* page: the device walk the daemon runs, driven
// from the page. It starts a walk when the page is entered, draws every state the daemon sends
// (the `walk` events of the stream, `needs=walk`) in place (walk.js), and sends the person's
// answers, Back and Stop by `core.actions.walk`, each naming its job. A step the daemon causes
// (a new prompt, a unit heard, done, failed) changes the page in place and pushes no entry of the
// history: only *Open its page* is a gesture that steps.
//
// Which walk is the page's: the one it started (its answer names the job, which the tab's session
// store remembers through a reload) or, when another walk runs (`busy`), the one the stream says is
// running: if it is the job this tab started before a reload it is taken up where it stands and
// answered as any other; any other walk (another origin, or another tab's) is shown read-only. The
// stream replays the last state of each walk, ended ones too, when it opens: only the walk in hand
// shows how it ended, and nothing is drawn of an old one.
//
// Leaving the page (another entry, Back, Escape, the popup closed) stops the walk this page
// started, by its job, from the moment its start answered; a walk that is only watched is left
// alone.
//
// Two events that answer one write come in either order (the call's answer, the stream's state):
// both are applied whole, and the controls stay shut from the answer until the state that follows
// it, so an answer is never made twice for one prompt. A job the daemon no longer knows answers
// `no_such_job`: the page says so once and offers *Start again*; nothing is retried.
//
// What is typed (a name, a code) is kept through every put of the controls by this wire, as the
// settings' fields are, and goes back only into the field of the same step; a code is also dropped
// when its answer was refused, and after it is sent: it is nowhere in the page but in the field.

import {
  ADD, AGAIN_ID, BACK_ID, CONTROLS_ID, FIELD_ID, HEAD_ID, OPEN_ID, REFUSAL_ID, REVEAL_ID, ROWS_ID,
  STATUS_ID, STOP_ID, controlsHtml, headHtml, refusalHtml, refusalWords, rowsHtml,
  statusHtml, walkHtml,
} from './walk.js';

const TERMINAL = new Set(['done', 'failed', 'cancelled']);
/** The origin of a walk this page starts, in the daemon's word. */
const OWN_ORIGIN = 'web';

/**
 * The walk part of the popup `frame` over `core`. `deviceNamed(name)` gives the popup's row of the
 * configured device with that name (`{ name, url, connected, since }`) or none; `written()` is
 * told when a walk has written a device, so the popup reads its list; `isOpen()` says whether the
 * popup stands. Returns `{ view, enter, leave, refresh, listen, close }`.
 */
export function createWalk({ core, frame, deviceNamed, written, isOpen }) {
  const store = core.model;
  const doc = frame.el.ownerDocument;

  /** The last state of each job the stream told of, by job (`{ kind, payload, message }`). Only
   * the walk in hand shows how it ended: a walk that ended before this page started it or took
   * it up is no business of the page's. */
  const latest = new Map();
  /** The jobs this page started (and one it took up again after a reload, which its tab's session
   * store remembers): the only walks it stops by leaving. Another tab's walk is shown, read-only.
   */
  const own = new Set();
  const win = doc.defaultView;
  const REMEMBERED = 'svolder-walk-job';
  /** The daemon's run the walks held here belong to (its stamp, which the walk events and the
   * start's answer carry), or none before any was told. Job ids restart with each run. */
  let run = null;
  /** The run as the remembered job names it: no run told is the empty word. */
  const tag = () => (run === null ? '' : String(run));
  /** The job this tab started last and its run, kept through a reload (the tab's own session
   * store), as `{ job, run }`, or none. */
  const remembered = () => {
    try {
      const text = String(win.sessionStorage.getItem(REMEMBERED) ?? '');
      const cut = text.lastIndexOf(':');
      const job = Number(text.slice(cut + 1));
      if (text === '' || Number.isNaN(job)) return null;
      return { job, run: text.slice(0, Math.max(cut, 0)) };
    } catch {
      return null;
    }
  };
  const remember = (id) => {
    try {
      if (id === null) win.sessionStorage.removeItem(REMEMBERED);
      else win.sessionStorage.setItem(REMEMBERED, `${tag()}:${id}`);
    } catch {
      // no store: a reload then shows the walk read-only
    }
  };
  /** Whether the walk `id` of the run held now is the one this tab remembers. */
  const isRemembered = (id) => {
    const last = remembered();
    return last !== null && last.job === id && last.run === tag();
  };
  /** Whether the walk `id` is this page's to answer and stop. */
  const ours = (id) => own.has(id) || isRemembered(id);
  let onPage = false;
  /** Counts the entries of the page: an answer of an earlier one is dropped. */
  let visit = 0;
  /** The jobs this page stopped by leaving: one of them that still runs is waited for, never taken
   * over. */
  const abandoned = new Set();
  /** The page's view, as walk.js says it. `job` is the walk the page drives or watches. */
  let mode = 'idle';
  let job = null;
  let end = null;
  let sending = false;
  let words = null;
  let revealed = false;
  /** Counts the states of the job in hand: a call's answer that finds it moved on need not wait
   * for the next. */
  let turns = 0;
  /** The running walk a `joining` page waits on: when it ends, the start is made again. */
  let waitingOn = null;
  /** Whether the last start was refused `busy` with no running walk seen and no stream open: the
   * stream's next opening starts again. */
  let blind = false;
  /** What the controls were last drawn for: the step, so a new one takes the focus. */
  let drawnFor = '';
  let lastRefusal = null;
  let wantFocus = false;
  /** The name of a device a done walk wrote, until the page behind has moved to it (when none
   * was shown: the first success of a fresh install). */
  let adopt = null;

  /** The state of the job in hand, or none. */
  const stateOf = () => {
    const last = job === null ? null : latest.get(job);
    return last?.kind === 'progress' ? last.payload : null;
  };
  const originOf = () => stateOf()?.origin ?? '';

  /** The view the draws take. */
  function view() {
    const state = stateOf();
    const done = end?.kind === 'done' && end.payload ? deviceNamed(end.payload.name) ?? null : null;
    return {
      mode: onPage ? mode : 'starting', state, end, sending, words, revealed, device: done,
      origin: originOf(), blind: blind && mode === 'joining',
    };
  }

  // ---- drawing, each place alone ----

  /** Run `change` (puts into the controls) and give back what is typed when it is the same field
   * of the same step (the field says its step): a code never goes into the name's field nor a name
   * into the code's, and a code is dropped when its answer was refused. */
  function keepTyped(change, { dropSecret = false } = {}) {
    const field = doc.getElementById(FIELD_ID);
    const step = field?.dataset.step;
    const typed = field && frame.el.contains(field) && field.value !== field.defaultValue
      && !(dropSecret && field.dataset.secret !== undefined) ? field.value : null;
    const focused = Boolean(field) && doc.activeElement === field;
    change();
    const now = doc.getElementById(FIELD_ID);
    if (typed === null || !now || now.dataset.step !== step || now.value === typed) return;
    now.value = typed;
    if (focused && doc.activeElement === now) now.setSelectionRange?.(typed.length, typed.length);
  }

  /** The key of the step the controls show: another key is a new prompt. */
  function stepKey() {
    const prompt = stateOf()?.prompt;
    const step = prompt ? `${prompt.step}|${prompt.title}|${prompt.takes}` : end?.kind;
    return `${mode}|${job}|${step}`;
  }

  /** Give the focus to what the step offers: its field, its safe or first choice, the way on of a
   * walk that ended. */
  function giveFocus() {
    if (!wantFocus) return;
    const place = doc.getElementById(CONTROLS_ID);
    const target = place?.querySelector('[data-walk-focus]') ?? place?.querySelector(`#${OPEN_ID}`)
      ?? place?.querySelector(`#${AGAIN_ID}`);
    if (!target || target.getAttribute('aria-disabled') === 'true') return;
    wantFocus = false;
    target.focus({ preventScroll: true });
  }

  /** Put every place of the page. A put of what a place holds changes nothing. */
  function put() {
    if (!onPage || !isOpen()) return;
    const now = view();
    const key = stepKey();
    if (key !== drawnFor) {
      drawnFor = key;
      wantFocus = ['live', 'ended', 'idle'].includes(mode);
    }
    const refusal = now.state?.refusal ?? null;
    const fresh = refusal !== null && JSON.stringify(refusal) !== JSON.stringify(lastRefusal);
    if (fresh && !wantFocus) wantFocus = true;
    lastRefusal = refusal;
    frame.put(HEAD_ID, headHtml(now));
    frame.put(ROWS_ID, rowsHtml(now));
    keepTyped(() => frame.put(CONTROLS_ID, controlsHtml(now)), { dropSecret: fresh });
    frame.put(STATUS_ID, statusHtml(now));
    frame.put(REFUSAL_ID, refusalHtml(now));
    const place = doc.getElementById('popup-devices-walk');
    if (place) place.dataset.mode = mode;
    giveFocus();
  }

  // ---- what the stream says ----

  /** The running walk the stream told of last, as `[job, state]`, or none. */
  function running() {
    let found = null;
    for (const [id, last] of latest) if (!TERMINAL.has(last.kind)) found = [id, last];
    return found;
  }

  /** Take over the walk `id`, which the stream says runs: this page's origin is answered, any
   * other is shown. */
  function follow(id) {
    blind = false;
    job = id;
    end = null;
    sending = false;
    words = null;
    turns += 1;
    const last = latest.get(id);
    if (ours(id)) own.add(id);
    if (last?.kind !== 'progress') mode = 'starting';
    else mode = last.payload.origin === OWN_ORIGIN && ours(id) ? 'live' : 'foreign';
  }

  /** A state of the job in hand, applied: the controls a call shut are open again, a walk that
   * ended shows its end. */
  function applied(state) {
    turns += 1;
    sending = false;
    words = null;
    if (TERMINAL.has(state.kind)) {
      end = state;
      mode = 'ended';
      if (own.has(job) && isRemembered(job)) remember(null);
      if (state.kind === 'done' && state.payload) finished(state.payload);
    } else if (state.kind === 'progress') {
      mode = state.payload.origin === OWN_ORIGIN && ours(job) ? 'live' : 'foreign';
    }
  }

  /** The daemon's run is `now`: when it is not the run the held walks belong to, the daemon has
   * restarted and its job ids began again, so every walk held here is forgotten (the page that
   * drove or watched one is left on its card). */
  function noteRun(now) {
    if (now === undefined || now === run) return;
    const first = run === null;
    run = now;
    if (first) return;
    latest.clear();
    own.clear();
    abandoned.clear();
    waitingOn = null;
    remember(null);
    if (onPage && job !== null) {
      job = null;
      end = null;
      sending = false;
      words = null;
      turns += 1;
      mode = 'idle';
      put();
    }
  }

  /** A walk's state came. */
  function heard(data) {
    const id = data?.job;
    const state = data?.state;
    if (!Number.isInteger(id) || !state || typeof state.kind !== 'string') return;
    noteRun(data.run);
    // a state told again (the stream replays each walk's last when it opens) changes nothing, but
    // the controls a call shut open on any state that follows it, a repeated one too (a refused
    // Back sends none between)
    const again = JSON.stringify(latest.get(id)) === JSON.stringify(state);
    if (again && !(sending && id === job)) return;
    latest.set(id, state);
    const final = TERMINAL.has(state.kind);
    if (!onPage) return;
    if (id === job) {
      applied(state);
      put();
      return;
    }
    // the start is on its way: its answer says whether this walk is the page's
    if (mode === 'starting' && job === null) return;
    if (final) {
      if (id === waitingOn) begin();
      return;
    }
    // only a page that asked (and was told another walk runs) or watches one follows a new one: a
    // page left on its card stays there, and Start again joins
    if (['joining', 'foreign'].includes(mode) && !abandoned.has(id)) {
      follow(id);
      put();
    }
  }

  /** A walk wrote a device: the popup reads its list; a page that showed none moves to it once
   * the fleet lists it. */
  function finished(payload) {
    written();
    adopt = store.shown() === null ? payload.name : null;
    refresh();
  }

  /** The fleet or the list changed: the done card has its device's link, and a page that showed
   * no device moves to the new one. */
  function refresh() {
    if (adopt !== null && store.shown() === null) {
      const row = (store.page().fleet?.devices ?? []).find((one) => one.name === adopt);
      if (row) {
        adopt = null;
        core.actions.showDevice(row.url, { push: false, quiet: true });
      }
    } else if (adopt !== null) {
      adopt = null;
    }
    if (onPage && mode === 'ended' && end?.kind === 'done') put();
  }

  // ---- the walk this page starts ----

  /** Start a walk. Another walk running is joined; any other refusal is worded and offers a new
   * start. */
  async function begin() {
    const mine = visit;
    mode = 'starting';
    job = null;
    end = null;
    sending = false;
    words = null;
    waitingOn = null;
    blind = false;
    put();
    const answer = await core.actions.walk.start();
    if (!onPage || mine !== visit) {
      // the page was left while the start was on its way: the walk it started is stopped
      if (answer.ok) {
        abandoned.add(answer.job);
        core.actions.walk.cancel({ job: answer.job, run: answer.run });
      }
      return;
    }
    if (answer.ok) {
      noteRun(answer.run);
      own.add(answer.job);
      remember(answer.job);
      follow(answer.job);
      // a walk that ended before its start answered shows its end
      const last = latest.get(answer.job);
      if (last && TERMINAL.has(last.kind)) applied(last);
      put();
      return;
    }
    if (answer.reason === 'busy') {
      const [id] = running() ?? [];
      mode = 'joining';
      if (id !== undefined) {
        waitingOn = id;
        // a walk this page stopped by leaving is waited for, and the start made again when it ends
        if (!abandoned.has(id)) follow(id);
      } else {
        // no running walk has been seen and no stream is open to tell of one (closed or lost): say
        // so, and start again when the stream is back; an open stream's replay is on its way
        blind = store.state() !== 'open';
      }
      put();
      return;
    }
    mode = 'idle';
    words = refusalWords(answer);
    put();
  }

  /** A call of this page's on the job in hand was answered. */
  function answered(result, turn) {
    if (!onPage || result.ok) {
      // a state that came before the answer has ended the wait already
      if (turns !== turn) sending = false;
      put();
      return;
    }
    sending = false;
    if (result.reason === 'no_such_job') {
      job = null;
      mode = 'idle';
    }
    words = refusalWords(result);
    put();
  }

  /** Send `call` (one of `core.actions.walk`, naming the job in hand) and shut the controls
   * until the state that follows. */
  function send(call) {
    if (mode !== 'live' || job === null || sending) return;
    const turn = turns;
    const mine = visit;
    sending = true;
    words = null;
    put();
    call(job).then((result) => { if (mine === visit) answered(result, turn); });
  }

  /** Whether the walk can be answered now: the page drives it and it waits. */
  const waiting = () => mode === 'live' && !sending && stateOf()?.phase === 'waiting';

  // ---- the page's controls ----

  frame.el.addEventListener('submit', (event) => {
    const form = event.target.closest?.('form[data-walk-form]');
    if (!form) return;
    // stopped before anything else can return: a submit the browser took as its own would be a
    // GET of the page with the form's fields, a code among them, in the address
    event.preventDefault();
    if (!isOpen() || !onPage || !waiting()) return;
    const field = form.querySelector('input');
    const text = field.value;
    if (field.dataset.secret !== undefined) field.value = '';
    send((id) => core.actions.walk.answer({ text }, { job: id, run }));
  });

  frame.el.addEventListener('click', (event) => {
    const button = event.target.closest?.('button');
    if (!button || !isOpen() || !onPage || !button.closest('#popup-devices-walk')) return;
    if (button.getAttribute('aria-disabled') === 'true') return;
    if (button.dataset.walkChoice !== undefined) {
      if (waiting()) {
        const choice = Number(button.dataset.walkChoice);
        send((id) => core.actions.walk.answer({ choice }, { job: id, run }));
      }
    } else if (button.id === BACK_ID) {
      if (waiting()) send((id) => core.actions.walk.back({ job: id, run }));
    } else if (button.id === STOP_ID) {
      stop();
    } else if (button.id === REVEAL_ID) {
      revealed = !revealed;
      put();
    } else if (button.id === AGAIN_ID) {
      begin();
    } else if (button.id === OPEN_ID) {
      const url = button.dataset.open;
      if (url) core.router.step(`d-${url}`);
    }
  });

  /** Stop the walk the page drives (its Stop button); the daemon answers with the cancelled
   * state. */
  function stop() {
    if (mode !== 'live' || job === null) return;
    const turn = turns;
    const mine = visit;
    const id = job;
    abandoned.add(id);
    sending = true;
    put();
    core.actions.walk.cancel({ job: id, run }).then((result) => {
      if (mine === visit) answered(result, turn);
    });
  }

  // ---- the page and the popup ----

  return {
    view,
    refresh,
    /** The markup of the page's body, before the walk starts. */
    html: () => walkHtml(view()),
    /** The page was entered: start, or join, a walk. */
    enter() {
      if (onPage) return;
      onPage = true;
      visit += 1;
      revealed = false;
      drawnFor = '';
      lastRefusal = null;
      wantFocus = false;
      begin();
    },
    /** The page was left: the walk it drives is stopped, one it watches is left alone. With
     * `quiet` the viewer can no longer write: nothing is asked of the daemon, the walk runs on. */
    leave({ quiet = false } = {}) {
      if (!onPage) return;
      onPage = false;
      visit += 1;
      // a walk this page started is stopped from the moment its start answered, whatever state it
      // has told of; a walk of another tab is left alone
      if (quiet) {
        remember(null);
      } else if (job !== null && !end && own.has(job) && !abandoned.has(job)) {
        const state = latest.get(job);
        if (!state || !TERMINAL.has(state.kind)) {
          abandoned.add(job);
          core.actions.walk.cancel({ job, run });
          remember(null);
        }
      }
      mode = 'idle';
      job = null;
      end = null;
      sending = false;
      words = null;
      revealed = false;
      waitingOn = null;
      blind = false;
    },
    /** The popup opened: hear the walk's states. Returns what takes it back. */
    listen() {
      const offs = [
        store.on('walk', heard),
        store.on('opened', () => {
          // a stream found again says where each walk stands: a page that waited on one looks again
          if (onPage && mode === 'joining' && job === null) {
            const [id] = running() ?? [];
            if (id !== undefined && !abandoned.has(id)) {
              follow(id);
              put();
            } else if (blind) {
              begin();
            }
          }
        }),
      ];
      return () => { for (const off of offs) off(); };
    },
    /** The popup closed: nothing of any walk is kept. */
    close() {
      latest.clear();
      own.clear();
      abandoned.clear();
      adopt = null;
      run = null;
      blind = false;
    },
    ADD,
  };
}
