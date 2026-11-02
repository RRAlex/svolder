/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The meters' arches: the segments of one meter's arch, in the box its figure's viewBox shows.

/** Segments in an arch. */
export const SEGMENTS = 32;
/** The arch's sweep, in degrees. */
export const SWEEP = 220;
/** The fraction from which a bar is red: the top two segments. */
export const RED_FROM = 0.93;

/**
 * The segments of the arch of `meter` (`{ fraction, class_a }`): each line's ends as strings with
 * two decimals, its colour class (`brass` up to the class-A share, then `ramp`, `red` from
 * RED_FROM) and whether the reading lights it.
 */
export function arch(meter) {
  const { fraction, class_a: classA } = meter;
  const lit = fraction == null ? 0 : Math.round(fraction * SEGMENTS);
  return Array.from({ length: SEGMENTS }, (_, i) => {
    const angle = ((-SWEEP / 2 + (SWEEP * (i + 0.5)) / SEGMENTS) * Math.PI) / 180;
    const end = (r) => [
      (100 + r * Math.sin(angle)).toFixed(2),
      (100 - r * Math.cos(angle)).toFixed(2),
    ];
    const [x1, y1] = end(85);
    const [x2, y2] = end(98);
    const at = i / SEGMENTS;
    let cls = 'ramp';
    if (at >= RED_FROM) cls = 'red';
    else if (at < classA) cls = 'brass';
    return {
      x1, y1, x2, y2, class: cls, on: i < lit,
    };
  });
}
