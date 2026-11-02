/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The smoke's rules that need no GPU: the look's constants, the grids' size, the resting source's
// strength, the easing, the frame rhythm, the shade drawn per page and the haze's sources.
// smoke.js draws; this file decides.

/** Every look constant of the smoke, in one place (the look is retuned here and nowhere else). */
export const TUNING = Object.freeze({
  simRes: 128, // the velocity grid's cells on the canvas's short side
  dyeRes: 512, // the dye grid's cells on the canvas's short side
  pressureIterations: 20, // Jacobi passes per frame
  pressureDamp: 0.8, // the previous pressure kept as the next frame's start (factor)
  calmFps: 20, // simulated frames per second while nothing stirs
  calmSlack: 0.004, // a calm frame may come this early (s), so 60 Hz callbacks keep 20 fps
  stirredSecs: 2, // how long after a splat every animation frame is simulated (s)
  fadeSecs: 1, // the away look's grey-and-stop fade, and a level change's settle (s)
  awayVelocityLoss: 5, // velocity lost per fade length while away, so it rests (factor)
  awayOpacity: 0.25, // the drawn brightness while away, as the glow's (factor)
  dprCap: 1.5, // the canvas's backing pixels per CSS pixel, at most
  maxDt: 0.05, // the longest step the simulation takes (s)
  velocityDissipation: 0.02, // velocity lost per second (rate)
  dyeDissipation: 0.35, // dye lost per second (rate)
  curl: 10, // vorticity confinement strength (factor); kept low because a stroke's smoke
  // travels further with it
  restRadius: 0.16, // the resting source's Gaussian radius (of the canvas's short side)
  restDrift: 6, // the resting source's push at full strength, along its wandering direction
  // (cells per second); weak, so the cloud spreads from where it is made and does not plume
  wanderRate: 0.5, // how fast a source's push direction wanders (radians per square root of s)
  restPull: 3, // how fast the velocity there is pulled to the drift (per second)
  restDye: 0.05, // the dye the resting source adds at its centre (per second, full strength)
  restPickSecs: 0.5, // how long a pointer's dye keeps one of the two shades (s)
  drawGain: 25, // dye amount to opacity steepness, 1 - exp(-amount * gain) (factor)
  peakAlpha: 0.4, // the densest dye's opacity at full brightness (0 to 1)
  maxPush: 80, // the most velocity any pointer or puff force stands for (cells per second); a wild
  // flick (15 short sides a second in the browser check) must not add a runaway velocity
  pointerPush: 40, // the pointer's velocity per short side swept in a second (cells per second)
  pointerRadius: 0.04, // the pointer splat's Gaussian radius (of the canvas's short side)
  pointerAdd: 0.3, // the share of the pointer's motion added to the velocity under it, per 1/60 s
  // of strokes (an add, so a slow pointer never brakes the plume it crosses)
  pointerGapSecs: 0.1, // a pause this long between two moves starts a fresh stroke (s)
  pointerDyePerSide: 1.4, // the dye a pressed pointer leaves per short side moved (amount; by the
  // distance moved, so a pressed, motionless pointer adds none)
  pointerDyeMax: 0.05, // the most dye one pointer move leaves (amount)
  maxPointerSplats: 12, // the pointer moves kept for one frame, the newest (a cost bound)
  puffPush: 40, // a full volume puff's velocity along the dial's tangent (cells per second)
  puffFullSize: 0.1, // the volume step that gives a full puff (of the dial's whole range)
  puffFloor: 0.35, // the share of a puff left at level 0 (the muted dial's reading)
  puffDye: 0.04, // a full puff's dye (amount)
  puffRadius: 0.06, // the puff's Gaussian radius (of the canvas's short side)
  puffAdd: 0.5, // the share of a puff's force added to the velocity at the dot (once per puff)
  maxPuffs: 8, // the volume puffs kept for one frame, the newest (a glide stays a stream)
  meterRed: 0.93, // a meter's fraction from which it is red (the server's RED_FROM)
  meterDye: 0.12, // a full meter's dye emission (amount per second)
  meterDrift: 5, // a full meter's push along its wandering direction (cells per second)
  meterRedKick: 10, // the extra push of a meter in its red zone (cells per second)
  meterRadius: 0.05, // the meter emission's Gaussian radius (of the canvas's short side)
  meterPull: 3, // how fast the velocity at a meter is pulled to its drift (per second)
  meterEaseSecs: 0.25, // how long a meter's reading takes to be shown in the smoke (s)
  meterFloor: 0.01, // an eased reading under this emits nothing (fraction)
  scrollPush: 15, // the scroll's velocity per short side scrolled in a second (cells per second;
  // gentler than the pointer's 40, so a wheel notch stirs without a flick's force)
  maxScrollPush: 30, // the most velocity a scroll stands for (cells per second); a phone's
  // momentum fling or a held key stays bounded
  scrollEaseSecs: 0.15, // how long the scroll's speed takes to follow the page's (s), so a
  // wheel's bursts and a momentum fling push steadily
  scrollFloor: 0.05, // the eased scroll speed under which the stir ends (short sides a second), so
  // a settled scroll stops stirring and the rhythm returns to calm
  scrollSplatRows: 3, // the scroll's broad splats down the visible page (count)
  scrollSplatCols: 2, // the scroll's broad splats across the visible page (count)
  scrollRadius: 0.3, // a scroll splat's Gaussian radius (of the canvas's short side)
  scrollPull: 8, // how fast the velocity under a scroll splat is pulled to the push (per second)
  agitScroll: 1, // agitation per short side a second of scroll speed above the floor, per second
  agitScrollFloor: 1, // the scroll speed under which nothing is added (short sides a second)
  agitDecaySecs: 0.5, // the agitation's decay time constant (s)
  agitPointer: 3.5, // agitation per short side a second of pointer speed above the floor, per
  // second of motion: an ordinary move of 2 short sides a second for 0.5 s reaches 1.5, a flick of
  // 5 for 0.2 s reaches 2.2 and 8 or more for 0.15 s 3 (the speed here is the pointer's own, not
  // the capped push)
  agitSpeedFloor: 0.6, // the pointer speed under which nothing is added (short sides a second; a
  // slow drift of 1 for 0.5 s adds 0.44, so it never arms a settle, and a move of 2 does)
  agitSpeedMax: 8, // the pointer speed above which a flick adds no more (short sides a second)
  agitStep: 6, // agitation per share of the dial's range a volume step moves
  agitMeter: 10, // agitation per second of the loudest meter at the top of its scale (scaled by
  // its excess over red, so a meter just past red adds almost none): a 0.15 s peak to 0.99
  // reaches 1.1, a reading held at 0.95 for half a second stays under 1
  settleFrom: 1, // the agitation a stir must pass to count as one a note may follow
  settleBelow: 0.25, // the agitation under which that stir counts as settled
  settleSecs: 0.8, // the least time between the rise and its settle (s)
  settleOdds: 1 / 3, // the chance a settled stir makes a note (share)
  clickOdds: 1 / 2, // the chance a settled control click makes a note (share)
  bigRiseOdds: 2 / 3, // the chance a settled big volume rise makes a note (share)
  clickAgit: 1.5, // the agitation of a control's click
  agitStepMin: 1.5, // the least agitation of a volume step, so one step arms a settle
  burstGapSecs: 1, // a pause this long ends a burst of volume steps (s)
  bigRiseDb: 10, // a burst's rise over this many dB is a big one (dB)
  bigRiseShare: 0.125, // a dial without dB: a burst's rise over this share of its range is big
  clickPuffPush: 12, // a click's puff, along a random direction (cells per second)
  clickPuffDye: 0.015, // a click's puff's dye (amount)
  beatPuffPush: 16, // a full music beat's puff, outward from the dial (cells per second)
  beatPuffDye: 0.02, // a full music beat's puff's dye (amount)
  noteCooldown: 4, // the least time between two notes (s)
  noteLife: 3, // how long a stamped note counts as visible (s)
  noteCap: 2, // the most notes visible at once
  noteTiltDeg: 25, // a note's tilt, either way (degrees)
  noteSizeMin: 0.06, // a note's smallest side (of the canvas's short side)
  noteSizeMax: 0.09, // a note's largest side (of the canvas's short side)
  noteCell: 128, // one glyph's cell in the atlas (px)
  noteGlyphHeight: 0.64, // a glyph's visible height as a share of its cell (all three)
  noteHaloBlur: 0.07, // the halo's blur in the atlas (of the cell)
  noteHaloWidth: 0.05, // the halo's dilation: the stroke it is blurred from (of the cell)
  noteInk: 0.7, // how far the dye's colour goes toward the lighter tint inside a glyph (share)
  noteLight: 0.6, // the tint's share of white (share)
  noteOpenHalf: 0.32, // half the box a note needs open on the page, from its centre (of the cell)
  noteDenseMin: 0.005, // the least mean dye in the note's cell for a note to form there (amount;
  // the drawn opacity there is 0.12 of the ceiling, and the glyph carved out of it reads against
  // the smoke round it; the thin haze behind the column holds 0.001 to 0.002, a touch flick's
  // own dye 0.007 to 0.01)
  noteFindStep: 0.5, // the lattice step of the search for the nearest dense, open spot (of the
  // cell)
  noteCarve: 0.85, // the share of the dye removed inside a glyph, the gap the note reads by (share)
  noteHalo: 0.25, // the share of the dye removed at the halo's strongest, round the glyph (share)
  hazeSpread: 0.0016, // the steady dye density of a spread source at its centre (amount)
  hazeSide: 0.024, // the steady dye density of a side-bar source at its centre (amount)
  hazeReach: 0.6, // how far into a band a side source may go (of the band's width)
  hazeInner: 0.2, // a side source's strength where the band meets the content (share of its edge
  // strength; the gradient falls linearly between the window's edge and there)
  hazeSideCount: 3, // the sources in each side band (count)
  hazeSpreadCount: 3, // the sources spread over the whole viewport (count)
  hazeSideRadius: 0.14, // a side source's Gaussian radius (of the canvas's short side)
  hazeSpreadRadius: 0.3, // a spread source's Gaussian radius (of the canvas's mean side, the
  // square root of width times height, so the coverage is the same on a phone and a desktop)
  hazeEdge: 0.12, // the side bands' width when there are no margins beside the content, and the
  // narrowest margin that counts as one (of the canvas's width)
  hazeEdgeGain: 0.4, // the side bands' strength where there are no margins (share of a margin's)
  hazeWander: 0.2, // how far a side source drifts along its band, either way (of the band's width)
  hazeSlotWander: 0.35, // how far a side source drifts up and down its own slot of the height,
  // either way (of the slot's height, the canvas's height over `hazeSideCount`)
  hazeSpreadMargin: 0.1, // the nearest a spread source's centre comes to the canvas's side (of
  // the canvas's width; a narrow canvas lets them roam its whole width)
  hazeWanderSecs: 90, // the time a source's drift takes to go round once (s, typical; each differs)
  hazeMaxStep: 0.5, // the longest time a feeding stands for (s); the haze is fed at every
  // simulated frame, in proportion to its step
  hazePush: 0.06, // the velocity a source adds each second, in its own slowly turning direction
  // (cells per second, per second)
  hazeTurnSecs: 70, // the time a source's push takes to turn once round (s, typical)
  hazePickSecs: 5, // how long a source keeps one of the two side tones (s)
  hazeFadeSecs: 1.5, // how long the dye in the content's place fades after the margins change (s)
  hazeFadeRate: 4, // that fade's loss (rate per second)
  hazeFadeSoft: 0.03, // the soft edge of that fade (of the canvas's width)
  hazeFadeShare: 0.02, // the margins' change that starts the fade (of the canvas's width)
  shadeHueLo: 205, // the palette's lowest middle hue, cyan-blue (degrees)
  shadeHueHi: 275, // the palette's highest middle hue, violet (degrees)
  shadeStepLo: 22, // the least hue distance of a side tone from the dial's tone (degrees; the
  // two side tones are twice that apart, and the dial's tone differs from each by at least
  // `shadeToneMargin`)
  shadeStepHi: 30, // the greatest hue distance of a side tone from the dial's tone (degrees)
  shadeToneMargin: 20, // the least hue distance between any two of the three tones (degrees)
  // The palette's saturation, one value for the three tones (HSL percent); node/model.mjs proves
  // the range lies in the look round's band.
  shadeSatLo: 54, // the palette's lowest saturation (percent)
  shadeSatHi: 60, // the palette's highest saturation (percent)
  shadeLightLo: 55, // the palette's lowest base lightness (percent)
  shadeLightHi: 74, // the palette's highest base lightness (percent)
  shadeGreyHue: 25, // the grey shade's hue, either side of the base (degrees)
  shadeGreyDark: 4, // how much darker the grey shade is than the base (lightness percent)
  shadeLighter: 10, // how much lighter the light tint is than the base (lightness percent)
  shadeLightMax: 82, // the light tint's greatest lightness (percent)
  shadeGreyShareLo: 0.3, // the lowest share of emissions that take the grey shade (share)
  shadeGreyShareHi: 0.7, // the highest share of emissions that take the grey shade (share)
});

