/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The computer's side of the page, for every theme: which side this viewer looks at (the device or
// the computer, kept in the browser and never written to the daemon), and the computer's volume and
// tone control as the parts a theme draws, read out of the page's `host.computer` (the host event
// carries it for any shown device). The curve, the points' markers, the line that counts the
// points and the preamp, and the point a tap or `+ Add point` makes are plain data here; the writes
// are `actions.computer`.
//
// What this viewer is changing is pending: a value asked for (the volume, a point's frequency, gain
// or Q) is drawn as asked and is not moved by a host update meanwhile; once the writes have
// settled the host's own model is read again and the pending values go. Flipping to the other side
// writes nothing: it ends any drag or hold a theme has registered with `track`, and keeps the
// side in storage.

import { readSide, writeSide } from './prefs.js';
import { dbText } from './words.js';
import { F_MAX, F_MIN, logFreqs, POINTS, responseDb, xOf, yOf } from './tone-curve.js';

/** The most points the computer's EQ holds, when the model says nothing else. */
const POINTS_MAX = 16;
/** What the switch calls the computer. */
export const THIS_COMPUTER = 'This computer';
/** The ranges of a hand by field, `[low, high, places]`: what the daemon holds a typed or dragged
 * value to (`HAND` in crates/svolder-host/src/eq/points.rs, pinned by core-computer.mjs), and the
 * decimal places a typed value is rounded to before it is written. */
export const HAND = { f: [F_MIN, F_MAX, 2], g: [-12, 12, 1], q: [0.1, 10, 2] };
/** The types a point of the computer's EQ can have, as the daemon names them: peaking, low shelf,
 * high shelf (`kind_word` in crates/svolder-host/src/eq/control.rs, pinned by core-computer.mjs). */
export const POINT_TYPES = ['PK', 'LSC', 'HSC'];
/** A press that moves less than this (CSS px) and lets go within `TAP_MS` is a tap. */
export const TAP_PX = 6;
export const TAP_MS = 600;
/** A wheel burst ends, and its last value is written as the final one, after this long. */
export const WHEEL_END_MS = 300;
/** The rows of the points' table shown before it scrolls. */
export const ROWS_SHOWN = 6;
/** The one point a flat curve is: what the daemon holds for it, and what an empty list stands for. */
export const FLAT_POINT = { type: 'PK', f: 1000, g: 0, q: 1 };

const FREQS = logFreqs(POINTS);

/** A marker for each point, sitting on the summed curve at the point's frequency (held to the
 * graph's edges), so a shelf's dot is on the line and not at its written gain. */
export function markersOf(points) {
  const bands = points.map((p) => ({ f0: p.f, gainDb: p.g, q: p.q, type: p.type }));
  const hz = points.map((p) => clamp(p.f, F_MIN, F_MAX));
  const db = responseDb(bands, hz);
  return points.map((p, i) => ({ n: p.n, type: p.type, x: xOf(hz[i]), y: yOf(db[i]) }));
}

/** The id a point is held by: its slot, which is its own for as long as it exists whatever place
 * the table lists it in; a model that gives none (an older daemon) has the point's number. */
const pointId = (point, index) => point.slot ?? index + 1;

/** The id of point `n` (1-based) of `points`; `n` itself for a point the list does not have. */
export function idAt(points, n) {
  const point = points[n - 1];
  return point ? pointId(point, n - 1) : n;
}

/** Where the point with `id` is listed (1-based), or `null` when it is gone. A flat list the daemon
 * sends with no point is the one point 1. */
export function indexOfId(points, id) {
  if (points.length === 0) return id === 1 ? 1 : null;
  const at = points.findIndex((point, i) => pointId(point, i) === id);
  return at < 0 ? null : at + 1;
}

/** Where a point added with frequency `f` is listed: after every point at or below it. */
export function addedAt(points, f) {
  return points.filter((p) => p.f <= f).length + 1;
}
const round = (value, places) => Math.round(value * 10 ** places) / 10 ** places;
const clamp = (value, low, high) => Math.min(Math.max(value, low), high);

