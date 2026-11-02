/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The volume's climb, said for every theme: while the level climbs to its target the page's cursor
// is `progress` (the body's `climbing` class) and the climb's words show once as a popup. A popup
// is said once per climb and wording, however often the volume is painted and whichever theme
// paints it, and it is not taken away when the climb ends: it leaves with its time.

/** How long the climb's popup stays from the moment it was last said, in milliseconds. */
export const CLIMB_POPUP_MS = 6000;

/** The climb over `body` (the page's body), saying with `toast` and taking back with `dismissToast`. */
export function createClimb({ toast, dismissToast, body }) {
  /** The climb's words as last said in its popup (`null`: none, or the climb has ended). */
  let said = null;
  /** The target of a climb a touch ended: readings that still say it do not bring the climb
   * back until one says another (`null`: none ended). */
  let ended = null;

  return {
    /** The volume (a model `Volume`, or `null` for a device with none) is on show: mark the page
     * and say the climb's words when it starts or they change. */
    paint(volume) {
      if (ended !== null && volume?.target !== ended) ended = null;
      const climbing = ended === null && Boolean(volume) && volume.target !== null
        && volume.target !== undefined && volume.raw !== null && volume.raw !== undefined
        && volume.target > volume.raw;
      body.classList.toggle('climbing', climbing);
      const say = climbing ? (volume.notes ?? []).find((note) => note.popup)?.text ?? null : null;
      if (say === said) return;
      said = say;
      if (say) toast(say, null, { key: 'climb', ms: CLIMB_POPUP_MS });
    },

    /** A touch ended the climb heading for `target`: its popup and cursor go, and readings that
     * still say that target do not bring it back. `null`: the stop did not land, the climb goes
     * on. */
    cancel(target) {
      ended = target;
      if (target !== null) this.drop();
    },

    /** The page no longer knows the climb (the link is lost, the climb was stopped): the popup
     * and the cursor go. */
    drop() {
      body.classList.remove('climbing');
      dismissToast('climb');
      said = null;
    },
  };
}