/** The clamp of `x` into 0 to 1; 0 for a value that is not a number. */
function unit(x) {
  return Number.isFinite(x) ? Math.min(1, Math.max(0, x)) : 0;
}

/** The grid's cells for a canvas of `width` by `height` px with `shortSide` cells on its short
 * side: `{ w, h }` in the canvas's aspect. */
export function gridSize(width, height, shortSide) {
  const short = Math.min(width, height);
  if (!(short > 0)) return { w: 1, h: 1 };
  const scale = shortSide / short;
  return { w: Math.max(1, Math.round(width * scale)), h: Math.max(1, Math.round(height * scale)) };
}

/** The resting source's strength for the level: 0.22 + 0.78 * level, the glow's own opacity. */
export function sourceStrength(level) {
  return 0.22 + 0.78 * unit(level);
}

/** `from` moved toward `to` over `dt` seconds, exponentially: about 98 % of the way after `secs`;
 * never past `to`, and the same whatever the frame rate. */
export function ease(from, to, dt, secs) {
  if (!(secs > 0)) return to;
  if (!(dt > 0)) return from;
  return to + (from - to) * Math.exp((-4 * dt) / secs);
}

/** What the loop does at this animation frame: `'frame'` (simulate and draw), `'skip'` (wait for
 * the next callback) or `'stop'` (stop asking for frames). Times are in seconds; `fadedAt` is when
 * the away fade is over (`null` when not away). */