/** The most points the computer's EQ holds, from its model (`eq`). */
export const maxPoints = (eq) => eq?.max ?? POINTS_MAX;

/** What a table that is full answers to an add: nothing is added, and the words say why. */
export const fullAnswer = (max) => ({
  ok: false, status: 0, full: true, max, error: `${max} is the most`,
});

/** The point a tap at `hz` and `db` makes: a PK held to the hand ranges, Q 1.0. */
export function pointAt(hz, db) {
  return {
    type: 'PK',
    f: round(clamp(Number.isFinite(hz) ? hz : FLAT_POINT.f, HAND.f[0], HAND.f[1]), HAND.f[2]),
    g: round(clamp(Number.isFinite(db) ? db : 0, HAND.g[0], HAND.g[1]), HAND.g[2]),
    q: 1,
  };
}

/** The frequency `+ Add point` uses: the log-middle of the widest gap between 20 Hz, the points'
 * frequencies and 20 kHz (the lowest gap on a tie). */
export function nextFree(points) {
  const edges = [F_MIN, ...points.map((p) => clamp(p.f, F_MIN, F_MAX)).sort((a, b) => a - b), F_MAX];
  let best = 0;
  let width = -1;
  for (let i = 0; i + 1 < edges.length; i++) {
    const gap = Math.log(edges[i + 1] / edges[i]);
    if (gap > width) {
      width = gap;
      best = i;
    }
  }
  return round(Math.sqrt(edges[best] * edges[best + 1]), 2);
}

/**
 * The pending values: those this viewer is changing, by key (`volume`, `mute`, `mode`,
 * `point:<id>`, the id being the point's slot), until the daemon's own model has been read after
 * them. `fetch` reads (the page's); `listen(fn)` hears `'held'` when a value becomes pending and
 * `'settled'` when pending values went.
 */
