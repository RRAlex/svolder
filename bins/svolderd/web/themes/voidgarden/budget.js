/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The frame budget: smooth first, the least perceptible loss. The scene aims at 60 a second where
// the display and the GPU allow it (a desktop), else at 30 paced evenly (a phone, or a screen
// that cannot do 60); it never aims lower, it gives picture up instead. The ladder is a hierarchy
// of small steps, the backdrop first and the central sphere and the effects last: the backdrop's
// detail in two rounds (the sky's, the water's, the land's detail normals and rings, the trees'
// density, then the backdrop's share of the resolution: the sky, the land, the river and the trees
// drawn smaller and laid under the rest), then the particles, then the post chain's cost (its bloom
// drawn smaller), then the effects (the bloom's second octave, the grit layers), then the hero's
// particles, and only then the whole frame's resolution (to what the reference build's budget
// reached), the one step that softens the sphere. While the target is 60 only the backdrop's steps
// are spent chasing it; when the last of them has not held 60, the target becomes 30 and the
// budget goes back to the least step that holds 30 (the costs it read on the way down name it).
// The draws a second are the verdict as much as the times are (the GPU time leaves out the
// compositor), and a hitch is not one: giving 60 up or taking the effects waits for the slowness
// to last. A step back up comes after a calm spell and is on trial; one that is too much is undone
// and the way up shuts for a while (longer each time); a step given up is not tried again for
// five minutes. 30 is never for good: 60 is tried again, on trial at the end of the backdrop's
// steps, whenever its wait is over and the picture has been calm for a spell, and a failed trial
// goes back to 30 at once. The draws-a-second floors follow the display's own rate (what the
// offered frames measure): a 50 Hz screen paces the 30 cap as 25 and is not slow for it. A saved
// tier is keyed by the tier, the GPU and the size class (a step number means one thing on one
// ladder), and a resize maps the levels by their meaning. A phone's backdrop share stops at 0.6
// and its frame's last resort is 0.75.
// Pure and time-based: the draw time, the GPU time (where the browser gives one) and the clock
// (seconds) are handed in, so the same draws give the same steps at 30, 60 or 144 a second;
// `forceDrawMs(ms)` hands in a fixed draw time for the checks. The `Detail` pref: 'auto' is all of
// the above, 'high' pins the top (no step down, 60), 'low' pins the whole ladder (30).
//
//   createBudget({ coarse, start, detail, device })   `size(w, h, density)`, `device(id)` (the GPU,
//                                     once known), `offer(frameMs)` (a frame the display offered),
//                                     `sample({ ms, gpuMs, seconds, full })` (true when the render
//                                     scale changed), `probe()`, `pause()`, `forceDrawMs(ms)`,
//                                     `setDetail(detail)`, `hold(step, fps)` (the checks and the
//                                     measure: a step held); reads the levels (`sky`, `water`,
//                                     `det`, `rings`, `flora`, `back`, `q`, `post`, `bloom2`,
//                                     `grit`, `hero`, `scale`), `capped`, `target`, `step`,
//                                     `steps`, `tier`, `held`, `levels`
//   baseScale(w, h, density)          the backing pixels per CSS pixel before any step
//   paceUnder({ asked, capped })      the least ms between draws once the cap is counted
//   ladderOf(base, coarse)            the steps for a base scale: `[{ what, value }]`
//   GROUPS, groupOf(what), groupOfStep(step)   the ladder's groups in order, a `what`'s, a step's
//   tierKey(device, base)             the key a saved tier carries
//   rateFloors(hz)                    the draws a second under which 60 and 30 are not held on a display

/** The `Detail` pref's values: the budget (auto), the top tier held, the lowest tier held. */
export const DETAILS = ['auto', 'high', 'low'];