export function rhythm({ now, lastStir, lastFrame, away, fadedAt, hidden }) {
  if (hidden) return 'stop';
  if (away && fadedAt !== null && now >= fadedAt) return 'stop';
  if (now - lastStir < TUNING.stirredSecs) return 'frame';
  return now - lastFrame >= 1 / TUNING.calmFps - TUNING.calmSlack ? 'frame' : 'skip';
}

/** The step's length for `elapsedSecs` since the last simulated frame: never over `maxDt`, never
 * below 0. */
export function frameDt(elapsedSecs) {
  return Number.isFinite(elapsedSecs) ? Math.min(Math.max(elapsedSecs, 0), TUNING.maxDt) : 0;
}

/** The resting source's splat for one frame of `dt` seconds: at `anchor` (`{ x, y }` in viewport
 * pixels from the top left) on a canvas of `size` `[width, height]` CSS pixels, with `rgb`
 * (three numbers 0 to 1) at `strength`. Positions are uv (y up); the radius is in units of the
 * canvas's height; `pull` is the velocity's target `[x, y, 0, 0]` in cells per second and `rate`
 * the share of the way there this frame (the push is `restDrift` along `angle`, radians from the
 * right, y up); `dye` is the colour times its amount, then the amount. */
export function restSplat(anchor, size, strength, dt, rgb, angle) {
  const amount = TUNING.restDye * strength * dt;
  return {
    rest: true,
    ...uvOf(anchor, size),
    radius: radiusUv(TUNING.restRadius, size),
    pull: [Math.cos(angle) * TUNING.restDrift * strength,
      Math.sin(angle) * TUNING.restDrift * strength, 0, 0],
    rate: 1 - Math.exp(-TUNING.restPull * dt),
    dye: dyeOf(rgb, amount),
  };
}

/** The dye splat's value for `rgb` (three numbers 0 to 1) and `amount`: the colour times the
 * amount, then the amount. */
export function dyeOf(rgb, amount) {
  return [rgb[0] * amount, rgb[1] * amount, rgb[2] * amount, amount];
}

/** The grids' move in uv for a page that scrolled `dx`, `dy` CSS pixels (the content goes the
 * other way; uv y is up) on a canvas of `size`; no move when anything is not a finite size. */