export function createPending({ store, fetch }) {
  const held = new Map();
  const heard = new Set();
  const notify = (type, extra) => heard.forEach((fn) => fn(type, extra));
  let syncing = false;
  let again = false;
  // how many host reads the page has seen: a read of the model that was overtaken by a newer one
  // is not shown
  let reads = 0;

  /** Whether the computer's model already holds `value` for the hold `key`. */
  function matches(key, value, computer) {
    if (!computer) return false;
    if (key === 'volume') return computer.volume?.pct === value;
    if (key === 'mute') return computer.volume?.muted === value;
    if (key === 'mode') return computer.eq?.on === value;
    if (key === 'bypass') return computer.eq?.bypass?.on === value;
    if (key === 'crossfeed') {
      const now = computer.eq?.crossfeed;
      return Boolean(now) && Object.entries(value).every(([field, v]) => now[field] === v);
    }
    if (key === 'loudness') {
      const now = computer.eq?.loudness;
      return Boolean(now) && Object.entries(value).every(([field, v]) => now[field] === v);
    }
    if (key === 'speakers') {
      const now = computer.eq?.speakers;
      return Boolean(now) && Object.entries(value).every(([field, v]) => now[field] === v);
    }
    const id = /^point:(\d+)$/.exec(key)?.[1];
    if (!id) return false;
    // the point is found by its id: the daemon lists the points by frequency, so it may be
    // listed elsewhere than where it was
    const points = computer.eq?.points ?? [];
    const at = indexOfId(points, Number(id));
    return at !== null && Object.entries(value).every(([field, v]) => points[at - 1]?.[field] === v);
  }

  // The points whose holds went a moment ago are followed a little longer: the read that frees a
  // hold may be the last of a drag, listed before the sort that its release brings.
  const tracks = new Map();
  const TRACK_MS = 2000;

  /** Let go of the hold `key`. */
  function forget(key) {
    const hold = held.get(key);
    held.delete(key);
    if (hold?.at && key.startsWith('point:')) {
      tracks.set(key, { at: hold.at, expires: Date.now() + TRACK_MS });
    }
  }

  /** How many presets the daemon has loaded, or `null` when it does not say. */
  function loadsOf(computer) {
    const loads = computer?.eq?.loads;
    return Number.isInteger(loads) ? loads : null;
  }

  /** Where the point a hold is on is listed in `computer`'s model, or `null`. */
  function listedAt(key, computer) {
    const id = /^point:(\d+)$/.exec(key)?.[1];
    return id ? indexOfId(computer?.eq?.points ?? [], Number(id)) : null;
  }

  // A held value masks an older read only: once it is released, a newer read that says the same
  // takes its place at once.
  store.on('host', (host) => {
    reads++;
    // a preset loaded meanwhile renumbers the slots: a point held is another point now, so its
    // hold ends and nothing more is written for it (an edit of its own makes the preset `custom`).
    // The daemon counts its loads, so a preset reloaded under the same name counts too; a daemon
    // that sends no count is followed by the name.
    const loaded = host?.computer?.eq?.preset ?? null;
    const loads = loadsOf(host?.computer);
    const reloaded = (hold) => (hold.loads !== null && loads !== null
      ? loads !== hold.loads
      : loaded !== hold.preset && loaded !== 'custom');
    for (const [key, hold] of [...held]) {
      if (key.startsWith('point:') && !hold.released && reloaded(hold)) {
        held.delete(key);
        tracks.delete(key);
        notify('ended', { key });
        notify('settled');
      }
    }
    // a point that is held and was listed elsewhere by this read (its own edit ended, or another
    // front end's did) is followed: the themes holding its number are told where it went
    for (const [key, hold] of [...held, ...tracks]) {
      if (hold.expires && hold.expires < Date.now()) {
        tracks.delete(key);
        continue;
      }
      const now = listedAt(key, host?.computer);
      // the point is gone from the list (another front end removed it): whoever follows it is told
      if (hold.at && !now && host?.computer?.eq && !hold.gone) {
        hold.gone = true;
        notify('gone', { n: hold.at });
        continue;
      }
      if (hold.at && now && now !== hold.at) {
        const from = hold.at;
        hold.at = now;
        notify('moved', { from, to: now });
      }
    }
    let freed = false;
    for (const [key, hold] of [...held]) {
      if (hold.released && matches(key, hold.value, host?.computer)) {
        forget(key);
        freed = true;
      }
    }
    if (freed) notify('settled');
  });

  /** The model of the computer read afresh: the host event, with the volume kept when the read
   * has none yet. */
  function apply(host) {
    const current = store.page().host;
    if (!current) return;
    let computer = host?.computer ?? null;
    if (computer && computer.volume === null && current.computer?.volume) {
      computer = { ...computer, volume: current.computer.volume };
    }
    store.emit('host', { ...current, computer });
  }

  async function resync() {
    if (syncing) {
      again = true;
      return;
    }
    syncing = true;
    try {
      do {
        again = false;
        const mine = [...held].filter(([, hold]) => hold.released);
        const shown = store.shown();
        const before = reads;
        try {
          const answer = await fetch(`api/v1/host${shown ? `?device=${shown}` : ''}`,
            { credentials: 'same-origin' });
          const host = answer.status === 200 ? await answer.json() : undefined;
          // a host event that came while this read was on its way is newer: it stands
          if (host !== undefined && reads === before) apply(host);
        } catch {
          // the host events bring the model; the held values go either way
        }
        for (const [key, hold] of mine) if (held.get(key) === hold) forget(key);
        notify('settled');
      } while (again);
    } finally {
      syncing = false;
    }
  }

  return {
    listen(fn) {
      heard.add(fn);
      return () => heard.delete(fn);
    },
    get: (key) => held.get(key) ?? null,
    set(key, value) {
      const at = listedAt(key, store.page().host?.computer);
      const preset = store.page().host?.computer?.eq?.preset ?? null;
      const loads = held.get(key)?.loads ?? loadsOf(store.page().host?.computer);
      held.set(key, { value, released: false, at, preset: held.get(key)?.preset ?? preset, loads });
      notify('held');
    },
    /** The change is over: the model is read again and the value goes. */
    release(key) {
      const hold = held.get(key);
      if (hold) hold.released = true;
      return resync();
    },
    /** The value goes at once (a refusal: what the daemon holds is shown). */
    drop(key) {
      if (held.has(key)) {
        forget(key);
        notify('settled');
      }
    },
    clear() {
      if (held.size === 0) return;
      held.clear();
      tracks.clear();
      notify('settled');
    },
    /** Read the computer's model again (a structural write landed). */
    resync,
  };
}