/** The budget's numbers. */
export const BUDGET = Object.freeze({
  /** The most backing pixels per CSS pixel, and the most backing megapixels. */
  density: 1.75,
  megapixels: 3.7,
  /** The backdrop's detail in two rounds (each a value per part: the sky's, the water's, the land's
   * detail normals, the land's rings, the trees' density), and the backdrop's share of the
   * resolution after each round, down to its floor (a phone's is higher). */
  rounds: Object.freeze([
    Object.freeze({
      sky: 0.5, water: 0.5, det: 0.5, rings: 0.75, flora: 0.7,
      back: Object.freeze([0.9, 0.82, 0.75]),
    }),
    Object.freeze({
      sky: 0, water: 0, det: 0, rings: 0.55, flora: 0.5,
      back: Object.freeze([0.67, 0.6, 0.55, 0.5]),
    }),
  ]),
  /** The land's last ring step, after the second round's backdrop share (the least density the
   * land keeps). */
  ringsLast: 0.4,
  phoneBackFloor: 0.6,
  /** The least backing pixels per CSS pixel the backdrop is drawn at (the reference build's floor):
   * the frame's last resort raises the backdrop's share to keep it. */
  backLeast: 0.3,
  /** The particles' shares, the post chain's bloom share, the hero's particle shares. */
  q: Object.freeze([0.7, 0.5, 0.3]),
  post: Object.freeze([0.5]),
  hero: Object.freeze([0.6, 0.35]),
  /** The whole frame's resolution, the last resort: down from the base by `scaleStep` to the
   * reference build's 0.3 (a phone: 0.75 only). */
  scaleStep: 0.82,
  scaleLast: 0.3,
  phoneScaleLast: Object.freeze([0.75]),
  /** The GPU (or CPU) ms a frame may cost to hold 60 and to hold 30; a frame under `calm` of its
   * limit is comfortable. */
  limit60Ms: 15,
  limit30Ms: 27,
  calm: 0.6,
  /** A cost over this many times its limit takes two steps down at once. */
  farOver: 2,
  /** The share of the draws a second the display can give (at 60, or at the cap's pace) under
   * which the rate alone says the target is not held. */
  floorShare: 0.92,
  /** How long the draws are read before a verdict, seconds (a probe reads half of it), and the
   * least time between two verdicts. */
  windowS: 1,
  evalS: 0.03,
  /** How many steps the chase of 60 (or of 30) on the rate alone takes before it asks whether they
   * raised the rate (the ladder's steps are small: one or two of the backdrop's details may change
   * the rate by less than a draw a second), and the draws a second they must add for the chase to
   * be worth going on. */
  chaseSteps: 5,
  chaseGain: 4,
  /** How long a rate must stay low before it counts, seconds. */
  lowForS: 1,
  /** How long comfortable it stays before a step back up, seconds. */
  upAfterS: 12,
  /** How long a step that was given up is not tried again, seconds, and how long a cost read at a
   * step is evidence of what it costs now. */
  retryStepS: 300,
  costKeepS: 120,
  /** How long a step back up is on trial, seconds, how long the way up then shuts, and the most
   * that doubles to. */
  trialS: 5,
  heldS: 60,
  heldMaxS: 480,
  /** How long a rate (or a time) must stay over before a step of the effects or the hero (the
   * bloom's second octave, the grit, the hero's particles, the frame's resolution) or 60 itself is
   * given up, seconds: one hitch (a seek's tile fill) is not the screen's limit. */
  lowBottomS: 6,
  lowLookS: 4,
  /** How long 60 stays given up after the quiet steps did not hold it (or a trial of it failed),
   * seconds, doubling each time to the most; then it is tried again at the end of the quiet steps,
   * on trial. */
  retry60S: 60,
  retry60MaxS: 480,
  /** A bottom that read over this many times the 60 limit, lately (seconds), is not tried again:
   * the trial would be a blip for nothing. */
  retryOverX: 2,
  retryCostS: 600,
  /** How many trials of 60 may fail in a row before 60 is given up until the page is loaded again
   * (the user, 11-27: "only happen twice before giving up and requiring a reload"); 60 holding for
   * `held60S` starts the count again. */
  retry60Tries: 2,
  /** How long 60 must hold for the next give-up to wait the first time again, seconds. */
  held60S: 30,
  /** The cap's draws a second (the 30 target's pace). */
  capFps: 30,
  /** How long a probe reads fast, seconds. */
  probeS: 2,
  /** The kept tier's format version, and the most steps it may name. */
  version: 4,
  maxStep: 40,
});

/** The slack a comparison of two clock readings gets: a clock that is a sum of steps is not
 * exact. */
const EPS = 1e-9;
/** The most draws (and offered frames) a window keeps: a second at 240 Hz fits. */
const CAP = 512;
/** The refresh rates under 60 a display may have (the rest is read as 60 or more). */
const SLOW_DISPLAYS = Object.freeze([24, 25, 48, 50]);

/** The least ms between draws: `asked`, held to the cap's pace while `capped`. */
export function paceUnder({ asked, capped }) {
  return capped ? Math.max(asked, 1000 / BUDGET.capFps) : asked;
}

