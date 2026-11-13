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
// More than one owner can ask the stream for feeds: the theme mounted, and each popup that shows
// devices live. Each owner `want`s its `needs` (the words of STREAM_NEEDS) and the devices it
// `follow`s, and the stream's address is the union of every owner's wants; it opens afresh only
// when the union changes, under the rules above, and an owner that leaves (`release`) takes only
// its own wants with it.
//
// The state ('open', 'lost', 'resting', 'waiting') is the page's, not a device's, and lives in the
// store; the body's `lost` and `waking` classes and the document's `svolder-lost` event follow it.

import { pageShown } from './visible.js';

/** The card the daemon gives a page whose configuration names no device (web/view.rs `no_devices`):
 * the page shows it as it does at a load, when the last device goes while it is open. */
const NO_DEVICES = Object.freeze({
  kind: 'dim',
  title: 'No devices',
  detail: 'No devices yet. `svolder setup` adds one.',
  spans: [{ code: false, text: 'No devices yet. ' }, { code: true, text: 'svolder setup' },
    { code: false, text: ' adds one.' }],
  faults: [],
  retry_in: null,
  try_now: false,
});

/** How long after an error the stream opens again, in milliseconds. */
export const REOPEN_MS = 2000;
/** The longest a woken page waits for its first reading, in milliseconds. */
export const WAIT_MS = 5000;

/** What the daemon can send beside a device's own parts: the words an owner's `needs` may name
 * (`strip` is the shown device's composite bar, which one theme draws; `walk` is the device walk's
 * states, which only a viewer who may write is sent). */
export const STREAM_NEEDS = ['meters', 'host', 'strip', 'walk'];

/** The address of the stream: the device shown, what the page needs (`meters`, `host`, `strip`) and
 * the devices it follows. The names are the model's own URL segments, already encoded. */
export function streamUrl({ device, needs = [], follow = [] } = {}) {
  const query = [];
  if (device) query.push(`device=${device}`);
  if (needs.length > 0) query.push(`needs=${needs.join(',')}`);
  if (follow.length > 0) query.push(`follow=${follow.join(',')}`);
  return `api/v1/events${query.length > 0 ? `?${query.join('&')}` : ''}`;
}

/**
 * The stream over `store`. What it asks the daemon to send and the devices it carries come from the
 * owners' `want`s; `watchVisible` is visible.js's; `signedOut` loads the sign-in
 * page and `reload` loads this one again; `show(url)` shows another device (a fleet that no longer
 * lists the one shown moves the page to the first one it lists, or to the empty page when it
 * lists none); `doc` is the document (its body and its events).
 * Returns `{ open, close, want, follow, followed, needed }`: `open` opens the stream on the device
 * shown now; `want(owner, { needs, follow })` sets what `owner` asks for and returns its `release`;
 * `follow(urls)` sets the devices the theme carries; `batch()` batches changes of wants.
 */
