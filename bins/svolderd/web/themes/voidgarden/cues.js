/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The scene's signal cues, read from the page model: what the computer, the player and the device
// say becomes `{ mute, pause, changed, cross, weak, linkDrop, volume, rateHz, inputRateHz }`. Pure:
// no DOM, no timers; the page model, the clock and the two things the model cannot say (whether
// the path changes the sound, and whether the frames are silent) are handed in.
//
//   cuesOf(page, opts)        the cues the model alone gives; `opts`: `changed` (the wire's
//                             judgement of the path, never made here), `side` ('device' or
//                             'computer', whose volume is the shown one), `nowMs` (the daemon's
//                             clock for the signal's age; the facts' own `now_ms` by default)
//   new CueTracker()          `update(page, { dt, silent, changed, side, nowMs })`: the cues with
//                             what needs memory: 2 s of silent frames also pause, sound heard ends a player's pause, and a link that
//                             leaves `on` is `linkDrop` for the one update that sees it

/** Seconds of silent frames before the scene drifts as if paused. */
export const SILENT_PAUSE_S = 2;
/** Seconds of silent frames before a player's own paused or stopped status may pause the scene:
 * a quiet passage, a fade or a gap between tracks under a stale paused player must not flicker it. */
export const STATUS_PAUSE_S = 0.5;

/** One bar of the Bluetooth signal is below this, dBm: `Signal::weak` in
 * crates/svolder-ipc/src/socket/signal.rs (`bars`: 4 from -55, 3 from -67, 2 from -80); change
 * both together. */
export const WEAK_BELOW_DBM = -80;

/** A reading older than this, seconds, is not fresh: `SIGNAL_OLD_SECS` in the same file. While a
 * BLE link is up BlueZ reports no strength, so the reading is the last advert before the link. */
export const SIGNAL_OLD_SECS = 120;

/** The path words that say the computer's chain has crossfeed on: `chain_words` in
 * crates/svolder-ipc/src/host/path.rs; change both together. */
const CROSSFEED_WORD = 'crossfeed on';

/** The shown device of `page`: its model, or null. */
const deviceOf = (page) => page?.device ?? null;

/** The link state of the shown device (`on`, `wait`, `off`), or null when none is known. */
function linkState(page) {
  const own = deviceOf(page)?.link?.state;
  if (own) return own;
  const entry = (page?.fleet?.devices ?? []).find((d) => d.url === page.shown);
  return entry?.link?.state ?? null;
}

/** The fraction of a device volume model's range (`lo` to `hi`, at `raw`), or null while unread. */
function fraction(volume) {
  if (!volume || !Number.isFinite(volume.raw) || !Number.isFinite(volume.lo) || !Number.isFinite(volume.hi)) {
    return null;
  }
  if (volume.hi <= volume.lo) return null;
  return Math.min(1, Math.max(0, (volume.raw - volume.lo) / (volume.hi - volume.lo)));
}

/** The computer's volume as a fraction: its `pct` of the sink's volume, or null while unread. */
function computerFraction(volume) {
  return Number.isFinite(volume?.pct) ? Math.min(1, Math.max(0, volume.pct / 100)) : null;
}

/** A rate readout (`96 kHz`, `44.1 kHz`, `48000 Hz`) in Hz, or null. */
export function rateOf(text) {
  const m = /^\s*([0-9]+(?:[.,][0-9]+)?)\s*(k?hz)\s*$/i.exec(String(text ?? ''));
  if (!m) return null;
  const n = Number(m[1].replace(',', '.'));
  return Math.round(m[2].toLowerCase() === 'khz' ? n * 1000 : n);
}

/** The device's signal in dBm, from the head the model words it in (`−62 dBm advert`, with U+2212
 * or a hyphen), or null. The model carries the strength only as those words (`facts.signal_age`),
 * which `Signal::describe_undated` in crates/svolder-ipc/src/socket/signal.rs writes; change both
 * together. */
export function dbmOf(head) {
  const m = /^\s*([−-]?\d+) dBm/.exec(String(head ?? ''));
  return m ? Number(m[1].replace('−', '-')) : null;
}

/**
 * Whether the shown device's signal is weak: a fresh reading below `WEAK_BELOW_DBM`. No reading,
 * one older than `SIGNAL_OLD_SECS`, or no clock to age it by, is not weak: the link state alone
 * says the rest.
 */
export function weakOf(page, nowMs = null) {
  const facts = deviceOf(page)?.facts;
  const age = facts?.signal_age;
  const dbm = dbmOf(age?.head);
  if (dbm === null || !Number.isFinite(age.heard_at_ms)) return false;
  const now = Number.isFinite(nowMs) ? nowMs : facts.now_ms;
  if (!Number.isFinite(now)) return false;
  if (Math.max(0, now - age.heard_at_ms) > SIGNAL_OLD_SECS * 1000) return false;
  return dbm < WEAK_BELOW_DBM;
}

/** The cues the page model gives by itself. `changed` is the wire's, passed in untouched. */
export function cuesOf(page, { changed = false, side = 'device', nowMs = null } = {}) {
  const device = deviceOf(page);
  const sink = page?.host?.route?.sink;
  const status = page?.host?.track?.status;
  const computer = side === 'computer';
  return {
    mute: Boolean(device?.volume?.muted) || Boolean(sink?.muted),
    pause: Boolean(status) && status !== 'playing',
    changed: Boolean(changed),
    cross: (device?.playing?.path?.words ?? []).includes(CROSSFEED_WORD),
    weak: weakOf(page, nowMs),
    linkDrop: false,
    volume: computer ? computerFraction(page?.host?.computer?.volume) : fraction(device?.volume),
    rateHz: Number.isFinite(sink?.rate_hz) ? sink.rate_hz : null,
    inputRateHz: rateOf((device?.controls ?? []).find((c) => c.key === 'sample_rate' && c.kind === 'readout')?.text),
  };
}

export class CueTracker {
  constructor() {
    this.clear();
  }

  clear() {
    this.silentFor = 0;
    this.link = null;
    this.shown = null;
  }

  /** The cues for `page` after `dt` seconds; `silent` is whether the latest frames carry no sound. */
  update(page, {
    dt = 0, silent = false, changed = false, side = 'device', nowMs = null,
  } = {}) {
    const cues = cuesOf(page, { changed, side, nowMs });
    this.silentFor = silent ? this.silentFor + dt : 0;
    if (this.silentFor >= SILENT_PAUSE_S) cues.pause = true;
    // sound heard lately outranks a player's status: the capture is of the whole sink, and a paused or
    // stale player (another one, or a tab) must not rest the features and slow the camera; the status
    // counts only after STATUS_PAUSE_S of silent frames, so an isolated quiet frame never flickers it
    else if (this.silentFor < STATUS_PAUSE_S) cues.pause = false;
    // the link is remembered per shown device: showing another device is never a drop
    const shown = page?.shown ?? null;
    if (shown !== null && shown !== this.shown) {
      this.shown = shown;
      this.link = null;
    }
    const link = linkState(page);
    cues.linkDrop = this.link === 'on' && link !== null && link !== 'on';
    if (link !== null) this.link = link;
    return cues;
  }
}
