/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The tone mode's note, said for every theme: Flat and Loudness each carry a line that says what
// they do. It is toasted when the mode changes into one of them, whoever changed it, or when it is
// the mode this person just clicked, whatever was last seen (a reconnect between the two forgets
// what was). A first sight (a load, a device switch, a reconnect) and a change into EQ say nothing.

/** How long after a click on a mode a change into it counts as that click's, in milliseconds. */
export const MODE_CLICK_MS = 8000;

/** The note over `toast`, with `now` the clock in milliseconds. */
export function createToneNote({ toast, now }) {
  /** The mode seen last (`null` when unknown). */
  let last = null;
  /** The mode the person last clicked and when. */
  let clicked = null;

  return {
    /** The tone (a model `Tone`, or `null` for a device with none) is on show. */
    seen(tone) {
      const mode = tone?.mode ?? null;
      const before = last;
      last = mode;
      const mine = clicked !== null && clicked.mode === mode && now() - clicked.at < MODE_CLICK_MS;
      if (mode !== null && tone.note && (mine || (before !== null && mode !== before))) {
        clicked = null;
        toast(tone.note);
      }
    },

    /** The person clicked the mode `mode`. */
    clicked(mode) {
      clicked = { mode, at: now() };
    },

    /** What was seen is no longer known (a reconnect, a switch): the next sight is a first. */
    forget() {
      last = null;
    },
  };
}
