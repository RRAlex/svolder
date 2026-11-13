/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The router: it owns what an entry of the browser's history says about the page, `{ device,
// popup, page }`: the device shown, the popup open over it (or none) and the page of that popup
// (or its list). A device shown pushes an entry, an opened popup pushes one and so does the first
// page entered from its list, so Back steps page → the popup's list → closed → the device before.
//
// A popup is a modal of the core's stack (modal.js) that the router puts there and takes away, so
// what the history says and what stands on the page are one thing, whoever closes the popup:
//
//   register(name, { el, labelledBy, focus, returnTo, onKey, show(page), hide() })   a popup. `el`
//                    is its element and the next four are the stack's options for it (`onKey` is
//                    where its keys come from; `focus` may be a function, which the stack asks
//                    once `show` has drawn the popup and it is in the page). `show(page)`
//                    draws it at `page` (`null`: its list)
//                    and may be called while it is open; on opening it runs before the stack
//                    puts `el` in the page, and an inline ask or a list it opens is the
//                    popup's all the same (`el` is claimed for the stack here). `hide()`, if
//                    given, runs once it has left the page
//                    A popup whose code is loaded when it is first opened gives, in place of
//                    `show`, `hide` and `onKey`: `load()`, which resolves to the part that has
//                    them (`{ show(page), hide(), onKey(event) }`), `loading(page)`, which draws
//                    the popup while the part is on its way, and `failed(page)`, which draws it
//                    when the part did not come. The popup is on the stack at once, so Escape
//                    and Back close it in either state; `load` runs once however often the
//                    popup is opened meanwhile, the part is shown only if the popup still
//                    stands when it arrives, and after a load that failed the next open loads
//                    again. A part that came is kept
//                    `allowed()`, if given, says whether this viewer may have the popup now.
//                    While it says no the popup opens by no path: `open` pushes nothing, draws
//                    nothing, loads nothing and calls `refused()` (which says why), and an entry
//                    of the history that names it is made a plain one, in silence
//                    `stays: true` is a popup that stands through a change of the shown device
//   A `show(page)` that throws, a popup's or a part's, throws nothing on: the part is told to
//   `hide()`, `failed(page)` draws if the popup has one, and the popup stands and closes as any
//   other; the next open or step calls `show` again
//   open(name, page) open the popup (at its list, or straight at `page`)
//   step(page)       enter `page` from the list (Back returns to the list); from a page, change
//                    the page in place; `null` goes back to the list, as Back does
//   close()          close the popup: one step of the history back to the entry it was opened
//                    from. Escape and the stack's `closeAll()` come here by themselves; a popup's
//                    close button calls it
//   state()          `{ device, popup, page }` as the page stands
//   pushState(state, title, address)   the history's own, for a device shown (`{ device }`): the
//                    new entry has no popup, and a popup that was open is closed. Under a popup
//                    that stays it is `shown(device, address)`
//   shown(device, address)   another device is on show with no entry of its own (a device the
//                    fleet dropped, a switch made from a popup that stays): nothing is pushed,
//                    and the entry the page is on names that device and has its address from
//                    now on, with its popup part as it was
//
// A popup registered with `stays: true` stands through a change of the shown device: the core
// leaves it on the stack (`closeAll({ device: true })`), and the switch is no entry of the
// history. The entries of that popup's run and the entry it was opened from were written for the
// device before; each takes the device now shown, and its address, as a step of the history
// reaches it. So Back from such a popup still steps page → list → closed and never shows the
// device before, and Escape and the close button land on the device now shown. What was carried
// is kept in the tab's session store, so the same holds after the page was loaded again.
//
// What a step of the history does:
// - only the popup part changed: the popup is shown at the entry's page, or closed. The device is
//   not read again and never changes;
// - the entry names another device than the one before it: that device is shown, then the entry's
//   popup;
// - the entry names no device (it is not one of the router's): the shown device stays;
// - the entry names a popup that was closed with `close()`, one this load of the page did not
//   open (the page was loaded again since) or one nothing registered: it is made a plain entry
//   and nothing opens. Forward after Escape therefore opens nothing, while Forward after Back
//   shows the popup again.
// A popup's page is the popup's own word (a string): it may name a device that is not the one
// shown.
//
// A popup's entry is pushed from a tap or a key of the person's: Chromium's Back skips an entry
// pushed without one, so a popup opened (or stepped from its list to a page) by a timer or a
// stream event could not be left by Back. Where the browser says whether there was one
// (`navigator.userActivation`), a push without is said in the console; it is made all the same.