export function shiftUv(dx, dy, size) {
  const [cw, ch] = size;
  if (!(cw > 0 && ch > 0 && Number.isFinite(dx) && Number.isFinite(dy))) return [0, 0];
  return [-dx / cw, dy / ch];
}

/** A viewport point `{ x, y }` (pixels from the top left) as a grid position on a canvas of
 * `size`: `{ x, y }` in uv, y up. */
export function uvOf(point, size) {
  return { x: point.x / size[0], y: 1 - point.y / size[1] };
}

/** A Gaussian radius of `share` of the canvas's short side, in units of its height. */
export function radiusUv(share, size) {
  return (share * Math.min(size[0], size[1])) / size[1];
}

/** The pixel box `{ x, y, w, h }` of a grid of `w` by `h` texels that a Gaussian splat at `point`
 * (uv) with `radius` (units of the canvas's height; `aspect` is its width over its height) can
 * change: its 3-radius square, where the Gaussian is still over 1e-4, clipped to the grid; `null`
 * when nothing of it is on the grid or anything is not a finite number. */
export function splatBox(point, radius, aspect, [w, h]) {
  const reach = 3 * radius;
  if (![point.x, point.y, reach, aspect, w, h].every(Number.isFinite) || !(aspect > 0)) return null;
  const x0 = Math.max(0, Math.floor((point.x - reach / aspect) * w));
  const x1 = Math.min(w, Math.ceil((point.x + reach / aspect) * w));
  const y0 = Math.max(0, Math.floor((point.y - reach) * h));
  const y1 = Math.min(h, Math.ceil((point.y + reach) * h));
  return x1 > x0 && y1 > y0 ? { x: x0, y: y0, w: x1 - x0, h: y1 - y0 } : null;
}

/** The splat of a pointer move of `dx`, `dy` CSS pixels (y down) over `dt` seconds on a canvas
 * whose short side is `shortSide` px, `pressed` when a button or a finger is down: the force
 * `fx`, `fy` along the motion (screen orientation, y down; cells per second, its length
 * proportional to the speed and never over `maxPush`) and the `dye` a pressed pointer leaves
 * (by the distance moved, so the event rate does not change it). Nothing for `dt <= 0`. */
export function pointerSplat({ dx, dy, dt, shortSide, pressed }) {
  const dist = Math.hypot(dx, dy);
  if (!(dt > 0 && shortSide > 0 && Number.isFinite(dist))) return { fx: 0, fy: 0, dye: 0 };
  const push = Math.min((dist / dt / shortSide) * TUNING.pointerPush, TUNING.maxPush);
  if (!(dist > 0)) return { fx: 0, fy: 0, dye: 0 }; // a motionless pointer adds nothing
  const dye = pressed ? Math.min((dist / shortSide) * TUNING.pointerDyePerSide,
    TUNING.pointerDyeMax) : 0;
  return { fx: (dx / dist) * push, fy: (dy / dist) * push, dye };
}

/** The dial's dot at the knob's fraction `at` (0 to 1) on the arc of `{ x, y, radius }`: the
 * knob's angle is -135 degrees + 270 degrees * `at` (0 up, clockwise), the dot `x + radius sin a`,
 * `y - radius cos a`; `tx`, `ty` is the clockwise unit tangent there (screen orientation). */
export function dialDot({ x, y, radius }, at) {
  const a = ((-135 + 270 * unit(at)) * Math.PI) / 180;
  return {
    x: x + radius * Math.sin(a), y: y - radius * Math.cos(a), tx: Math.cos(a), ty: Math.sin(a),
  };
}

/** The puff of a volume step of `direction` (+1 up, -1 down) and `size` (its share of the dial's
 * range) at `dot` (from [`dialDot`]) while the glow's `level` is what it is: `x`, `y` where, the
 * force `fx`, `fy` along the tangent in the step's direction (cells per second) and the `dye`.
 * Both grow with `size` up to `puffFullSize` and with `level` from `puffFloor` of a puff, and stop
 * there, so a climb of many steps is a stream and not a burst. */
export function stepPuff(dot, direction, size, level) {
  const share = unit(size / TUNING.puffFullSize)
    * (TUNING.puffFloor + (1 - TUNING.puffFloor) * unit(level));
  const along = (direction > 0 ? 1 : direction < 0 ? -1 : 0) * TUNING.puffPush * share;
  const dye = TUNING.puffDye * share;
  return { x: dot.x, y: dot.y, fx: dot.tx * along, fy: dot.ty * along, dye };
}

/** What a meter at `fraction` (0 to 1) emits: `dye` per second and the `drift`, its push (cells
 * per second, along the caller's wandering direction), with an extra kick from `red` up. Nothing
 * for a missing reading. */
export function meterEmission(fraction, red) {
  const f = unit(fraction);
  return {
    dye: TUNING.meterDye * f,
    drift: TUNING.meterDrift * f + (f > 0 && f >= red ? TUNING.meterRedKick : 0),
  };
}

/** The stir of a scroll of `dy` CSS pixels (positive when the page moves down) in `dt` seconds
 * on a canvas whose short side is `shortSide` px: `push`, the velocity along the content's motion
 * (cells per second; positive pushes the smoke up the screen, the way the page's shift moves it)
 * growing with the speed in short sides a second and never over `maxScrollPush`, and `agit`, the
 * agitation it adds. Both 0 for `dt <= 0` or an input that is not finite. */
export function scrollStir({ dy, dt, shortSide }) {
  if (!(dt > 0 && shortSide > 0 && Number.isFinite(dy) && Number.isFinite(dt)
    && Number.isFinite(shortSide)) || dy === 0) return { push: 0, agit: 0 };
  const speed = Math.abs(dy) / dt / shortSide;
  const push = Math.min(speed * TUNING.scrollPush, TUNING.maxScrollPush);
  const agit = Math.max(0, Math.min(speed, TUNING.agitSpeedMax) - TUNING.agitScrollFloor)
    * dt * TUNING.agitScroll;
  return { push: Math.sign(dy) * push, agit };
}

