/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The sky's events, scheduled by the clock and never by the frame: the sun's rays when the bass has
// been held high for a few seconds, a shooting star when the highs jump and a shower of them when
// the highs stay high. High and a jump are read against the loudest the bass or the highs reached in
// the last minute, so a quiet record and a loud one fire alike. Every wait and average is in seconds,
// so a screen at 30 or at 144 frames a second fires the same events. `levels()` says what the music
// reaches, for tuning the numbers.

/** The numbers of the events, in seconds; the levels are shares of the loudest of the last minute
 * (`floor` at least, in the 0 to 1 levels of the bands). */
export const SKY = Object.freeze({
  /** The least level the bass or the highs are read against: under it a level is a share of this. */
  floor: 0.1,
  /** How a band is read: its level eased over `envTau` s (a kick or a hat is one rise, not a hundred); its
   * place in the last minute's range, `(env - trough) / max(range, peak - trough)`, where the peak and the
   * trough both relax toward the envelope over `rangeTau` s and `range` is the least range that counts (a
   * flat drone or near silence has none). A place of `bassMin` or `showerMin` or more fills a leaky count
   * by `fill` a second, anything under it drains it by `drain` a second, so a signal that dips between
   * beats still counts as held; the events fire when the count reaches their hold. */
  envTau: 0.4,
  rangeTau: 60,
  range: 0.01,
  rangeShare: 0.1,
  fill: 1,
  drain: 0.5,
  /** The place in the range held for `bassHold` seconds that brings the rays. */
  bassMin: 0.8,
  bassHold: 3,
  /** The least time between the starts of two rays. */
  raysGap: 60,
  /** The rays' envelope: in, held, out; and their strongest alpha. */
  raysIn: 2,
  raysHold: 6,
  raysOut: 4,
  raysAlpha: 0.55,
  /** The rays turn clockwise round the sun while they show, this many degrees a second in Full (half
   * in Calm), from a random angle at each showing. */
  raysSpin: 6,
  /** A star: the highs jump this far above their average, as a share of the minute's loudest highs, while at
   * least `starMin` of it. */
  starJump: 0.45,
  starMin: 0.5,
  /** The average of the highs is taken over this many seconds (a time constant). */
  starAvg: 1,
  starGap: 5,
  /** The chance a transient brings a star once the gap has passed. */
  starChance: 0.5,
  starLife: 0.8,
  /** A shower: the highs held at `showerMin` or more for `showerHold` seconds bring `showerStars`
   * (this many at least, and up to `showerMore` more) over `showerSpan` seconds, at most one shower
   * every `showerGap` seconds; its stars ignore the gap between stars. */
  showerMin: 0.8,
  showerHold: 2,
  showerStars: 3,
  showerMore: 3,
  showerSpan: 1.5,
  showerGap: 40,
  /** The seconds the readout's maxima look back over. */
  readout: 60,
  /** How many firings the log keeps. */
  log: 20,
  /** A step this many seconds after the last (a hidden tab, a stall) breaks the count and the average. */
  gap: 0.5,
});

/** The rays' envelope (0 to 1) `since` seconds after they began. */
export function raysEnvelope(since) {
  const { raysIn, raysHold, raysOut } = SKY;
  if (!(since > 0)) return 0;
  if (since < raysIn) return since / raysIn;
  if (since < raysIn + raysHold) return 1;
  return Math.max(0, 1 - (since - raysIn - raysHold) / raysOut);
}

/** A band's reading: the level eased, the peak and the trough of the last minute, the place in their range
 * and the leaky count of how long it has been high. */
const band = () => ({
  env: null, peak: 0, trough: 0, pos: 0, count: 0,
});

/** Read `level` into `b` after `dt` seconds; a place of `min` or more in the range is high. */
function follow(b, level, dt, min, stalled) {
  if (b.env === null) {
    Object.assign(b, { env: level, peak: level, trough: level, pos: 0 });
    return;
  }
  if (stalled) {
    // a stall (a hidden tab) is not evidence of anything: the level is taken as it is, the count starts again
    b.env = level;
    b.count = 0;
    dt = 0;
  }
  b.env += (level - b.env) * (1 - Math.exp(-dt / SKY.envTau));
  const relax = 1 - Math.exp(-dt / SKY.rangeTau);
  b.peak = Math.max(b.env, b.peak + (b.env - b.peak) * relax);
  b.trough = Math.min(b.env, b.trough + (b.env - b.trough) * relax);
  b.pos = (b.env - b.trough) / Math.max(SKY.range, SKY.rangeShare * b.peak, b.peak - b.trough);
  b.count = Math.min(
    SKY.bassHold + SKY.showerHold,
    Math.max(0, b.count + dt * (b.pos >= min ? SKY.fill : -SKY.drain)),
  );
}

/**
 * The scheduler. `step(now, { bass, highs, on })` (`now` in seconds, the levels 0 to 1) returns
 * `{ rays, stars, raysAngle }`: the rays' alpha now, the angle they have turned (radians, clockwise; `spin`
 * is the share of `SKY.raysSpin` they turn at, 1 in Full and 0.5 in Calm) and the stars alive, each
 * `{ t0, x, y, dx }` in fractions of the sky. Nothing fires while not `on`, and what was showing is dropped. `log` holds the last
 * 20 firings (`rays`, `star`, `shower`) with their inputs; `levels()` is the readout
 * `{ bass, highs, bassHeldS, highsJump, bassRel, highsRel, jumpRel, max60: { bass, highs, highsJump } }`
 * (`bassRel` and `highsRel` are the places in the last minute's range, `bassHeldS` the leaky count); `clear()` forgets everything.
 */