/** The backing pixels per CSS pixel of a canvas `width` by `height` CSS px on a screen of density
 * `density`, before any step: at most `BUDGET.density` and `BUDGET.megapixels`. */
export function baseScale(width, height, density) {
  const area = Math.max(1, width * height);
  return Math.min(BUDGET.density, density, Math.sqrt((BUDGET.megapixels * 1e6) / area));
}

/** The draws a second the 30 cap gives on a display of `hz`: the loop keeps a frame once half a
 * frame's time before the cap's, so 60 gives 30, 50 gives 25 and 144 gives about 29. Where it
 * could fall either way (75, 165) the slower is taken. */
function capRate(hz) {
  const per = 1000 / hz;
  const x = 1000 / BUDGET.capFps / per - 0.5;
  let m = Math.max(1, Math.ceil(x - 1e-6));
  if (x > 0.5 && Math.abs(x - Math.round(x)) < 0.15) m += 1;
  return hz / m;
}

/** The draws a second under which 60 and 30 are not held on a display of `hz`. */
export function rateFloors(hz) {
  return {
    f60: BUDGET.floorShare * Math.min(hz, 60),
    f30: BUDGET.floorShare * capRate(hz),
  };
}

/** The key a saved tier carries: the tier and GPU (`device`) and the size class (the base scale in
 * 0.05 steps), the things a step number depends on. */
export function tierKey(device, base) {
  return `${device ?? ''}|${Math.round(base * 20)}`;
}

/** The ladder's groups, in the order they are given up: the backdrop, the particles, the post
 * chain's cost, the effects, the hero (its particles, then the frame's resolution). */
export const GROUPS = Object.freeze(['backdrop', 'particles', 'post', 'effects', 'hero']);

const GROUP_OF = Object.freeze({
  sky: 'backdrop', water: 'backdrop', det: 'backdrop', rings: 'backdrop', flora: 'backdrop',
  back: 'backdrop', q: 'particles', post: 'post', bloom2: 'effects', grit: 'effects', hero: 'hero',
  scale: 'hero',
});

/** The group of a step's `what`. */
export const groupOf = (what) => GROUP_OF[what];

/** The group of a step: its own (the frame's density above 1.0 CSS px is the backdrop's), else its
 * `what`'s. */
export const groupOfStep = (step) => step?.group ?? groupOf(step?.what);

/** The steps of the ladder for a base scale `base`, in order: `{ what, value }`, `what` one of
 * `sky`, `water`, `det`, `rings`, `flora`, `back` (the backdrop's share of the resolution), `q`
 * (the particles), `post` (the bloom's share), `bloom2`, `grit`, `hero` (the hero's particles),
 * `scale` (the whole frame's backing pixels per CSS pixel). The backdrop's two rounds come first (each the
 * sky, the water, the land, the trees, then the backdrop's share), then the particles, the post
 * chain's cost, the effects, the hero's particles, and the frame's resolution last of all. A phone
 * (`coarse`) keeps a backdrop share of 0.6 at least and has 0.75 for its frame's last resort. */
export function ladderOf(base, coarse = false) {
  const out = [];
  const backFloor = coarse ? BUDGET.phoneBackFloor : 0;
  // a frame denser than 1.0 CSS px (a hi-DPI screen) gives the density above 1.0 up with the
  // backdrop's first round (hardly seen on such a screen), by `scaleStep` to 1.0
  const dense = [];
  for (let scale = base; !coarse && scale > 1 + EPS;) {
    scale = Math.max(1, scale * BUDGET.scaleStep);
    if (scale < 1.05) scale = 1;
    dense.push({ what: 'scale', value: scale, group: 'backdrop' });
  }
  BUDGET.rounds.forEach((round, i) => {
    for (const what of ['sky', 'water', 'det', 'rings', 'flora']) {
      out.push({ what, value: round[what] });
    }
    if (i === 0) out.push(...dense);
    for (const value of round.back) if (value >= backFloor - EPS) out.push({ what: 'back', value });
  });
  out.push({ what: 'rings', value: BUDGET.ringsLast });
  if (coarse && !out.some((s) => s.what === 'back' && Math.abs(s.value - backFloor) < EPS)) {
    out.push({ what: 'back', value: backFloor });
  }
  for (const value of BUDGET.q) out.push({ what: 'q', value });
  for (const value of BUDGET.post) out.push({ what: 'post', value });
  out.push({ what: 'bloom2', value: 0 }, { what: 'grit', value: 0 });
  for (const value of BUDGET.hero) out.push({ what: 'hero', value });
  if (coarse) {
    for (const value of BUDGET.phoneScaleLast) out.push({ what: 'scale', value });
  } else {
    // down by `scaleStep` to the last resort, which ends it (a step that would land within 5 % of
    // it is the last resort itself)
    let scale = Math.min(base, 1);
    while (scale > BUDGET.scaleLast + EPS) {
      scale *= BUDGET.scaleStep;
      if (scale < BUDGET.scaleLast * 1.05) scale = BUDGET.scaleLast;
      out.push({ what: 'scale', value: scale });
    }
  }
  return out;
}