/** A source's push direction `angle` (radians) after `dt` seconds of a slow random walk that
 * takes its steps from `random` (`wanderRate` radians per square root of a second), kept in
 * [0, 2 pi). A bad angle is 0 and a bad `dt` no step. */
export function wander(angle, dt, random) {
  const turn = 2 * Math.PI;
  const from = Number.isFinite(angle) ? angle : 0;
  const step = dt > 0 && Number.isFinite(dt)
    ? TUNING.wanderRate * Math.sqrt(dt) * Math.sqrt(3) * (2 * random() - 1) : 0;
  return (((from + step) % turn) + turn) % turn;
}

/** The volume step between two readings `{ raw, lo, hi }` (`lo`, `hi` the knob's range): its
 * `direction` (+1 up, -1 down) and `size` (the change's share of the range), or `null` for the
 * first reading (none before), an unread one on either side or no change. */
export function stepOf(previous, next) {
  if (!previous || !next) return null;
  const { raw: before } = previous;
  const { raw, lo, hi } = next;
  if (!(Number.isFinite(before) && Number.isFinite(raw) && hi > lo) || raw === before) return null;
  return { direction: raw > before ? 1 : -1, size: Math.abs(raw - before) / (hi - lo) };
}

/** `a` when it is a finite number above zero, else 0. */
function positive(a) {
  return Number.isFinite(a) && a > 0 ? a : 0;
}

/** The agitation of a pointer move of `dist` px over `dt` seconds on a canvas whose short side is
 * `shortSide` px: its speed in short sides a second (not the capped push), between the floor and
 * the clamp, times `dt` (so the event rate does not change it). */
export function pointerAgitation({ dist, dt, shortSide }) {
  if (!(dt > 0 && shortSide > 0 && dist > 0 && Number.isFinite(dist))) return 0;
  const speed = Math.min(dist / dt / shortSide, TUNING.agitSpeedMax);
  return Math.max(0, speed - TUNING.agitSpeedFloor) * dt * TUNING.agitPointer;
}

/** The agitation of a volume step of `size` (its share of the dial's range). */
export function stepAgitation(size) {
  return positive(size) * TUNING.agitStep;
}

/** The agitation of a meter at `fraction` over `dt` seconds: its excess over `red`, as a share
 * of what is left of the scale, times `agitMeter` per second. Nothing at or under red. */
export function meterAgitation(fraction, red, dt) {
  const excess = (unit(fraction) - red) / (1 - red);
  return positive(excess) * positive(dt) * TUNING.agitMeter;
}

/** The loudest of the first `limit` readings `{ i, fraction }` (the first on a tie; unreadable
 * ones skipped), or `null` when there is none. */
export function peakOf(fractions, limit) {
  if (!Array.isArray(fractions)) return null;
  let best = null;
  for (let i = 0; i < Math.min(limit, fractions.length); i++) {
    const f = fractions[i];
    if (Number.isFinite(f) && (best === null || f > best.fraction)) best = { i, fraction: f };
  }
  return best;
}

/** A running measure of how hard the smoke is being stirred: contributions add up and decay as
 * `exp(-dt / agitDecaySecs)`; `where()` is the place of the largest recent one. Times are in
 * seconds on one clock; a time earlier than the last one seen counts as the last. */
export class Agitation {
  /** An agitation of nothing. */
  constructor() {
    this.reset();
  }

  /** Forget everything. */
  reset() {
    this.sum = 0;
    this.at = 0;
    this.best = null;
  }

  /** Decay the sum (and the best contribution) to `now`. */
  settle(now) {
    if (!(now > this.at)) return;
    const k = Math.exp(-(now - this.at) / TUNING.agitDecaySecs);
    this.sum *= k;
    if (this.best) this.best.amount *= k;
    this.at = now;
  }

  /** Add `amount` at the place `x`, `y` at time `now`; ignored unless all are finite and the
   * amount is above zero. */
  add(amount, x, y, now) {
    if (!(amount > 0 && Number.isFinite(amount) && Number.isFinite(x) && Number.isFinite(y)
      && Number.isFinite(now))) return;
    this.settle(now);
    this.sum += amount;
    if (!this.best || amount >= this.best.amount) this.best = { amount, x, y };
  }

  /** The measure at `now`. */
  value(now) {
    this.settle(now);
    return this.sum;
  }

  /** The place `{ x, y }` of the largest recent contribution, or `null` when there is none. */
  where() {
    return this.best ? { x: this.best.x, y: this.best.y } : null;
  }
}

/** The rule for when a note may be stamped: `cooldown` seconds have passed since the last note
 * and fewer than `cap` are within their `life`. */
export class NoteGate {
  /** A gate with `cooldown` seconds between notes, notes visible for `life` seconds and at most
   * `cap` visible at once (the `TUNING` values unless given). */
  constructor({
    cooldown = TUNING.noteCooldown, life = TUNING.noteLife, cap = TUNING.noteCap,
  } = {}) {
    this.cooldown = cooldown;
    this.life = life;
    this.cap = cap;
    this.stamps = [];
  }

  /** How many stamped notes are still within their life at `now`. */
  live(now) {
    return this.stamps.filter((t) => now - t < this.life).length;
  }

  /** Whether a note may be stamped at `now`: the gap has passed and the cap is not reached. */
  allows(now) {
    if (!Number.isFinite(now)) return false;
    const last = this.stamps[this.stamps.length - 1];
    if (last !== undefined && now - last < this.cooldown) return false;
    return this.live(now) < this.cap;
  }

