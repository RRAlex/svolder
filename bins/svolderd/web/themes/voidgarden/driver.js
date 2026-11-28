/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// What Void Garden's checks reach into: the static test driver the theme registers (as Night
// drive's does). It holds no logic: it asks the parts a mount binds (`bindDriver`, called with
// the four `{ driver }` objects the scene, HUD, panel and dash wires return) in order, and the
// first part that answers wins. While no part is bound, or none answers, every call refuses.
// The scene part answers `tier`, `frame`, `pixels`, `perf`, `forceDrawMs` and `hold`; the HUD part
// the
// facts it shows (`shown`, `linkWords`); the dash part the controls it draws (`reach`, `operate`,
// `shown`) and how the page stands (`state`). A target the dash does not draw is not taken by the
// dash part's own answer: it hands the target to the core's driver of the Device controls popup
// (`core.driver.controls`), so every control of the device and of the computer is reached.
//
//   reach(target) -> boolean            puts the page as the target needs it; is it drawn
//   operate(target, change) -> boolean  does what a person does to the target; false: not taken
//   shown(target) -> string | null      the words shown for a control, or for a fact by its label
//   state() -> object | null            how the page stands
//   linkWords() -> string | null        the words beside the shown device's name for its link
//   tier() -> string | null             the scene's draw tier (the scene part)
//   frame(seed, t) -> any | null        one frame of the scene for a seed at a time (scene part)
//   pixels(...) -> any | null           a read of the drawn pixels (scene part)
//   perf() -> object | null             the scene's draw cost (scene part)
//   forceDrawMs(ms) -> boolean          makes each draw take `ms` (scene part); false: refused
//   hold(step, fps) -> boolean          holds the budget's ladder at `step`, at 30 or 60 a second
//   freeze() -> boolean                 stops the live loop (scene part), for a still
//   still(seed, t) -> any | null        one frame at a seed and time drawn on the canvas, under the
//                                       budget's levels now (scene part)
//                                       (scene part; the checks and the measure); false: refused
//   heroAt() -> [x, y, front] | null   the hero's centre on the canvas, clip units (scene part)

/** The driver objects of the mounted parts, in the order they were bound; empty while unmounted. */
let bound = [];

/** Bind the parts of a mount (each `{ driver }`), or unbind with no argument. */
export function bindDriver(parts = []) {
  bound = parts.map((part) => part?.driver).filter(Boolean);
}

/** The first answer of the bound parts to `name(...args)` that is not `none`; else `refused`. */
async function ask(name, args, none, refused) {
  for (const part of bound) {
    if (typeof part[name] !== 'function') continue;
    const answer = await part[name](...args);
    if (answer !== none && answer !== undefined) return answer;
  }
  return refused;
}

/** The first answer of the bound parts to `name(...args)` that is not `none`; else `refused`. */
function first(name, args, none, refused) {
  for (const part of bound) {
    const answer = part[name]?.(...args);
    if (answer !== none && answer !== undefined) return answer;
  }
  return refused;
}

/** The test driver. */
export const driver = {
  reach: (target) => ask('reach', [target], false, false),
  operate: (target, change = {}) => ask('operate', [target, change], false, false),
  shown: (target) => first('shown', [target], null, null),
  state: () => first('state', [], null, null),
  linkWords: () => first('linkWords', [], null, null),
  tier: () => first('tier', [], null, null),
  frame: (seed, t) => first('frame', [seed, t], null, null),
  pixels: (...args) => first('pixels', args, null, null),
  perf: () => first('perf', [], null, null),
  forceDrawMs: (ms) => first('forceDrawMs', [ms], false, false),
  hold: (step, fps) => first('hold', [step, fps], false, false),
  freeze: () => first('freeze', [], false, false),
  still: (seed, t) => first('still', [seed, t], null, null),
  heroAt: () => first('heroAt', [], null, null),
};