import { claim } from './modal.js';

/** The session store's key for the devices carried under popups that stayed, and how many runs
 * are kept. */
const CARRIED = 'svolder-carried';
const CARRIED_MAX = 16;

/**
 * The router over `window`'s history, with its popups on the stack `modal`; `show(url)` shows a
 * device without pushing (it resolves once the device is on show or could not be shown) and
 * `device` is the one shown as the page opens. `changed()`, if given, is called each time the
 * entry the page is on has changed (the core makes the tab's title again).
 */
export function createRouter({
  window, modal, show, device = null, changed = () => {},
}) {
  const { history } = window;
  /** The popups by name: `{ popup, standing, part, loading }`: what was registered, its place on
   * the stack while it is open, and for a popup with a loader the part it loaded (`null` before)
   * and whether the load is under way. */
  const clients = new Map();
  /** The runs of popup entries that were closed: Forward does not open them again. */
  const closed = new Set();
  /** Starts the name of each run of popup entries this load of the page opens; no other load's
   * runs start with it. */
  const born = `${Date.now().toString(36)}${Math.random().toString(36).slice(2, 8)}.`;
  let runs = 0;
  /** How many steps back of the router's own are on their way. */
  let leaving = 0;
  /** What was asked for meanwhile, done once they have landed. */
  const waiting = [];
  /** Counts what changes the entry, so a device still being read does not act on an older one. */
  let moves = 0;

  /** An entry with no popup. `depth` is how many entries the popup has above the one it was
   * opened from, `run` names those entries together. */
  const plain = (url) => ({ device: url, popup: null, page: null, depth: 0, run: null });

  /** The entry the page is on. */
  let current = plain(device);
  /** The place in the history of the entry the page is on, as the router counts them: every
   * entry it writes carries its place (`at`), so a step can tell the entry a popup was opened
   * from. A page loaded again counts on from the entry it was loaded on. */
  let index = Number.isInteger(history.state?.at) ? history.state.at : 0;
  /** The device shown under a popup that stayed through a switch, by the popup's run: `{ device,
   * address, base }`. `base` is the place of the entry the popup was opened from, until that
   * entry has taken the device. */
  const carried = new Map();
  /** Where the carried devices are kept through a reload of the page: the tab's own session
   * store, which lives as long as the tab's history does. A reload makes a new router over the
   * same entries, and those written for the device before must still take the device shown. */
  let session = null;
  try {
    session = window.sessionStorage ?? null;
    for (const [run, one] of JSON.parse(session?.getItem(CARRIED) ?? '[]')) carried.set(run, one);
  } catch {
    // no store, or not ours to read: a reload then forgets what was carried
  }

  /** Keep the carried devices for a reload: the newest CARRIED_MAX runs. */
  function keep() {
    while (carried.size > CARRIED_MAX) carried.delete(carried.keys().next().value);
    try {
      session?.setItem(CARRIED, JSON.stringify([...carried]));
    } catch {
      // the store is full or closed
    }
  }

  // A page loaded on an entry with no place of the router's starts a new chain of loads (a
  // bookmark, a typed address, another svolder of the same origin): its places count from 0
  // again, so a base of the chain before would name an entry of this one. Those runs keep their
  // device for their own entries (a run's name is this tab's alone) and lose their base.
  if (!Number.isInteger(history.state?.at) && carried.size > 0) {
    for (const one of carried.values()) one.base = null;
    keep();
  }

  /** Write `entry` into the history with its place: pushed over the entry the page is on, or in
   * its stead. A push cuts off what was ahead, so a run whose opening entry is gone is dropped. */
  function put(entry, address, pushed) {
    if (pushed) {
      index += 1;
      let dropped = false;
      for (const [run, one] of carried) {
        if (one.base !== null && one.base >= index) dropped = carried.delete(run) || dropped;
      }
      if (dropped) keep();
      history.pushState({ ...entry, at: index }, '', address);
    } else {
      history.replaceState({ ...entry, at: index }, '', address);
    }
  }

  // The entry the page opened on knows its device, so a step back to it shows that device whatever
  // its address says.
  put(current, window.location.href, false);

  /** The state of the entry a step landed on, with the device now shown when a popup that
   * stayed through a switch carried it past this entry: an entry of that popup's run, or the
   * entry it was opened from. Such an entry is written again with the device and its address. */
  function carry(state) {
    if (typeof state?.device !== 'string') return state;
    let to = null;
    if (typeof state.run === 'string') {
      to = carried.get(state.run) ?? null;
    } else {
      for (const one of carried.values()) {
        if (one.base !== null && one.base === state.at) {
          to = one;
          one.base = null;
          keep();
        }
      }
    }
    if (to === null || to.device === state.device) return state;
    const fixed = { ...state, device: to.device };
    history.replaceState(fixed, '', to.address);
    return fixed;
  }

  /** Another device is on show and no entry is pushed for it: the entry the page is on names
   * it, popup part as it was, and a popup's run carries it to its other entries. */
  function move(url, address = window.location.href) {
    if (url === current.device) return;
    const entry = { ...current, device: url };
    if (entry.run !== null) {
      // the newest switch is the last of the map
      carried.delete(entry.run);
      carried.set(entry.run, { device: url, address, base: index - entry.depth });
      keep();
    }
    put(entry, address, false);
    settle(entry);
  }

  /** The entry a history state stands for: without a device it keeps the one shown, and a popup
   * that cannot be shown is none (nothing registered it, it was closed, or its run is not one
   * this load of the page opened). */
  function read(state) {
    const url = typeof state?.device === 'string' ? state.device : current.device;
    const run = state?.run;
    const mine = typeof run === 'string' && run.startsWith(born) && !closed.has(run);
    if (typeof state?.popup !== 'string' || !clients.has(state.popup) || !mine
      || !may(state.popup)) return plain(url);
    return {
      device: url,
      popup: state.popup,
      page: typeof state.page === 'string' ? state.page : null,
      depth: Number.isInteger(state.depth) && state.depth > 0 ? state.depth : 1,
      run,
    };
  }

  /** Whether this viewer may have the popup `name` now: yes, unless it was registered with an
   * `allowed()` that says no. */
  const may = (name) => clients.get(name).popup.allowed?.() !== false;

  /** The popup `name` has left the stack, whoever closed it: when the entry still names it (Escape,
   * the stack emptied), the history steps back past it. */
  function left(name) {
    const client = clients.get(name);
    client.standing = null;
    (client.popup.load ? client.part : client.popup)?.hide?.();
    if (current.popup === name) close();
  }

  /** Show `part` (a popup, or the part its loader gave) at `page`. One that throws is told to
   * hide, so it gives back what it took before it threw, and the popup draws its failed state
   * if it has one. Nothing is thrown on: the popup goes on the stack and into the history as any
   * other, so Escape, Back and its close button still close it. */
  function present(name, part, page) {
    try {
      part.show(page);
    } catch (error) {
      console.warn(`svolder: the popup ${name} threw as it was shown`, error);
      try {
        part.hide?.();
      } catch {
        // it could not even hide
      }
      clients.get(name).popup.failed?.(page);
    }
  }

  /** Draw the popup `name` at `page`: by its own `show`, or by the part its loader gives. Until
   * that part is there the popup draws its loading state, and the part is shown when it arrives
   * if the popup still stands then, at the page of then. A part that arrived is kept for good. */
  function draw(name, page) {
    const client = clients.get(name);
    const { popup } = client;
    if (!popup.load || client.part) {
      present(name, client.part ?? popup, page);
      return;
    }
    popup.loading?.(page);
    if (client.loading) return;
    client.loading = true;
    (async () => popup.load())().then((part) => {
      if (typeof part?.show !== 'function') throw new Error('its part has no show(page)');
      client.loading = false;
      client.part = part;
      if (client.standing && current.popup === name) present(name, part, current.page);
    }).catch((error) => {
      // the next open loads again
      client.loading = false;
      console.warn(`svolder: the popup ${name} could not be loaded`, error);
      if (client.standing && current.popup === name) popup.failed?.(current.page);
    });
  }

  /** `to` is the entry the page is on: the popup on the stack is brought in step with it. */
  function settle(to) {
    try {
      stand(to);
    } finally {
      changed();
    }
  }

  function stand(to) {
    const was = current;
    current = to;
    if (was.popup && was.popup !== to.popup) clients.get(was.popup)?.standing?.close();
    if (!to.popup || (to.popup === was.popup && to.page === was.page)) return;
    const client = clients.get(to.popup);
    const { popup } = client;
    draw(to.popup, to.page);
    const own = popup.onKey ? (event) => popup.onKey(event) : null;
    client.standing ??= modal.open({
      el: popup.el,
      labelledBy: popup.labelledBy,
      focus: popup.focus,
      returnTo: popup.returnTo,
      onKey: popup.load ? (event) => client.part?.onKey?.(event) === true : own,
      onClose: () => left(to.popup),
      stays: popup.stays === true,
    });
  }

  function push(entry, address = window.location.href) {
    put(entry, address, true);
    settle(entry);
  }

  /** Say it when a popup's entry is about to be pushed with no tap or key of the person's
   * behind it; `how` is what was done to the popup. A browser that does not tell says nothing. */
  function gesture(name, how) {
    if (window.navigator?.userActivation?.isActive !== false) return;
    console.warn(`svolder: the popup ${name} was ${how} with no tap or key of the person's: `
      + 'Back may skip its entry');
  }

  function replace(entry) {
    put(entry, window.location.href, false);
    settle(entry);
  }

  /** Do `run` now, or once the router's own steps back have landed: a push made while one is on
   * its way would be stepped back over. */
  function when(run) {
    if (leaving > 0) {
      waiting.push(run);
      return;
    }
    moves += 1;
    run();
  }

  /** A step of the history has landed on the entry whose state is `state`. */
  async function arrive(landed) {
    const own = leaving > 0;
    if (own) leaving -= 1;
    const mine = ++moves;
    const was = current;
    if (Number.isInteger(landed?.at)) index = landed.at;
    const state = carry(landed);
    const next = read(state);
    if (typeof state?.popup === 'string' && next.popup === null) {
      put(next, window.location.href, false);
    }
    // Whether the popup part is what this step changed: the popup on show changes, or the entry is
    // one of a popup's (a closed one's too).
    const moved = next.popup !== was.popup || next.page !== was.page
      || typeof state?.popup === 'string';
    // A step of the router's own lands on the entry the popup was opened from: only the popup
    // changed. Any other step shows the entry's device, unless only the popup part changed.
    if (!own && typeof state?.device === 'string' && (next.device !== was.device || !moved)) {
      settle(plain(next.device));
      try {
        await show(next.device);
      } catch {
        // the device could not be read: the page says so itself
      }
      if (mine !== moves) return;
    }
    // the right to the entry's popup may have gone while the device was read
    if (next.popup !== null && !may(next.popup)) {
      const bare = plain(next.device);
      put(bare, window.location.href, false);
      settle(bare);
    } else {
      settle(next);
    }
    if (leaving === 0) {
      for (const run of waiting.splice(0)) when(run);
    }
  }

  window.addEventListener('popstate', (event) => {
    arrive(event.state);
  });

  /** Go to `page` of the open popup. */
  function step(page) {
    when(() => {
      if (!current.popup || page === current.page) return;
      if (page !== null) {
        if (current.page === null) {
          gesture(current.popup, 'stepped to a page');
          push({ ...current, page, depth: current.depth + 1 });
        } else {
          replace({ ...current, page });
        }
      } else if (current.depth > 1) {
        // the list is the entry under this page
        leaving += 1;
        settle({ ...current, page: null, depth: current.depth - 1 });
        history.back();
      } else {
        replace({ ...current, page: null });
      }
    });
  }

  function close() {
    if (!current.popup) return;
    moves += 1;
    const { depth, run } = current;
    if (run !== null) closed.add(run);
    settle(plain(current.device));
    leaving += 1;
    history.go(-depth);
  }

  return {
    register(name, popup) {
      if (!popup?.el) {
        throw new Error(`svolder: the popup ${name} is registered with its element, \`el\``);
      }
      // what `show(page)` draws before the stack has `el` is the popup's from now on
      claim(popup.el);
      clients.set(name, { popup, standing: null, part: null, loading: false });
    },

    open(name, page = null) {
      when(() => {
        if (!clients.has(name)) return;
        if (!may(name)) {
          clients.get(name).popup.refused?.();
          return;
        }
        if (current.popup === name) {
          step(page);
          return;
        }
        const run = current.popup ? current.run : `${born}${++runs}`;
        gesture(name, 'opened');
        push({ device: current.device, popup: name, page, depth: current.depth + 1, run });
      });
    },

    step,

    close,

    state: () => ({ device: current.device, popup: current.popup, page: current.page }),

    pushState(state, title, address) {
      when(() => {
        const url = state?.device ?? current.device;
        if (current.popup && clients.get(current.popup).popup.stays === true) {
          move(url, address);
          return;
        }
        const entry = plain(url);
        put(entry, address, true);
        settle(entry);
      });
    },

    shown(url, address) {
      // a step of the history showed this device itself: the entry names it already
      if (typeof url !== 'string' || (leaving === 0 && url === current.device)) return;
      when(() => move(url, address));
    },
  };
}