  /** Record a stamped note at `now`; ignored unless `now` is finite. */
  record(now) {
    if (!Number.isFinite(now)) return;
    this.stamps = this.stamps.filter((t) => now - t < this.life);
    this.stamps.push(now);
  }
}

/** The agitation of a volume step of `size`, at least `agitStepMin`, so a single step arms a
 * settle. Nothing for no size. */
export function stepStir(size) {
  const a = stepAgitation(size);
  return a > 0 ? Math.max(a, TUNING.agitStepMin) : 0;
}

/** The puff of a control's click at `at` (`{ x, y }`) along `angle` (radians): `x`, `y`, the
 * force `fx`, `fy` (cells per second) and the `dye`; small, and the same size every time. */
export function clickPuff(at, angle) {
  return {
    x: at.x,
    y: at.y,
    fx: Math.cos(angle) * TUNING.clickPuffPush,
    fy: Math.sin(angle) * TUNING.clickPuffPush,
    dye: TUNING.clickPuffDye,
  };
}

/** The puff of a music beat of `strength` (0 to 1) at `dot` (from [`dialDot`]) on the arc of
 * `anchor` (`{ x, y }` its centre): the force `fx`, `fy` points away from the centre, the `dye`
 * and the push grow with `strength`. A dot on the centre pushes nowhere. */
export function beatPuff(dot, anchor, strength) {
  const share = unit(strength);
  const dx = dot.x - anchor.x;
  const dy = dot.y - anchor.y;
  const dist = Math.hypot(dx, dy) || 1;
  const push = TUNING.beatPuffPush * share;
  return {
    x: dot.x, y: dot.y, fx: (dx / dist) * push, fy: (dy / dist) * push,
    dye: TUNING.beatPuffDye * share,
  };
}

/** The rise of volume over one burst of steps: a burst ends after `burstGapSecs` without a
 * change, or on a step down. */
export class VolumeBurst {
  /** A burst of nothing. */
  constructor() {
    this.end();
    this.at = -Infinity;
  }

  /** Forget the burst. */
  end() {
    this.rise = 0;
    this.share = 0;
    this.dbs = true;
  }

  /** Add the step from `beforeDb` to `afterDb` (dB, or `null`; silence counts as the lowest finite
   * step) that moved `share` of the dial's range (negative for a step down) at `now` (s). Returns
   * the burst's `rise` in dB (0 unless every step had dB readings) and `riseShare`, its share of
   * the range (0 while every step had dB readings); both 0 after a step down. */
  add(beforeDb, afterDb, share, now) {
    if (now - this.at > TUNING.burstGapSecs) this.end();
    this.at = now;
    const both = Number.isFinite(beforeDb) && Number.isFinite(afterDb);
    const direction = both ? afterDb - beforeDb : share;
    if (direction < 0) {
      this.end();
      return { rise: 0, riseShare: 0 };
    }
    if (both) this.rise += afterDb - beforeDb;
    else this.dbs = false;
    if (Number.isFinite(share) && share > 0) this.share += share;
    return { rise: this.dbs ? this.rise : 0, riseShare: this.dbs ? 0 : this.share };
  }
}

/** Whether a burst's rise is a big one: over `bigRiseDb` dB, or (a dial without dB) over
 * `bigRiseShare` of its range, or it `enteredRed`, the dial's red zone. */
export function bigRise({ rise, riseShare, enteredRed } = {}) {
  return rise > TUNING.bigRiseDb || riseShare > TUNING.bigRiseShare || enteredRed === true;
}

/** Notes come as a stir settles. Stirs (each with the odds of a note) add up in an
 * [`Agitation`]; the agitation rising past `settleFrom` and then falling under `settleBelow` at
 * least `settleSecs` later is one settle, which rolls the episode's odds, the highest of its
 * stirs'. */
export class SettleWatch {
  /** A watch with nothing pending. */
  constructor() {
    this.agitation = new Agitation();
    this.rolls = 0;
    this.wins = 0;
    this.reset();
  }

  /** Forget the pending episode (away, lost, a level change). */
  reset() {
    this.agitation.reset();
    this.armedAt = null;
    this.odds = 0;
  }

  /** A stir of `amount` at `x`, `y` at `now` (s) whose settle makes a note with chance `odds`;
   * ignored unless all are finite and the amount and odds are above zero. */
  stir(amount, x, y, now, odds) {
    if (!(amount > 0 && Number.isFinite(amount) && odds > 0 && Number.isFinite(odds)
      && Number.isFinite(x) && Number.isFinite(y) && Number.isFinite(now))) return;
    this.agitation.add(amount, x, y, now);
    this.odds = Math.max(this.odds, odds);
  }

  /** The place `{ x, y }` of a settle that rolled well at `now` with `random` (a function giving
   * [0, 1)), else `null`; a settle is answered once. */
  poll(now, random) {
    if (!Number.isFinite(now)) return null;
    const value = this.agitation.value(now);
    if (this.armedAt === null) {
      if (value > TUNING.settleFrom) this.armedAt = now;
      return null;
    }
    if (value >= TUNING.settleBelow || now - this.armedAt < TUNING.settleSecs) return null;
    const at = this.agitation.where();
    const odds = this.odds;
    this.reset();
    if (!at) return null;
    this.rolls++;
    if (!(random() < odds)) return null;
    this.wins++;
    return at;
  }
}

/** A note's look from `random` (a function giving [0, 1)): one of the three `glyph`s (0 to 2), its
 * `tilt` in degrees (within `noteTiltDeg`) and its `size`, the glyph's visible height, in the
 * unit of `shortSide` (`noteSizeMin` to `noteSizeMax` of it). */