export function createSky({ random = Math.random } = {}) {
  const log = [];
  let last = null;
  const bandBass = band();
  const bandHighs = band();
  let raysAt = -Infinity;
  let starAt = -Infinity;
  let raysLive = false;
  /** How far the rays have turned, in radians, clockwise on the screen. */
  let raysAngle = 0;
  let avg = null;
  let rising = false;
  let stars = [];
  let showerAt = -Infinity;
  /** The times the stars of a shower have still to show at. */
  let due = [];
  let latest = 0;
  const current = {
    bass: 0, highs: 0, jump: 0, bassRel: 0, highsRel: 0, jumpRel: 0, starRel: 0,
  };
  /** The maxima of each whole second, newest last. */
  const seconds = [];

  const note = (entry) => {
    log.push(entry);
    if (log.length > SKY.log) log.shift();
  };

  function off() {
    bandBass.env = null;
    bandHighs.env = null;
    avg = null;
    rising = false;
    stars = [];
    due = [];
    raysLive = false;
  }

  const star = (at) => ({
    t0: at,
    x: 0.15 + random() * 0.7,
    y: 0.05 + random() * 0.3,
    dx: (random() < 0.5 ? -1 : 1) * 0.25,
  });

  /** The readout's state: the levels now and the maxima of each second; the levels against the last
   * minute's loudest. */
  function read(at, bass, highs, jump) {
    const whole = Math.floor(at);
    let slot = seconds[seconds.length - 1];
    if (!slot || slot.at !== whole) {
      slot = { at: whole, bass: 0, highs: 0, jump: 0 };
      seconds.push(slot);
      while (seconds.length > 0 && seconds[0].at <= whole - SKY.readout) seconds.shift();
    }
    slot.bass = Math.max(slot.bass, bass);
    slot.highs = Math.max(slot.highs, highs);
    slot.jump = Math.max(slot.jump, jump);
    latest = at;
    let bassTop = SKY.floor;
    let highsTop = SKY.floor;
    for (const one of seconds) {
      bassTop = Math.max(bassTop, one.bass);
      highsTop = Math.max(highsTop, one.highs);
    }
    Object.assign(current, {
      bass,
      highs,
      jump,
      starRel: Math.min(1, highs / highsTop),
      jumpRel: jump / highsTop,
    });
  }

  return {
    log,

    step(now, {
      bass = 0, highs = 0, on = false, spin = 1,
    } = {}) {
      const dt = last === null ? 0 : Math.max(0, now - last);
      last = now;
      if (!on) {
        off();
        read(now, bass, highs, 0);
        return { rays: 0, stars: [], raysAngle };
      }
      const stalled = dt > SKY.gap;
      if (stalled) {
        avg = null;
        rising = false;
      }
      follow(bandBass, bass, dt, SKY.bassMin, stalled);
      follow(bandHighs, highs, dt, SKY.showerMin, stalled);
      avg ??= highs;
      const jump = highs - avg;
      read(now, bass, highs, Math.max(0, jump));
      current.bassRel = bandBass.pos;
      current.highsRel = bandHighs.pos;
      const { starRel, jumpRel } = current;
      if (bandBass.count >= SKY.bassHold && now - raysAt >= SKY.raysGap) {
        raysAt = now;
        raysLive = true;
        raysAngle = random() * 2 * Math.PI;
        note({ kind: 'rays', at: now, bass, highs, held: bandBass.count });
        bandBass.count = 0;
      }
      const strong = jumpRel >= SKY.starJump && starRel >= SKY.starMin;
      if (strong && !rising && now - starAt >= SKY.starGap && random() < SKY.starChance) {
        starAt = now;
        stars.push(star(now));
        note({ kind: 'star', at: now, bass, highs, avg });
      }
      rising = strong;
      if (bandHighs.count >= SKY.showerHold && now - showerAt >= SKY.showerGap) {
        showerAt = now;
        const count = SKY.showerStars + Math.floor(random() * (SKY.showerMore + 1));
        for (let i = 0; i < count; i++) due.push(now + (SKY.showerSpan * i) / (count - 1));
        note({ kind: 'shower', at: now, bass, highs, held: bandHighs.count, count });
        bandHighs.count = 0;
      }
      if (due.length > 0) {
        for (const at of due.filter((when) => when <= now)) stars.push(star(at));
        due = due.filter((when) => when > now);
      }
      avg += (highs - avg) * (1 - Math.exp(-dt / SKY.starAvg));
      stars = stars.filter((one) => now - one.t0 < SKY.starLife);
      if (raysLive) raysAngle = (raysAngle + (SKY.raysSpin * Math.PI / 180) * spin * dt) % (2 * Math.PI);
      return {
        rays: raysLive ? raysEnvelope(now - raysAt) * SKY.raysAlpha : 0, stars: stars.slice(), raysAngle,
      };
    },

    levels() {
      const top = (key) => seconds.filter((slot) => slot.at > Math.floor(latest) - SKY.readout)
        .reduce((most, slot) => Math.max(most, slot[key]), 0);
      return {
        bass: current.bass,
        highs: current.highs,
        bassHeldS: bandBass.env === null ? 0 : bandBass.count,
        highsJump: current.jump,
        bassRel: current.bassRel,
        highsRel: current.highsRel,
        jumpRel: current.jumpRel,
        max60: { bass: top('bass'), highs: top('highs'), highsJump: top('jump') },
      };
    },

    clear() {
      log.length = 0;
      last = null;
      off();
      raysAt = -Infinity;
      starAt = -Infinity;
      showerAt = -Infinity;
      seconds.length = 0;
      Object.assign(current, {
        bass: 0, highs: 0, jump: 0, bassRel: 0, highsRel: 0, jumpRel: 0, starRel: 0,
      });
      bandBass.count = 0;
      bandHighs.count = 0;
    },
  };
}
