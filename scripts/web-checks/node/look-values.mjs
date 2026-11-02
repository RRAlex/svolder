/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The approved look of the smoke: the numbers of `TUNING` (bins/svolderd/web/themes/ember/smoke-model.js) that
// the node harnesses' scenarios are written for. This file is the one place that holds them.
//
// To retune the smoke on purpose, change the number in `TUNING` and the same number here, then run
// the node harnesses: the scenarios written for the old number show whether the rule they prove
// still holds. Until the two agree, `node/model.mjs` fails its test "the approved look" and names
// the numbers. The other harnesses run (and may fail honestly) unless LOOK_RETUNED_SKIPS=1 is set,
// as `run-all.sh` sets it, so that one retuned number fails one check. A number not listed here is
// a free choice that no node test depends on.

export const LOOK = Object.freeze({
  // the simulation
  simRes: 128, // the velocity grid's cells on the canvas's short side
  dyeRes: 512, // the dye grid's cells on the canvas's short side
  pressureIterations: 20, // Jacobi passes per frame
  pressureDamp: 0.8, // the previous pressure kept as the next frame's start
  calmFps: 20, // simulated frames per second while nothing stirs
  calmSlack: 0.004, // a calm frame may come this early (s), so 60 Hz callbacks keep 20 fps
  stirredSecs: 2, // how long after a splat every animation frame is simulated (s)
  fadeSecs: 1, // the away look's fade, and a level change's settle (s)
  dprCap: 1.5, // the canvas's backing pixels per CSS pixel, at most
  maxDt: 0.05, // the longest step the simulation takes (s)
  // the meters
  meterRed: 0.93, // a meter's fraction from which it is red (the server's RED_FROM)
  agitMeter: 10, // agitation per second of the loudest meter at the top of its scale
  // the notes: the stir that arms one, the odds, the gate and the glyphs
  agitScrollFloor: 1, // the scroll speed under which nothing is added (short sides a second)
  agitDecaySecs: 0.5, // the agitation's decay time constant (s)
  agitSpeedMax: 8, // the pointer speed above which a flick adds no more (short sides a second)
  agitPointer: 3.5, // agitation per short side a second of pointer speed above the floor
  agitSpeedFloor: 0.6, // the pointer speed under which nothing is added (short sides a second)
  agitStepMin: 1.5, // the least agitation of a volume step, so one step arms a settle
  clickAgit: 1.5, // the agitation of a control's click
  settleFrom: 1, // the agitation a stir must pass to count as one a note may follow
  settleOdds: 1 / 3, // the chance a settled stir makes a note
  clickOdds: 1 / 2, // the chance a settled control click makes a note
  bigRiseOdds: 2 / 3, // the chance a settled big volume rise makes a note
  burstGapSecs: 1, // a pause this long ends a burst of volume steps (s)
  bigRiseDb: 10, // a burst's rise over this many dB is a big one
  bigRiseShare: 0.125, // a dial without dB: a rise over this share of its range is big
  noteCooldown: 4, // the least time between two notes (s)
  noteLife: 3, // how long a stamped note counts as visible (s)
  noteCap: 2, // the most notes visible at once
  noteTiltDeg: 25, // a note's tilt, either way (degrees)
  noteSizeMin: 0.06, // a note's smallest side (of the canvas's short side)
  noteSizeMax: 0.09, // a note's largest side (of the canvas's short side)
  noteGlyphHeight: 0.64, // a glyph's visible height as a share of its cell (all three)
  // the haze
  hazeSideCount: 3, // the sources in each side band
  hazeSpreadCount: 3, // the sources spread over the whole viewport
  hazeSide: 0.024, // the steady dye density of a side-bar source at its centre (amount)
  hazeReach: 0.6, // how far into a band a side source may go (of the band's width)
  hazeSideRadius: 0.14, // a side source's Gaussian radius (of the canvas's short side)
  hazeEdgeGain: 0.4, // the side bands' strength where there are no margins (share of a margin's)
  hazeSpread: 0.0016, // the steady dye density of a spread source at its centre
  hazeEdge: 0.12, // the side bands' width when there are no margins beside the content
  hazeSlotWander: 0.35, // how far a side source drifts up and down its own slot of the height
  hazeFadeSecs: 1.5, // how long the dye in the content's place fades after the margins change (s)
  // the palette the page draws once per load
  shadeHueLo: 205, // the lowest middle hue, cyan-blue (degrees)
  shadeHueHi: 275, // the highest middle hue, violet (degrees)
  shadeStepLo: 22, // the least hue distance of a side tone from the dial's tone (degrees)
  shadeStepHi: 30, // the greatest hue distance of a side tone from the dial's tone (degrees)
  shadeSatLo: 54, // the lowest saturation (percent)
  shadeSatHi: 60, // the highest saturation (percent)
  shadeLightLo: 55, // the lowest base lightness (percent)
  shadeLightHi: 74, // the highest base lightness (percent)
  shadeGreyHue: 25, // the grey shade's hue, either side of the base (degrees)
  shadeGreyDark: 4, // how much darker the grey shade is than the base (lightness percent)
  shadeLighter: 10, // how much lighter the light tint is than the base (lightness percent)
  shadeLightMax: 82, // the light tint's greatest lightness (percent)
  shadeGreyShareLo: 0.3, // the lowest share of emissions that take the grey shade
  shadeGreyShareHi: 0.7, // the highest share of emissions that take the grey shade
});

/** The numbers where `tuning` differs from the approved look, as sentences. */
export function retuned(tuning) {
  return Object.keys(LOOK)
    .filter((k) => tuning[k] !== LOOK[k])
    .map((k) => `${k}: the code has ${tuning[k]}, the approved look has ${LOOK[k]}`);
}

/** `ok` for tests written for the approved look. When the look is retuned and the run says another
 * check reports it (LOOK_RETUNED_SKIPS=1), they are skipped; otherwise they run and may fail. */
export function lookTests(tuning, ok) {
  const diffs = retuned(tuning);
  if (diffs.length) {
    console.log(`look: the code's TUNING differs from node/look-values.mjs (${diffs.join('; ')})`);
  }
  const skip = diffs.length > 0 && process.env.LOOK_RETUNED_SKIPS === '1';
  return (name, fn) => {
    if (skip) console.log("skip", name, "(written for the approved look; see node/model.mjs)");
    else ok(name, fn);
  };
}