export function notePlacement(random, shortSide) {
  const glyph = Math.min(2, Math.floor(random() * 3));
  const tilt = (random() * 2 - 1) * TUNING.noteTiltDeg;
  const span = TUNING.noteSizeMax - TUNING.noteSizeMin;
  const size = shortSide * (TUNING.noteSizeMin + span * random());
  return { glyph, tilt, size };
}

/** The note's centre for the stirring at `at` (`{ x, y }`) kept `reach` px inside a canvas of
 * `size` (`[width, height]`); a canvas smaller than twice `reach` puts it at `reach`. */
export function notePosition(at, reach, size) {
  const keep = (v, extent) => Math.min(Math.max(v, reach), Math.max(reach, extent - reach));
  return { x: keep(at.x, size[0]), y: keep(at.y, size[1]) };
}

/** The cell's side for a glyph whose visible height is `height` (both in px). */
export function noteCell(height) {
  return height / TUNING.noteGlyphHeight;
}

/** `rgb` (three numbers 0 to 1) a `share` of the way to white. */
export function lighten(rgb, share) {
  return rgb.map((c) => c + (1 - c) * share);
}

/** A deterministic generator of numbers in [0, 1) from `seed` (mulberry32), for the checks. */
export function seeded(seed) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/** The sRGB triple (0 to 1) of a hue `h` (degrees, any), saturation `s` and lightness `l`
 * (percent). */
function hslToRgb(h, s, l) {
  const hue = ((h % 360) + 360) % 360;
  const sat = unit(s / 100);
  const light = unit(l / 100);
  const a = sat * Math.min(light, 1 - light);
  const channel = (n) => {
    const k = (n + hue / 30) % 12;
    return light - a * Math.max(-1, Math.min(k - 3, 9 - k, 1));
  };
  return [channel(0), channel(8), channel(4)];
}

/** The palette a page draws once from `random` (a function giving [0, 1)): three cool tones of
 * one saturation, the side bands' two `sides` (hues `shadeStepLo` to `shadeStepHi` either side
 * of the middle hue) and the `dial`'s (the middle hue, lighter), sRGB triples 0 to 1; with them
 * `base` (the middle hue), the greyer `grey` and the lighter tint `light` that stirs, puffs and
 * meters take, `greyShare` (how many stir emissions take the grey) and the base's `hsl` (hue in
 * degrees, saturation and lightness in percent). The grey's hue is within `shadeGreyHue` of the
 * base's and wraps into [0, 360). */
export function drawShade(random) {
  const between = (lo, hi) => lo + (hi - lo) * random();
  const h = between(TUNING.shadeHueLo, TUNING.shadeHueHi);
  const s = between(TUNING.shadeSatLo, TUNING.shadeSatHi);
  const l = between(TUNING.shadeLightLo, TUNING.shadeLightHi);
  const greyHue = (((h + between(-TUNING.shadeGreyHue, TUNING.shadeGreyHue)) % 360) + 360) % 360;
  const greyShare = between(TUNING.shadeGreyShareLo, TUNING.shadeGreyShareHi);
  const step = between(TUNING.shadeStepLo, TUNING.shadeStepHi);
  const light = hslToRgb(h, s, Math.min(TUNING.shadeLightMax, l + TUNING.shadeLighter));
  return {
    base: hslToRgb(h, s, l),
    grey: hslToRgb(greyHue, s / 3, l - TUNING.shadeGreyDark),
    light,
    sides: [hslToRgb(h - step, s, l), hslToRgb(h + step, s, l)],
    dial: [...light],
    greyShare,
    hsl: { h, s, l },
  };
}

/** The shade's `grey` with probability `greyShare`, else its `base`: the colour of one emission. */
export function shadePick(shade, random) {
  return random() < shade.greyShare ? shade.grey : shade.base;
}

/** One of the palette's two side tones, each with probability a half: the colour of one haze
 * emission. */
export function sidePick(shade, random) {
  return shade.sides[random() < 0.5 ? 0 : 1];
}

/** The haze's bands for a canvas of `size` with the content `column` (`{ left, right }` in px, or
 * `null`): `left` and `right` the bands' widths (px) and `gain` their strength. Margins beside
 * the column that are each at least `hazeEdge` of the width are the bands; else the outer
 * `hazeEdge` of the width is, at `hazeEdgeGain` of the strength. `margins` tells which. */
export function hazeBands(size, column) {
  const [cw] = size;
  const edge = TUNING.hazeEdge * cw;
  const has = column && Number.isFinite(column.left) && Number.isFinite(column.right)
    && column.left >= edge && cw - column.right >= edge;
  if (has) return { left: column.left, right: cw - column.right, gain: 1, margins: true };
  return { left: edge, right: edge, gain: TUNING.hazeEdgeGain, margins: false };
}

/** Whether `size` is a canvas the haze can fill: two finite numbers above zero. */
function usable(size) {
  return Array.isArray(size) && size.length === 2 && size.every((v) => Number.isFinite(v) && v > 0);
}

/** A side source's strength at `u` (0 at the window's edge, 1 where the band meets the content):
 * linear from 1 down to `hazeInner`. */
function sideWeight(u) {
  return 1 - (1 - TUNING.hazeInner) * u;
}

/** The haze's sources, one per slot, for a canvas of `size` and `column`: `{ side, u, density,
 * radius }` (`side` -1 left, 1 right, 0 spread; `u` the base place along the band; `density` the
 * steady density at its centre; `radius` in px). */