/** The core's `computer` over `store`: the kept side (in `storage`), the parts of the computer's
 * controls, and the points a tap makes. `pending` is the values this viewer is changing. */
export function createComputer({ store, storage, pending }) {
  let kept = readSide(storage);
  const listeners = new Set();
  const open = new Set();

  const notify = (type, extra) => {
    for (const fn of [...listeners]) {
      try {
        fn({ type, side: computer.side(), ...extra });
      } catch (error) {
        globalThis.console?.error?.('svolder: a listener of the computer threw', error);
      }
    }
  };

  const modelOf = (page) => page?.host?.computer ?? null;
  const drawable = (page = store.page()) => Boolean(modelOf(page) && page?.device);
  const effective = (page = store.page()) => {
    const model = modelOf(page);
    return kept === 'computer' && drawable(page) && !model.unavailable ? 'computer' : 'dac';
  };

  // The point in hand in the table (1-based) and the device it was picked on: a page's own, which
  // goes with the side or the device and follows a point the daemon lists elsewhere.
  const picked = { n: null, device: null };

  pending.listen((type, extra) => {
    if (type === 'moved' && picked.n === extra.from) picked.n = extra.to;
    notify(type, extra);
  });
  // a whole page read afresh (the stream opened again) keeps what a drag of this device is
  // holding; another device shown has none of it
  let shownBefore = store.shown();
  store.on('model', () => {
    const now = store.shown();
    if (now !== shownBefore) pending.clear();
    shownBefore = now;
  });

  const computer = {
    /** The side shown now: the kept one, unless the computer cannot be drawn or is unavailable
     * (the device's side then, and the switch's place says why). */
    side: () => effective(),
    /** The point in hand (1-based), or `null`: it goes when the device shown changes or the side
     * flips, and it follows a point that an edit moved in the table's order. */
    picked(page = store.page()) {
      const shown = store.shown();
      if (picked.device !== shown || effective(page) !== 'computer') {
        picked.n = null;
        picked.device = shown;
      }
      return picked.n;
    },
    /** Put point `n` (1-based, or `null` for none) in hand. */
    pick(n) {
      picked.n = n;
      picked.device = store.shown();
    },
    /** Put point `n` (or `null`) in hand because of what the core itself did (a point removed
     * renumbered the rest), and say so: the listeners hear `'picked'` with `n`. A pick the page
     * makes is `pick`, which says nothing. */
    repick(n) {
      computer.pick(n);
      notify('picked', { n });
    },
    /** Read the computer's model again (a preset renamed or deleted changed its list). */
    resync: () => pending.resync(),
    /** The side this viewer chose, whether or not it can be shown. */
    kept: () => kept,
    /**
     * Look at `side` (`'dac'` or `'computer'`). Writes nothing to the daemon: any drag or hold
     * tracked is ended (`cancel('flip')`, or `end()` for a hold), the side is kept, the listeners
     * hear it.
     */
    flip(side) {
      if (side !== 'dac' && side !== 'computer') return;
      for (const handle of [...open]) {
        if (typeof handle.cancel === 'function') handle.cancel('flip');
        else handle.end?.();
      }
      open.clear();
      kept = side;
      computer.pick(null);
      writeSide(side, storage);
      notify('flip');
    },
    /** `fn({ type: 'flip' | 'held' | 'settled' | 'moved' | 'picked' | 'gone', side })` hears what
     * the core changes itself; every event carries `side`. `'moved'` also carries `from` and `to`
     * (1-based): an edit that ended moved a point in the table's frequency order, and a theme
     * holding a point's number follows it. `'picked'` carries `n`: the point in hand the core put
     * there (or took out of hand, `null`) after a remove. `'gone'` carries `n`: a point held at
     * that place was removed. The host events themselves come through `model.on('host')`. Returns
     * the way to stop. */
    on(fn) {
      listeners.add(fn);
      return () => listeners.delete(fn);
    },
    /** A drag (`cancel(why)`) or hold (`end()`) a flip ends; returns the way to forget it. */
    track(handle) {
      open.add(handle);
      return () => open.delete(handle);
    },
    /** Whether the page has a computer side to look at: a device is shown and the computer's
     * controls are in the model (any kind of device, an amp too). */
    drawable,

    /** The switch: the shown device's name, the computer's, the side shown, and the words of why
     * the computer cannot be used (`null` when it can). `null` where there is no switch. */
    switchOf(page) {
      if (!drawable(page)) return null;
      return {
        dac: page.device.name,
        computer: THIS_COMPUTER,
        side: effective(page),
        unavailable: modelOf(page).unavailable ?? null,
      };
    },

    /**
     * What a page's drawing needs of the computer, or `null` where there is no switch (no device
     * shown, or no computer in the model): `{ side, sw, writes, picked, volume, tone, eq }`. `volume`
     * and `tone` are made when first read, and only on the computer side (`null` on the device's);
     * so is `eq`, the light read of the EQ's state (`eqOf`) for a part that needs no curve.
     */
    view(page = store.page()) {
      const sw = computer.switchOf(page);
      if (!sw) return null;
      const on = effective(page) === 'computer';
      const point = computer.picked(page);
      const lazy = (make) => {
        let made;
        return () => (made ??= { value: make() }).value;
      };
      const volume = lazy(() => (on ? computer.volumeOf(page) : null));
      const tone = lazy(() => (on ? computer.toneOf(page) : null));
      const eq = lazy(() => (on ? computer.eqOf(page) : null));
      return {
        side: effective(page),
        sw,
        writes: Boolean(page.viewer?.writes),
        picked: point,
        get volume() { return volume(); },
        get tone() { return tone(); },
        get eq() { return eq(); },
      };
    },

    /** The volume as drawn: `pct` is `null` until the first read (then `text` is `—` and the
     * control is `inert`), `boosted` when another program put the sink above 100 %. */
    volumeOf(page) {
      if (!drawable(page)) return null;
      const read = modelOf(page).volume;
      const pct = pending.get('volume')?.value ?? read?.pct ?? null;
      const muted = pending.get('mute')?.value ?? read?.muted ?? false;
      let aria = 'Volume, not read yet';
      if (pct !== null) aria = `Volume ${pct} %${muted ? ', muted' : ''}`;
      return {
        pct,
        muted,
        text: pct === null ? '—' : String(pct),
        unit: '%',
        aria,
        inert: pct === null,
        boosted: pct !== null && pct > 100,
      };
    },

    /**
     * The state of the computer's EQ with no curve worked out: `{ on, preset, verified }`, or
     * `null` with no EQ (or no computer). `on` has the mode this viewer is changing laid over it,
     * `preset` is the name of the preset that is on (or `null`), `verified` is false while the
     * EQ's state is only what its state file says. `toneOf` says the same of these three.
     */
    eqOf(page) {
      if (!drawable(page)) return null;
      const eq = modelOf(page).eq;
      if (!eq) return null;
      return {
        on: pending.get('mode')?.value ?? eq.on,
        preset: eq.preset ?? null,
        verified: !eq.running_unknown,
      };
    },

    /**
     * The stages and levels of the chain, all from the model's `eq.levels` (its words are the
     * daemon's; nothing here works a level out): `{ stages, total, totalText, words, passthrough,
     * untouched, climbTo }` (`climbTo` the dB a climb is going to, else `null`), or `null` with no EQ or from a host that sends no levels. `stages` is EQ,
     * Loudness and Space in that order, each `{ key, name, on, level, text }`: `level` is the dB
     * (EQ's and Space's share) or the strength in percent (Loudness), `null` while the stage is
     * off; Space also carries `kind` (`crossfeed`, `speakers` or `off`). The EQ's `on` has the
     * mode this viewer is changing laid over it.
     */
    levelsOf(page) {
      if (!drawable(page)) return null;
      const eq = modelOf(page).eq;
      const levels = eq?.levels;
      if (!eq || !levels) return null;
      const eqOn = pending.get('mode')?.value ?? eq.on;
      const loud = eq.loudness;
      const kind = eq.speakers?.on ? 'speakers' : (eq.crossfeed?.on ? 'crossfeed' : 'off');
      const stage = (key, name, on, level, text, extra = {}) => ({
        key, name, on, level: on ? level : null, text: on ? text : '', ...extra,
      });
      return {
        stages: [
          stage('eq', 'EQ', eqOn, levels.eq_db, dbText(levels.eq_db)),
          stage('loudness', 'Loudness', Boolean(loud?.on), loud?.strength_pct ?? null,
            `${loud?.strength_pct ?? 0} %`),
          stage('space', 'Space', kind !== 'off', levels.space_db,
            dbText(levels.space_db), { kind }),
        ],
        total: levels.total_db,
        totalText: dbText(levels.total_db),
        words: levels.words ?? '',
        passthrough: Boolean(levels.passthrough),
        untouched: Boolean(levels.untouched),
        climbTo: typeof levels.climb_to_db === 'number' ? levels.climb_to_db : null,
      };
    },

    /**
     * The bypass as drawn: `{ on, can, held }`, or `null` from a host that has none (no bypass
     * is offered then). `on` has the bypass this viewer is changing laid over it; `can` is
     * whether turning it is possible now: it is on (a restore), or the EQ or a stage plays;
     * `held` is the stages it brings back (`eq`, `loudness`, `crossfeed`, `speakers`).
     */
    bypassOf(page) {
      if (!drawable(page)) return null;
      const eq = modelOf(page).eq;
      if (!eq?.bypass) return null;
      const on = pending.get('bypass')?.value ?? eq.bypass.on;
      const plays = Boolean(eq.on || eq.loudness?.on || eq.crossfeed?.on || eq.speakers?.on);
      return {
        on,
        can: !modelOf(page).unavailable && (on || plays),
        held: { eq: false, loudness: false, crossfeed: false, speakers: false, ...eq.bypass.held },
      };
    },

    /** The tone control of the page now (`toneOf` of the store's page). */
    tone: () => computer.toneOf(store.page()),

    /** The tone control as drawn, or `null`: the mode (Off, EQ), the points with the values this
     * viewer is changing laid over them, the curve at the 181 log frequencies (the points' own
     * types, shelves included), a marker for each point in the graph's viewBox (a point beyond the
     * hand ranges at the curve's edge), the line that counts them, the most points (`max`), whether
     * a point can be added, and the presets. `verified` is false while the EQ's state is only what
     * its state file says. */
    toneOf(page) {
      if (!drawable(page)) return null;
      const model = modelOf(page);
      const eq = model.eq;
      if (!eq) return null;
      const listed = (eq.points?.length ? eq.points : [FLAT_POINT]).map((point, i) => ({
        type: point.type, f: point.f, g: point.g, q: point.q, ...pending.get(`point:${pointId(point, i)}`)?.value,
      }));
      const points = listed.map((point, i) => ({ n: i + 1, ...point }));
      const { on, preset, verified } = computer.eqOf(page);
      const max = maxPoints(eq);
      return {
        modes: [
          { name: 'off', label: 'Off', on: !on },
          { name: 'eq', label: 'EQ', on },
        ],
        points,
        response: responseDb(
          points.map((p) => ({ f0: p.f, gainDb: p.g, q: p.q, type: p.type })),
          FREQS,
        ),
        markers: markersOf(points),
        line: `${points.length} of ${max} points · preamp ${dbText(eq.preamp_db ?? 0)} dB`,
        max,
        canAdd: points.length < max,
        presets: (model.presets ?? []).map((preset) => ({ ...preset })),
        preset,
        verified,
        live: eq.live !== false,
        note: eq.note ?? null,
      };
    },

    /** The point a tap on an empty spot makes: a PK at `hz` and `db`, held to the hand ranges, Q
     * 1.0; `null` when the table is full or there is no computer EQ. */
    addAt(hz, db) {
      const tone = computer.toneOf(store.page());
      return tone?.canAdd ? pointAt(hz, db) : null;
    },

    /** The frequency `+ Add point` uses (`nextFree`). */
    nextFree,
  };
  return computer;
}