/** The levels at the top of the ladder for a base scale `base`. */
const topLevels = (base) => ({
  sky: 1, water: 1, det: 1, rings: 1, flora: 1, back: 1, q: 1, post: 1, bloom2: true, grit: true,
  hero: 1, scale: base,
});

/** Fold step `s` into the levels `lv`. */
function apply(lv, s) {
  if (s.what === 'bloom2' || s.what === 'grit') lv[s.what] = false;
  else lv[s.what] = Math.min(lv[s.what], s.value);
}

/** Whether the levels `lv` have given step `s` up already. */
function taken(lv, s) {
  if (s.what === 'bloom2' || s.what === 'grit') return !lv[s.what];
  return lv[s.what] <= s.value + EPS;
}

/** The step count of ladder `lad` that leaves what the levels `lvl` leave: every step from the top
 * that they have given up already, or, when the frame's resolution was given up, the nearest of its
 * steps. */
function stepOfLevel(lad, lvl) {
  if (lvl.scaled) {
    let best = -1;
    lad.forEach((s, i) => {
      if (s.what === 'scale'
        && (best < 0 || Math.abs(s.value - lvl.scale) < Math.abs(lad[best].value - lvl.scale))) {
        best = i;
      }
    });
    if (best >= 0) return best + 1;
  }
  let n = 0;
  while (n < lad.length && lad[n].what !== 'scale' && taken(lvl, lad[n])) n++;
  return n;
}

/**
 * The step-down for a scene. `coarse` (a finger for a pointer: a phone) aims at 30 and starts one
 * step down; `start` (`{ v, key, step, fps }`, the kept tier) is where a viewer's next visit
 * begins when its key is the one of this tier, GPU (`device`: an id, or null until `device(id)`
 * says it) and size class; otherwise the visit starts at the top with a probe. `detail` ('auto',
 * 'high', 'low') pins the top or the whole ladder: the start and the draw times then change
 * nothing.
 */