function hazeSources(size, column) {
  const short = Math.min(size[0], size[1]);
  const mean = Math.sqrt(size[0] * size[1]);
  const { gain } = hazeBands(size, column);
  const list = [];
  const n = TUNING.hazeSideCount;
  for (const side of [-1, 1]) {
    for (let k = 0; k < n; k++) {
      const reach = TUNING.hazeReach;
      const u = n > 1 ? TUNING.hazeWander + (reach - 2 * TUNING.hazeWander) * (k / (n - 1))
        : reach / 2;
      list.push({ side, u, density: TUNING.hazeSide * gain * sideWeight(u),
        radius: TUNING.hazeSideRadius * short });
    }
  }
  for (let k = 0; k < TUNING.hazeSpreadCount; k++) {
    list.push({ side: 0, u: 0, density: TUNING.hazeSpread,
      radius: TUNING.hazeSpreadRadius * mean });
  }
  return list;
}

/** The dye the haze holds when it is steady, in amount times px2: each source's centre density
 * times its Gaussian's area. Its emission per second is `dyeDissipation` times this. 0 for a
 * canvas that is not usable. */
export function hazeTarget(size, column) {
  if (!usable(size)) return 0;
  return hazeSources(size, column).reduce((t, s) => t + s.density * Math.PI * s.radius ** 2, 0);
}

/** The haze: steady sources that feed the dye behind the whole page, denser in the bands beside
 * the content, each wandering slowly and pushing the fluid weakly in a direction that turns
 * slowly. Its first splats also lay down the steady density at once, so a page (or a new
 * canvas) starts with its haze. Its state is the sources' phases and shades, drawn once from
 * `random`. */
export class Haze {
  /** A haze whose phases come from `random` and whose emissions take `shade` (one drawn from
   * `random` when none is given). */
  constructor(random, shade = drawShade(random)) {
    this.random = random;
    this.shade = shade;
    this.primed = false;
    const count = 2 * TUNING.hazeSideCount + TUNING.hazeSpreadCount;
    const spin = () => 2 * Math.PI * random();
    const period = (secs) => (2 * Math.PI) / (secs * (0.7 + 0.6 * random()));
    this.parts = Array.from({ length: count }, () => ({
      px: spin(), py: spin(), pt: spin(),
      wx: period(TUNING.hazeWanderSecs), wy: period(TUNING.hazeWanderSecs),
      wt: period(TUNING.hazeTurnSecs) * (random() < 0.5 ? -1 : 1),
      rgb: shade.sides[0], until: -Infinity,
    }));
  }

  /** The splats for one frame of `dt` seconds at time `now` (s) on a canvas of `size` (CSS px)
   * with the content `column`: `{ x, y, radius, dye, add, rest }` in CSS px, the dye's value
   * (colour times amount, then the amount), the velocity added (cells per second) and `rest`
   * (they count as calm). Nothing for a canvas that is not usable; a bad `dt` is 0 and a long
   * one `hazeMaxStep`. The first frame adds the steady density as well. Deterministic for a given
   * `random`. */
  splats({ size, column, now, dt }) {
    if (!usable(size)) return [];
    const [cw, ch] = size;
    const step = Number.isFinite(dt) ? Math.min(Math.max(dt, 0), TUNING.hazeMaxStep) : 0;
    const t = Number.isFinite(now) ? now : 0;
    const bands = hazeBands(size, column);
    const first = !this.primed;
    this.primed = true;
    return hazeSources(size, column).map((src, i) => {
      const p = this.parts[i];
      if (t >= p.until) {
        p.rgb = sidePick(this.shade, this.random);
        p.until = t + TUNING.hazePickSecs * (0.5 + this.random());
      }
      const along = (phase, w) => Math.sin(w * t + phase);
      // a spread source drifts without leaving the canvas; a side source may reach the window's
      // edge, where half its Gaussian falls outside (a band's strength counts that loss)
      const keep = (extent, wave, edge = src.radius) =>
        extent / 2 + (extent / 2 - Math.min(edge, extent / 2)) * wave;
      let x;
      let y;
      let weight = 1;
      if (src.side === 0) {
        x = keep(cw, along(p.px, p.wx), Math.min(src.radius, TUNING.hazeSpreadMargin * cw));
        y = keep(ch, along(p.py, p.wy));
      } else {
        // each side source keeps to its own slot of the height, so a band is never patchy
        const slot = ch / TUNING.hazeSideCount;
        y = ((i % TUNING.hazeSideCount) + 0.5) * slot
          + TUNING.hazeSlotWander * slot * along(p.py, p.wy);
        const u = Math.min(1, Math.max(0, src.u + TUNING.hazeWander * along(p.px, p.wx)));
        const band = src.side < 0 ? bands.left : bands.right;
        x = src.side < 0 ? u * band : cw - u * band;
        weight = sideWeight(u) / sideWeight(src.u);
      }
      const amount = (TUNING.dyeDissipation * step + (first ? 1 : 0)) * src.density * weight;
      const angle = p.pt + p.wt * t;
      return {
        rest: true, x, y, radius: src.radius, dye: dyeOf(p.rgb, amount),
        add: [Math.cos(angle) * TUNING.hazePush * step, Math.sin(angle) * TUNING.hazePush * step],
      };
    });
  }
}

/** When the haze is fed: at every simulated frame, the capped `dt` that the dye's dissipation
 * also uses (never the wall clock), so the haze is as steady from frame to frame as the dial's
 * cloud and a slow frame rate cannot feed more per simulated second than the dye loses. */
export class HazeClock {
  constructor() {
    this.fed = false;
  }

  /** One frame's simulated `dt` (s). Returns the simulated seconds to feed now: the first frame
   * whatever its step (it lays the steady density down), then every frame with a step above 0;
   * `null` for a bad or empty step. */
  step(dt) {
    const good = Number.isFinite(dt) && dt > 0;
    if (this.fed && !good) return null;
    this.fed = true;
    return good ? dt : 0;
  }
}
