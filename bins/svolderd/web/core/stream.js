/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The page's one event stream (`api/v1/events`) and the store it feeds: the model, the shown
// device's parts and readings, the followed devices', the fleet and the host's player. A page that
// is not in front rests (the stream closed, the daemon then stops reading for it) and wakes when it
// is shown; until the new stream's first reading the page waits, dimmed and inert as a lost link
// is, without its notice, because a step from the dial is absolute and would ask for the old
// level plus one. A woken page waits no longer than WAIT_MS, and a screen with nothing to read is
// ready at its model.
//
// Its error is never left to the browser's own retry: the stream closes, asks the session (a
// sign-in that ended loads the sign-in page; a signed-in page that is now a guest's loads again)
// and opens afresh REOPEN_MS later on whatever device is shown by then. A device switched while the
// page rests, or while it is lost, is opened on when the stream next opens; one switched on a live
// stream opens a new stream at once, and nothing the old stream still carried reaches the page. A
// page that is waiting keeps waiting through such a switch, for the new device's first reading.
//
// The state ('open', 'lost', 'resting', 'waiting') lives in the store; the body's `lost` and
// `waking` classes and the document's `svolder-lost` event follow it.

import { pageShown } from './visible.js';

/** How long after an error the stream opens again, in milliseconds. */
export const REOPEN_MS = 2000;
/** The longest a woken page waits for its first reading, in milliseconds. */
export const WAIT_MS = 5000;

/** The address of the stream: the device shown, what the page needs (`meters`, `host`) and the
 * devices it follows. The names are the model's own URL segments, already encoded. */
export function streamUrl({ device, needs = [], follow = [] } = {}) {
  const query = [];
  if (device) query.push(`device=${device}`);
  if (needs.length > 0) query.push(`needs=${needs.join(',')}`);
  if (follow.length > 0) query.push(`follow=${follow.join(',')}`);
  return `api/v1/events${query.length > 0 ? `?${query.join('&')}` : ''}`;
}

/**
 * The stream over `store`. `needs` is what the page asks the daemon to send (a list, or a
 * function giving it at each open); `watchVisible` is visible.js's; `signedOut` loads the sign-in
 * page and `reload` loads this one again; `show(url)` shows another device (a fleet that no longer
 * lists the one shown moves the page to the first one it lists, or to the empty page when it
 * lists none); `doc` is the document (its body and its events).
 * Returns `{ open, close, follow, followed }`: `open` opens the stream on the device shown now,
 * `follow(urls)` sets the other devices it carries.
 */
