/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// How the scene looks to the music's ear (mood.js): the night's mood, the wet road, the haze, the
// colour the heat leans to, the cruise speed, and the flare of a moment. Each Motion level has its
// own rates of change. A pure module: the draws read its result, it touches nothing.

import { ease } from './mood.js';

/** The look's numbers. `mood0` and `moodSpan`: the mood is `mood0 + moodSpan * long`; `neutral` is
 * the mood at which the scene looks as it did before there was one. `cruiseBase` and `cruiseSpan`:
 * the road's speed is the tempo's times `cruiseBase + cruiseSpan * long`, eased over `cruiseTau`.
 * `wet*`, `haze*`: the wet road and the haze from the medium view of the level, each a line from a
 * start, clamped. `balanceMid` and `balanceGain` turn the ear's balance into -1 (all bass) to 1 (all
 * highs). A moment's flare rises over `flareRise` s, falls with `flareTau` and is gone after
 * `flareFor`; Calm shows it at `calmFlare` of its strength. */
export const LOOK = Object.freeze({
  mood0: 0.18,
  moodSpan: 0.82,
  neutral: 0.7,
  cruiseBase: 0.6,
  cruiseSpan: 0.8,
  cruiseTau: 2,
  wetFrom: 0.75,
  wetSlope: 1.1,
  wetMax: 0.6,
  hazeFrom: 0.85,
  hazeSlope: 1.2,
  hazeMax: 0.75,
  balanceMid: 0.45,
  balanceGain: 4,
  flareRise: 0.15,
  flareTau: 0.8,
  flareFor: 2.5,
  calmFlare: 0.5,
});

/** The look of a scene that has no mood: what the scene looked like before there was one. */
export const NEUTRAL = Object.freeze({
  mood: LOOK.neutral, wet: 0, haze: 0, x: 0, cruise: 1, flare: 0, wave: 0, since: Infinity, embers: 0,
});

const clamp = (v, lo, hi) => Math.min(hi, Math.max(lo, v));

/** The strength (0 to 1) of a moment's flare `since` seconds after it. */
export function flareOf(since) {
  if (!(since >= 0) || since >= LOOK.flareFor) return 0;
  return Math.exp(-since / LOOK.flareTau) * (since < LOOK.flareRise ? since / LOOK.flareRise : 1);
}

/**
 * The look. `step(dt, ear, motion, off)` takes the seconds since the last step, the ear's state, the
 * Motion level (`full`, `calm`, `still`) and the set of switches in force (`nomood`, `nowet`,
 * `nohaze`, `noembers`) and returns `{ mood, wet, haze, x, cruise, flare, wave, since, embers }`: the
 * mood (0 deep night to 1 warm and loud), how wet the road is and how thick the haze (0 to 1), where
 * the heat leans (-1 warm, 1 cool), the factor on the road's speed, the flare (0 to 1), the
 * wave that runs down the floor (0 to 1, in Full only), the seconds since the moment that brought them,
 * the embers (0 to 1). The colours are the ear's, which Calm eases at half the rate (mood.js); in Calm the flare
 * is half as strong and nothing runs down the floor or rises from it; in Still there is no flare (one that was
 * running stops) and no embers.
 */
export function createLook() {
  let seen = null;
  let clock = 0;
  let momentAt = -Infinity;
  const look = { ...NEUTRAL };

  return {
    step(dt, ear, motion = 'full', off = new Set()) {
      clock += dt;
      const bare = off.has('nomood');
      if (motion === 'still') momentAt = -Infinity;
      else if (ear.moment && !bare) momentAt = clock;
      const target = {
        mood: bare ? LOOK.neutral : LOOK.mood0 + LOOK.moodSpan * ear.long,
        wet: off.has('nowet') ? 0 : clamp(LOOK.wetFrom - LOOK.wetSlope * ear.med, 0, LOOK.wetMax),
        haze: off.has('nohaze') ? 0 : clamp(LOOK.hazeFrom - LOOK.hazeSlope * ear.med, 0, LOOK.hazeMax),
        x: bare ? 0 : clamp((ear.balance - LOOK.balanceMid) * LOOK.balanceGain, -1, 1),
        cruise: bare ? 1 : LOOK.cruiseBase + LOOK.cruiseSpan * ear.long,
      };
      if (seen === null) seen = { ...target };
      else {
        for (const key of ['mood', 'wet', 'haze', 'x']) seen[key] = target[key];
        seen.cruise = ease(seen.cruise, target.cruise, dt, LOOK.cruiseTau);
      }
      Object.assign(look, seen);
      look.since = clock - momentAt;
      look.flare = flareOf(look.since) * (motion === 'calm' ? LOOK.calmFlare : 1);
      look.wave = motion === 'full' ? flareOf(look.since) : 0;
      look.embers = motion === 'full' && !off.has('noembers') ? ear.embers : 0;
      return look;
    },

    read: () => look,

    clear() {
      seen = null;
      clock = 0;
      momentAt = -Infinity;
      Object.assign(look, NEUTRAL);
    },
  };
}