export function createStream({
  store, EventSource, fetch, watchVisible, signedOut, reload, show,
  doc = globalThis.document,
}) {
  let source = null;
  /** The device the open stream was opened for (`undefined` when none is open). */
  let openedFor;
  let reopen = 0;
  let waitTimer = 0;
  /** What each owner asks for, by owner: `{ needs, follow }`. An owner's want is kept as it asked;
   * the devices the fleet does not list are left off the address, not off the want. */
  const wants = new Map();
  /** The devices the last fleet event listed (`null` before the first: nothing is left off). */
  let listed = null;
  /** The renames of the shown device that are on their way: old address to new. */
  const renamed = new Map();
  /** The last shown device the fleet dropped and the device the page moved to instead: a rename
   * that is told after the drop (the fleet event came before the write's answer) still moves the
   * page to the renamed device. */
  let lastDrop = null;
  function moveRenamed() {
    if (!lastDrop) return;
    const to = renamed.get(lastDrop.from);
    if (![lastDrop.to, lastDrop.from].includes(store.shown())) {
      // an old drop the page has left: forget it, and keep the renames that are on their way
      lastDrop = null;
    } else if (to && listed?.has(to)) {
      renamed.delete(lastDrop.from);
      lastDrop = null;
      show?.(to);
    }
  }
  /** The feeds the open stream was opened with, as `wantsKey` says them. */
  let openedKey = '';
  /** How many batches are on: while one is, a change of wants opens nothing. */
  let batches = 0;
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
    const wanted = union('needs');
    return Boolean(held.volume) || (held.controls ?? []).some((control) => control.kind === 'slider')
      || (Boolean(held.meters) && wanted.includes('meters'));
  }

  /** Every owner's `key` (`needs` or `follow`), each word once, the first owner's first; a
   * followed device the fleet does not list is left out. */
  function union(key) {
    const all = [...new Set([...wants.values()].flatMap((one) => one[key]))];
    return key === 'follow' && listed ? all.filter((url) => listed.has(url)) : all;
  }

  /** The union as a set, in a fixed order: the same feeds in another order are the same key. */
  const wantsKey = () => `${[...union('needs')].sort()}|${[...union('follow')].sort()}`;

  /** Open the stream again when what it carries is no longer what the owners want. */
  function reopenIfChanged() {
    if (source && batches === 0 && wantsKey() !== openedKey) open(true);
  }

  const isFollowed = (url) => union('follow').includes(url);
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
    const mine = new EventSource(streamUrl({
      device: shownNow, needs: union('needs'), follow: union('follow'),
    }));
    source = mine;
    openedKey = wantsKey();
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
      reopen = setTimeout(() => open(true), REOPEN_MS);
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
        if (name === 'strip' && !union('needs').includes('strip')) return;
        if (!known(data.device ?? store.shown())) return;
        store.emit(name, data);
        heard(data);
      });
    }
    on('fleet', (data) => {
      store.emit('fleet', data);
      const before = union('follow');
      listed = new Set((data.devices ?? []).map((entry) => entry.url));
      for (const url of before) if (!union('follow').includes(url)) store.forget(url);
      const shown = store.shown();
      const dropped = Boolean(shown) && !listed.has(shown);
      // The open stream carries the devices that left in its address, and lacks one that came
      // back: it is opened again with the fleet's own, once; a dropped shown device opens it anew
      // when the page moves off it.
      if (!dropped) reopenIfChanged();
      if (dropped) {
        // a device renamed from a page that stays on show moves to its new address
        const moved = renamed.get(shown);
        renamed.clear();
        const first = listed.has(moved) ? moved : data.devices?.[0]?.url;
        lastDrop = first && first !== moved ? { from: shown, to: first } : null;
        if (first) show?.(first);
        else {
          store.replace({
            ...store.page(), shown: null, device: null, card: NO_DEVICES, strip: null,
          });
        }
      } else if (!shown && data.devices?.length > 0) {
        // the page showed the no-device card and a device has come: it takes up the first one
        show?.(data.devices[0].url);
      } else {
        moveRenamed();
      }
    });
    on('host', (data) => store.emit('host', data));
    on('walk', (data) => store.emit('walk', data));
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

  /** Change the wants with `change`: the devices nobody carries any more are forgotten, and a
   * live stream opens again when its address would differ. */
  function apply(change) {
    const carried = union('follow');
    change();
    const now = union('follow');
    for (const url of carried) if (!now.includes(url)) store.forget(url);
    reopenIfChanged();
  }

  return {
    open,

    /** Close the stream for good: nothing reopens it. */
    close() {
      closed = true;
      shut();
      offModel();
      watch?.stop?.();
    },

    /**
     * Set what `owner` asks for: the `needs` (words of STREAM_NEEDS, others are left out) and the
     * devices (URL segments) to carry beside the one shown. It replaces the owner's earlier want.
     * The stream opens again on the new address when the union changed, and a device nobody
     * follows any more is forgotten. Returns the owner's `release`, which takes this want away
     * (once; a want made since is not touched).
     */
    want(owner, { needs = [], follow = [] } = {}) {
      const mine = {
        needs: [...new Set(needs.filter((word) => STREAM_NEEDS.includes(word)))],
        follow: [...new Set(follow)],
      };
      apply(() => wants.set(owner, mine));
      return () => {
        if (wants.get(owner) === mine) apply(() => wants.delete(owner));
      };
    },

    /** Carry these devices (URL segments) beside the one shown, as the theme's own want; its
     * needs stay as they were. */
    follow(urls) {
      const kept = wants.get('theme');
      apply(() => wants.set('theme', { needs: kept?.needs ?? [], follow: [...new Set(urls)] }));
    },

    followed: () => union('follow'),

    /** The shown device `from` is being renamed to the address `to`: if the fleet then lists `to`
     * and no longer `from`, the page moves to `to`, not to the fleet's first device. */
    renamed(from, to) {
      renamed.set(from, to);
      moveRenamed();
    },

    /** Batch every change of wants: the stream opens at most once, when the returned function
     * ends the batch, and only if what it carries then differs from what it was opened with. */
    batch() {
      batches++;
      let ended = false;
      return () => {
        if (ended) return;
        ended = true;
        batches--;
        reopenIfChanged();
      };
    },

    /** Every feed some owner asks for now. */
    needed: () => union('needs'),
  };
}