export function createBudget({
  coarse = false, start = null, detail = 'auto', device = '',
} = {}) {
  let base = coarse ? 1 : BUDGET.density;
  let dev = device;
  let sized = false;
  let pending = start?.v === BUDGET.version && typeof start.key === 'string'
    && Number.isInteger(start.step) && start.step >= 0 && start.step <= BUDGET.maxStep ? start : null;
  let n = coarse ? 1 : 0;
  let target = coarse ? 30 : 60;
  let forced = null;
  let mode = DETAILS.includes(detail) ? detail : 'auto';
  // the draws of the last window: when, CPU ms, GPU ms (NaN: none), a ring each
  const winAt = new Float64Array(CAP);
  const winCpu = new Float64Array(CAP);
  const winGpu = new Float64Array(CAP);
  let winHead = 0;
  let winLen = 0;
  // the clock of the draws of the last second (the rate), and the frames the display offered
  const stampAt = new Float64Array(CAP);
  let stampHead = 0;
  let stampLen = 0;
  const offered = new Float64Array(128);
  let offeredHead = 0;
  let offeredLen = 0;
  const scratch = new Float64Array(CAP);
  let hz = 60;
  let hzAt = null;
  let evalAt = null;
  let freshAt = null;
  let calmSince = null;
  let lowSince = null;
  let overSince = null;
  let blockedUntil = -Infinity;
  let blockFor = BUDGET.heldS;
  let giveUp60Until = -Infinity;
  let giveUpFor = BUDGET.retry60S;
  let failed60 = 0;
  let rateOffUntil = -Infinity;
  let at60Since = null;
  /** A step up on trial, or the retry of 60: `{ from, fps, until }`. */
  let trial = null;
  /** The chase of 60 by the rate alone: `{ rate, steps }` at its start and its steps so far. */
  let chase = null;
  /** The same at 30: `{ rate, steps, from }`, the step it began at. */
  let chase30 = null;
  let probing = start !== null;
  let probeEnd = null;
  /** When each step was last given up (the clock), so a step up is not tried again soon. */
  let failedAt = [];
  /** The ms a frame cost at each step the budget has stood on, and when it was read. */
  let costs = [];
  let costAt = [];
  let ladder = ladderOf(base, coarse);
  let lv = null;

  const total = () => ladder.length;
  /** Whether the next step down takes the effects or the hero (the steps that wait for a lasting
   * slowness). */
  const lookNext = () => {
    const group = groupOfStep(ladder[n]);
    return group === 'effects' || group === 'hero';
  };
  /** The steps spent chasing 60: the backdrop's. */
  const quiet = () => {
    const at = ladder.findIndex((s) => groupOfStep(s) !== 'backdrop');
    return at < 0 ? ladder.length : at;
  };

  /** What the first `n` steps leave (kept until the step changes). */
  function level() {
    if (lv) return lv;
    lv = topLevels(base);
    const upto = Math.min(n, total());
    for (let i = 0; i < upto; i++) apply(lv, ladder[i]);
    // the backdrop never draws under `backLeast` px per CSS px: as the frame's last resort goes
    // under 1.0, the backdrop's share of it rises back
    if (lv.scale < 1 - EPS) lv.back = Math.max(lv.back, Math.min(1, BUDGET.backLeast / lv.scale));
    return lv;
  }

  /** A pinned level's step: the top is 0 at 60, the lowest is the last at 30. */
  function pin() {
    if (mode === 'high') {
      n = 0;
      target = 60;
    } else if (mode === 'low') {
      n = total();
      target = 30;
    }
    lv = null;
  }
  pin();

  /** The kept tier applies once the size class and the GPU are both known. */
  function tryStart() {
    if (!pending || !sized || dev === null) return;
    const kept = pending;
    pending = null;
    if (kept.key !== tierKey(dev, base)) return;
    n = Math.min(kept.step, total());
    target = coarse || kept.fps === 30 ? 30 : 60;
    pin();
  }

  const popWindow = () => {
    winHead = (winHead + 1) % CAP;
    winLen--;
  };
  const popStamp = () => {
    stampHead = (stampHead + 1) % CAP;
    stampLen--;
  };

  /** The middle of `len` values of ring `from` (NaN is no value): one hitch (a shader compiling,
   * a seek's fill) is not a slow second. Null when none. */
  function middleOf(from, head, len) {
    let count = 0;
    for (let i = 0; i < len; i++) {
      const v = from[(head + i) % from.length];
      if (v === v) {
        let j = count++;
        while (j > 0 && scratch[j - 1] > v) {
          scratch[j] = scratch[j - 1];
          j--;
        }
        scratch[j] = v;
      }
    }
    return count ? scratch[count >> 1] : null;
  }

  /** The display's refresh rate, from the frames it offered (once a second): 24, 25, 48 or 50
   * where they come steadily at that rate, 60 or more otherwise. */
  function displayHz(seconds) {
    if (hzAt !== null && seconds - hzAt < 1) return hz;
    hzAt = seconds;
    hz = 60;
    if (offeredLen < 24) return hz;
    const med = middleOf(offered, offeredHead, offeredLen);
    let steady = 0;
    for (let i = 0; i < offeredLen; i++) {
      if (Math.abs(offered[(offeredHead + i) % offered.length] - med) <= 0.1 * med) steady++;
    }
    if (steady < 0.7 * offeredLen) return hz;
    const rate = 1000 / med;
    if (rate > 62) hz = Math.round(rate);
    for (let i = 0; i < SLOW_DISPLAYS.length; i++) {
      if (Math.abs(rate - SLOW_DISPLAYS[i]) <= 0.04 * SLOW_DISPLAYS[i]) hz = SLOW_DISPLAYS[i];
    }
    return hz;
  }

  const limit = () => (target === 60 ? BUDGET.limit60Ms : BUDGET.limit30Ms);

  /** How long a verdict to give up 60 or the look must last: none for another step. */
  function holdFor() {
    if (target === 60 && n >= quiet()) return trial && trial.fps === 30 ? BUDGET.lowForS : BUDGET.lowBottomS;
    return lookNext() ? BUDGET.lowLookS : BUDGET.lowForS;
  }

  function reset(seconds) {
    winHead = 0;
    winLen = 0;
    stampHead = 0;
    stampLen = 0;
    evalAt = null;
    calmSince = null;
    lowSince = null;
    overSince = null;
    freshAt = seconds;
  }

  /** Move to step `to` and/or target `fps`; whether the render scale changed. */
  function move(seconds, to, fps = target) {
    const was = api.scale;
    reset(seconds);
    if (probing) probeEnd = seconds + BUDGET.probeS;
    n = Math.max(0, Math.min(total(), to));
    target = fps;
    lv = null;
    return api.scale !== was;
  }

  /** The least step whose cost, read lately, holds 30 with room, or `from` when none was. */
  function stepFor30(from, seconds) {
    const at = costs.findIndex((c, i) => i <= from && c !== null && c !== undefined
      && seconds - costAt[i] <= BUDGET.costKeepS && c <= 0.8 * BUDGET.limit30Ms);
    return at < 0 ? from : at;
  }

  /** 60 is given up: the target is 30 from the least step that holds it, and 60 is tried again
   * after a while (longer each time). */
  function giveUp60(seconds, to = stepFor30(n, seconds)) {
    giveUp60Until = seconds + giveUpFor;
    giveUpFor = Math.min(BUDGET.retry60MaxS, giveUpFor * 2);
    chase = null;
    at60Since = null;
    return move(seconds, to, 30);
  }

  /** The target is not held at `cost`: one step down (two when far over), or, at the end of the
   * quiet steps while chasing 60, the target 30. */
  function lower(seconds, cost) {
    costs[n] = cost;
    costAt[n] = seconds;
    if (trial && seconds <= trial.until) {
      const { from, fps } = trial;
      trial = null;
      // the retry of 60 was too much: back to 30 at once, and the next waits longer
      if (fps !== target) {
        failed60 += 1;
        return giveUp60(seconds, from);
      }
      // the step up on trial was too much: back, and the way up shuts for longer
      blockedUntil = seconds + blockFor;
      blockFor = Math.min(BUDGET.heldMaxS, blockFor * 2);
      return move(seconds, from, fps);
    }
    trial = null;
    failedAt[n] = seconds;
    if (target === 60 && n >= quiet()) return giveUp60(seconds);
    const far = cost > BUDGET.farOver * limit() && n + 1 < (target === 60 ? quiet() : total());
    return move(seconds, n + (far ? 2 : 1));
  }

  /** One step up, on trial; whether the render scale changed. */
  function raise(seconds) {
    trial = { from: n, fps: target, until: seconds + BUDGET.trialS };
    return move(seconds, n - 1);
  }

  const api = {
    /** The canvas's CSS size and the screen's density, once known and at each resize: the levels
     * stay what they mean on the ladder of the new base. */
    size(width, height, density) {
      const next = coarse ? 1 : baseScale(width, height, density);
      if (sized && Math.abs(next - base) < 1e-9) return;
      const was = sized ? { ...level(), scaled: level().scale < base - EPS } : null;
      const lad = ladderOf(next, coarse);
      if (was) n = stepOfLevel(lad, was);
      base = next;
      ladder = lad;
      costs = [];
      costAt = [];
      failedAt = [];
      trial = null;
      chase = null;
      chase30 = null;
      lv = null;
      sized = true;
      if (was) reset(null);
      tryStart();
      n = Math.min(n, total());
      pin();
    },

    /** The GPU's id (the tier and the adapter or renderer), once the renderer says it. */
    device(id) {
      dev = String(id ?? '');
      tryStart();
    },

    /** A frame the display offered, `frameMs` after the one before (drawn or not): the display's
     * refresh rate is read from them. */
    offer(frameMs) {
      offered[(offeredHead + offeredLen) % offered.length] = frameMs;
      if (offeredLen < offered.length) offeredLen++;
      else offeredHead = (offeredHead + 1) % offered.length;
    },

    /** The `Detail` pref's value; true when the render scale changed. */
    setDetail(next) {
      const was = api.scale;
      const want = DETAILS.includes(next) ? next : 'auto';
      // the pref is read again and again: only a change of it ends a held step
      if (want === mode || (mode === 'hold' && want === 'auto')) return false;
      mode = want;
      reset(null);
      pin();
      return api.scale !== was;
    },
    get detail() { return mode === 'hold' ? 'auto' : mode; },
    /** Whether a step is held for the checks (what is held is not what this screen settled on). */
    get held() { return mode === 'hold'; },

    /** The sky's, the water's and the land's detail (1, 0.5, 0), the share of the land's ring
     * cells and of the trees that is drawn. */
    get sky() { return level().sky; },
    get water() { return level().water; },
    get det() { return level().det; },
    get rings() { return level().rings; },
    get flora() { return level().flora; },
    /** The backdrop's share of the frame's resolution. */
    get back() { return level().back; },
    /** The share of the particle count that is drawn. */
    get q() { return level().q; },
    /** The post chain's bloom share of its resolution. */
    get post() { return level().post; },
    get grit() { return level().grit; },
    get bloom2() { return level().bloom2; },
    /** The share of the hero's particles that is drawn. */
    get hero() { return level().hero; },
    /** The backing pixels per CSS pixel of the frame now. */
    get scale() { return level().scale; },
    /** Every level, as one object (the budget's own: read it, do not keep or change it). */
    get levels() { return level(); },
    /** Whether the draws are paced to 30 a second (the target is 30). */
    get capped() { return target === 30; },
    get target() { return target; },
    get step() { return n; },
    /** How many steps the ladder has at this size. */
    get steps() { return total(); },
    /** What a viewer's next visit starts from. */
    get tier() {
      return {
        v: BUDGET.version, key: tierKey(dev, base), step: n, fps: target,
      };
    },

    /** The scene is about to draw every frame (Full is chosen): read fast. */
    probe() {
      probing = true;
      probeEnd = null;
      reset(null);
    },

    /** For the checks and the measure: step `step` held, at `fps` (30 or 60), until the Detail pref
     * changes. */
    hold(step, fps = 60) {
      const was = api.scale;
      mode = 'hold';
      n = Math.max(0, Math.min(total(), step));
      target = fps === 30 ? 30 : 60;
      lv = null;
      reset(null);
      return api.scale !== was;
    },

    /** The draws are `ms` of CPU (`gpuMs` of GPU time, or null where none is read) at `seconds` on
     * the clock handed in; `full` is whether the scene asks for every frame. Returns true when the
     * render scale changed (the caller sizes its targets again). A forced draw time stands in for
     * both. */
    sample({
      ms, gpuMs = null, seconds, full = false,
    }) {
      if (mode !== 'auto') {
        const was = api.scale;
        pin();
        return api.scale !== was;
      }
      const cpu = forced ?? ms;
      const gpu = forced ?? gpuMs;
      freshAt ??= seconds;
      stampAt[(stampHead + stampLen) % CAP] = seconds;
      if (stampLen < CAP) stampLen++;
      else popStamp();
      while (stampLen && stampAt[stampHead] < seconds - 1) popStamp();
      const slot = (winHead + winLen) % CAP;
      winAt[slot] = seconds;
      winCpu[slot] = cpu;
      winGpu[slot] = gpu === null || gpu === undefined ? NaN : gpu;
      if (winLen < CAP) winLen++;
      else popWindow();
      const windowS = probing ? BUDGET.windowS / 2 : BUDGET.windowS;
      while (winLen && winAt[winHead] < seconds - windowS) popWindow();
      if (probing) {
        if (probeEnd === null) probeEnd = seconds + BUDGET.probeS;
        if (seconds > probeEnd) probing = false;
      }
      // nothing is decided before a window has been read, and the first draw after it is weighed at
      // once (a verdict is not late by the throttle, so a long ladder keeps the same times at any
      // rate); then the verdicts are worked out at most every `evalS`: the medians are not worth a
      // pass a draw
      if (seconds - freshAt < windowS - EPS) return false;
      if (evalAt !== null && seconds - evalAt < BUDGET.evalS - EPS) return false;
      evalAt = seconds;
      const cpuMid = middleOf(winCpu, winHead, winLen);
      const gpuMid = middleOf(winGpu, winHead, winLen);
      const cost = Math.max(cpuMid ?? 0, gpuMid ?? 0);
      // the rate alone: a read of the draws a second, while the scene asks for every frame and
      // a draw time is not forced (it is the real clock's, not the forced one's); the floor is
      // what the display can give
      const floors = rateFloors(displayHz(seconds));
      const minFps = target === 60 ? floors.f60 : floors.f30;
      const slowRate = forced === null && full && seconds - freshAt >= 1 - EPS
        && seconds >= rateOffUntil && stampLen < minFps;
      if (slowRate) lowSince ??= seconds;
      else lowSince = null;
      const lowFor = holdFor();
      if (lowSince !== null && seconds - lowSince >= lowFor - EPS) {
        lowSince = null;
        if (target === 30) {
          // at 30 the rate is the verdict too (the times leave out the compositor's share): a step
          // down, however the GPU time reads; `chaseSteps` that do not raise it are not the way:
          // the draws are as many as the display gives, and the budget goes back to where it began
          chase30 ??= { rate: stampLen, steps: 0, from: n };
          if (chase30.steps >= BUDGET.chaseSteps && stampLen < chase30.rate + BUDGET.chaseGain) {
            const back = chase30.from;
            chase30 = null;
            rateOffUntil = seconds + BUDGET.retryStepS;
            return move(seconds, back, 30);
          }
          chase30.steps++;
          return lower(seconds, cost);
        }
        // the draws do not come at 60, whatever the times say: the quiet steps are spent until they
        // do. When `chaseSteps` of them have not raised the rate the screen cannot show 60, and the
        // budget aims at 30 from the least step that holds it.
        chase ??= { rate: stampLen, steps: 0 };
        if (chase.steps >= BUDGET.chaseSteps && stampLen < chase.rate + BUDGET.chaseGain) {
          return giveUp60(seconds);
        }
        chase.steps++;
        return lower(seconds, cost);
      }
      if (seconds - freshAt < windowS - EPS) return false;
      if (cost > limit()) {
        // a second over is a hitch where the step would give up 60 or take the look: that waits
        // for it to last, unless it is far over
        const wait = holdFor();
        if (cost <= BUDGET.farOver * limit() && wait > BUDGET.lowForS) {
          overSince ??= seconds;
          if (seconds - overSince < wait - EPS) return false;
        }
        return lower(seconds, cost);
      }
      overSince = null;
      costs[n] = cost;
      costAt[n] = seconds;
      if (trial && seconds > trial.until) {
        trial = null;
        blockFor = BUDGET.heldS;
      }
      if (!slowRate) {
        chase = null;
        chase30 = null;
      }
      if (target === 60) {
        at60Since ??= seconds;
        if (seconds - at60Since >= BUDGET.held60S) {
          giveUpFor = BUDGET.retry60S;
          failed60 = 0;
        }
      }
      // 30 is not for good: once its wait is over and the picture has been calm for a spell, 60 is
      // tried at the end of the quiet steps, on trial (a failed one goes back to 30 at once)
      if (target === 30 && !coarse && !trial && failed60 < BUDGET.retry60Tries && seconds >= giveUp60Until
          && seconds - freshAt >= BUDGET.upAfterS - EPS) {
        const bottom = quiet();
        const read = costs[bottom];
        const hopeless = read !== undefined && read !== null
          && seconds - costAt[bottom] <= BUDGET.retryCostS
          && read > BUDGET.retryOverX * BUDGET.limit60Ms;
        if (!hopeless) {
          at60Since = null;
          trial = { from: n, fps: 30, until: seconds + BUDGET.trialS + BUDGET.lowForS };
          return move(seconds, bottom, 60);
        }
      }
      if (cost >= BUDGET.calm * limit()) {
        calmSince = null;
        return false;
      }
      calmSince ??= seconds;
      // the step above was given up not long ago: it is not tried again yet
      const failed = n > 0 && seconds - (failedAt[n - 1] ?? -Infinity) < BUDGET.retryStepS;
      if (seconds - calmSince < BUDGET.upAfterS - EPS || seconds < blockedUntil - EPS || n === 0
          || failed) {
        if (n === 0) calmSince = null;
        return false;
      }
      return raise(seconds);
    },

    /** The scene stopped drawing (the page is hidden): what was read is no evidence of what
     * comes. A slow draw is not a pause: draws far apart in a row are the evidence of a slow
     * scene. */
    pause() {
      reset(null);
    },

    /** For the checks: every draw counts as `ms` (null: the real times again). A forced time is
     * the only reading while it stands: the draws-a-second rule is off, and a 60 or a step given
     * up is open to try again. */
    forceDrawMs(ms) {
      forced = Number.isFinite(ms) ? ms : null;
      reset(null);
      giveUp60Until = -Infinity;
      rateOffUntil = -Infinity;
      failedAt = [];
    },
  };
  return api;
}