export function createStream({
  store, needs = [], EventSource, fetch, watchVisible, signedOut, reload, show,
  doc = globalThis.document,
}) {
  let source = null;
  /** The device the open stream was opened for (`undefined` when none is open). */
  let openedFor;
  let reopen = 0;
  let waitTimer = 0;
  let following = [];
  let lost = false;
  let waiting = false;
  /** Whether the stream was closed for good: nothing it has pending may reach the page. */
  let closed = false;
  /** Whether the page rests: hidden past the grace (or loaded hidden), with no stream open. */
  let resting = doc ? doc.visibilityState !== 'visible' : !pageShown();

  /** The state the store, the body and the document say now. */
  function settle() {
    const dim = lost || waiting;
    doc?.body?.classList.toggle('lost', dim);
    doc?.body?.classList.toggle('waking', waiting && !lost);
    store.setState(lost ? 'lost' : resting ? 'resting' : waiting ? 'waiting' : 'open');
  }

  function setWaiting(on) {
    waiting = on;
    clearTimeout(waitTimer);
    waitTimer = on ? setTimeout(() => setWaiting(false), WAIT_MS) : 0;
    settle();
  }

  function setLost(now) {
    if (lost === now) return;
    lost = now;
    settle();
    doc?.dispatchEvent?.(new CustomEvent('svolder-lost', { detail: now }));
  }

  /** Whether the device has something a reading would set: the dial, a slider or the meters (the
   * meters only when the page asked for them). */
  function readable(held) {
    if (!held) return false;
    const wanted = typeof needs === 'function' ? needs() : needs;
    return Boolean(held.volume) || (held.controls ?? []).some((control) => control.kind === 'slider')
      || (Boolean(held.meters) && wanted.includes('meters'));
  }

  const isFollowed = (url) => following.includes(url);
  const known = (url) => url === store.shown() || isFollowed(url);

  /** Close the stream and what waits on it; a resting page keeps nothing open. */
  function shut() {
    source?.close();
    source = null;
    openedFor = undefined;
    clearTimeout(reopen);
    reopen = 0;
    clearTimeout(waitTimer);
    waitTimer = 0;
    waiting = false;
  }

  /** Open the stream on the device shown now. A page that was waiting (`keepWait`) waits on, for
   * the new stream's first reading; otherwise the wait ends. */
  function open(keepWait = false) {
    const wasWaiting = keepWait && waiting;
    shut();
    if (resting) {
      settle();
      return;
    }
    const shownNow = store.shown();
    const wanted = typeof needs === 'function' ? needs() : needs;
    const mine = new EventSource(streamUrl({ device: shownNow, needs: wanted, follow: following }));
    source = mine;
    if (wasWaiting) setWaiting(true);
    else settle();
    openedFor = shownNow;
    const wasGuest = Boolean(store.page().viewer?.guest);
    const live = (handler) => (event) => {
      if (source !== mine) return;
      let data;
      try {
        data = event.data === undefined ? undefined : JSON.parse(event.data);
      } catch {
        return;
      }
      handler(data);
    };
    const on = (name, handler) => mine.addEventListener(name, live(handler));
    // The first reading of the shown device is what a woken page waited for.
    const heard = (data) => {
      if (waiting && (data?.device === undefined || data.device === store.shown())) {
        setWaiting(false);
      }
    };

    // A stream that opens is a first sight of everything: a theme forgets what it last read.
    on('open', () => {
      setLost(false);
      store.emit('opened', null);
    });
    mine.addEventListener('error', async () => {
      if (source !== mine) return;
      setLost(true);
      mine.close();
      // No stream is open until the reopen: a device switched meanwhile is opened on then.
      source = null;
      openedFor = undefined;
      reopen = setTimeout(() => open(), REOPEN_MS);
      try {
        const answer = await fetch('api/v1/session', { credentials: 'same-origin' });
        const session = await answer.json();
        if (closed) return;
        if (!session.signed_in && !session.guest) signedOut();
        // A rotated token leaves a guest-readable server: a page that was signed in is now a
        // guest's, and is loaded again as one.
        else if (!session.signed_in && !wasGuest) reload();
      } catch {
        // The daemon is away: the stream is tried again in two seconds.
      }
    });
    on('model', (data) => {
      store.replace(data);
      if (waiting && !readable(store.device())) setWaiting(false);
    });
    on('device', (data) => {
      if (isFollowed(data.device)) store.setDevice(data.device, data.data);
    });
    on('part', (data) => {
      if (known(data.device)) store.applyPart(data.device, data.name, data.data);
    });
    for (const name of ['volume', 'level', 'meters', 'strip']) {
      on(name, (data) => {
        if (!known(data.device ?? store.shown())) return;
        store.emit(name, data);
        heard(data);
      });
    }
    on('fleet', (data) => {
      store.emit('fleet', data);
      const listed = new Set((data.devices ?? []).map((entry) => entry.url));
      for (const url of following.filter((one) => !listed.has(one))) {
        following = following.filter((one) => one !== url);
        store.forget(url);
      }
      const shown = store.shown();
      if (shown && !listed.has(shown)) {
        const first = data.devices?.[0]?.url;
        if (first) show?.(first);
        else store.replace({ ...store.page(), shown: null, device: null, card: null, strip: null });
      }
    });
    on('host', (data) => store.emit('host', data));
  }

  // The page rests once it has been hidden for the grace, and wakes when it is shown: the stream
  // closes without a word (the link is not lost), and opens anew on the device then shown, the
  // screen waiting until the stream's first reading.
  const watch = watchVisible({
    onShow() {
      if (!resting) return;
      resting = false;
      open(true);
      if (source) setWaiting(true);
    },
    onHide() {
      resting = true;
      open();
    },
  });

  // A switch of the device shown, on a live stream, opens the stream on the new one. The
  // stream's own `model` names the device it was opened for and changes nothing.
  const offModel = store.on('model', () => {
    if (source && store.shown() !== openedFor) open(true);
  });
  settle();

  return {
    open,

    /** Close the stream for good: nothing reopens it. */
    close() {
      closed = true;
      shut();
      offModel();
      watch?.stop?.();
    },

    /** Carry these devices (URL segments) beside the one shown; the stream opens again on the
     * new address when the list changed. */
    follow(urls) {
      const next = [...new Set(urls)];
      const gone = following.filter((url) => !next.includes(url));
      const changed = next.length !== following.length || next.some((url, i) => url !== following[i]);
      following = next;
      for (const url of gone) store.forget(url);
      if (changed && source) open(true);
    },

    followed: () => [...following],
  };
}
