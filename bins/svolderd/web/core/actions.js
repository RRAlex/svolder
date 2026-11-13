/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// What a theme does to the device: every write svolder takes, as one call. A guest's or a
// read-only page's call sends nothing and says why; a refused write is toasted with the daemon's
// words and the model read again, so what the device holds is shown, not what was chosen; an
// answer's own toast is said and its parts are applied to the store. Every device call takes an
// optional last `{ device }` naming the device it acts on (the shown one by default: `aimed` is the
// one place that names it) and resolves `{ ok, status, error?, ...answer }`.
//
// The pieces are one file each: write.js (the turn-away and the finish every write shares),
// device-actions.js (the thin device writes), walk-actions.js (`actions.walk`: the device walk's
// calls), autoeq-actions.js (the EQ library's calls), volume-actions.js (the volume's one-at-a-time
// climb), computer-actions.js (`actions.computer`: the computer's lanes and points, the routes under
// `api/v1/host/`), host-actions.js (the player's buttons and the hops) and navigation.js (showing a
// device, signing out). This file joins them and names the default target.

import { createAutoEqActions } from './autoeq-actions.js';
import { createComputerActions } from './computer-actions.js';
import { createPending } from './computer.js';
import { createDeviceActions } from './device-actions.js';
import { createDeviceConfigActions } from './device-config-actions.js';
import { createHostActions } from './host-actions.js';
import { createNavigation } from './navigation.js';
import { createVolumeActions } from './volume-actions.js';
import { createWalkActions } from './walk-actions.js';
import { createWrites, mayWrite } from './write.js';

/** The actions over `store`, writing with `post` (api.js's), saying things with `toast`, moving the
 * address with `history` and loading pages with `location`. `computer` (the core's, optional) keeps
 * the point in hand, which a removal of that point clears; `needs()` gives the feeds the page asks
 * the stream for (the shown device's strip is read with its model only when `strip` is one). */
export function createActions({
  store, post, toast, history, location, pending = null, ask = null, timing = {}, computer = null,
  needs = () => [],
}) {
  const held = pending ?? createPending({ store, fetch: async () => ({ status: 0 }) });
  const writes = createWrites({ store, post, toast });
  const computerActions = createComputerActions({
    store, post, toast, held, writes, ask, computer, timing,
  });
  /** Every call to a device, each acting on the device its last `options` name (`{ device }`). */
  const onDevice = {
    ...createVolumeActions({ store, post, toast, writes }),
    ...createDeviceActions({ store, post, toast, writes }),
    ...createDeviceConfigActions({ store, post, writes }),
    ...createAutoEqActions({ store, post, writes }),
  };
  const { press, hops } = createHostActions({ store, post, writes });
  const { signOut, signInPrompt, showDevice } = createNavigation({
    store, post, toast, history, location, needs,
  });

  /** The default target: `options` with the device the page shows named in them when the caller
   * named none. The public actions below are their device-naming twins wrapped in it. */
  const aimed = (options) => ({ ...options, device: options?.device ?? store.shown() });

  /** The computer's actions: the preset calls name the device whose list they act on, the shown one
   * for a caller that named none. */
  const computerOn = {
    ...computerActions,
    recallPreset: (name, options) => computerActions.recallPreset(name, aimed(options)),
    renamePreset: (from, to, options) => computerActions.renamePreset(from, to, aimed(options)),
    deletePreset: (name, options) => computerActions.deletePreset(name, aimed(options)),
    savePreset: (name, options) => computerActions.savePreset(name, aimed(options)),
  };

  return {
    computer: computerOn,

    /** The device walk's calls (walk-actions.js): each but `start` names its job, `{ job }`. */
    walk: createWalkActions({ store, post, writes }),

    /** A tap that cannot write: a guest's or a read-only page's is turned away and says why (the
     * answer it would be given); `null` when the page may write, and nothing is said. */
    refuse: () => writes.turnedAway(),

    /** Whether the page may write: false for a guest and for a page whose writes are off. Pure:
     * nothing is said and nothing is sent (a theme draws its controls inert from it; `refuse`
     * says why when one is pressed anyway). */
    canWrite: (page = store.page()) => mayWrite(page),

    setVolume: (raw, options) => onDevice.setVolume(raw, aimed(options)),
    stopClimb: (options) => onDevice.stopClimb(aimed(options)),
    setControl: (key, body, options) => onDevice.setControl(key, body, aimed(options)),
    setMode: (mode, options) => onDevice.setMode(mode, aimed(options)),
    setBand: (band, param, body, options) => onDevice.setBand(band, param, body, aimed(options)),
    setLoudness: (body, options) => onDevice.setLoudness(body, aimed(options)),
    recallPreset: (name, options) => onDevice.recallPreset(name, aimed(options)),
    savePreset: (name, options) => onDevice.savePreset(name, aimed(options)),
    renamePreset: (from, to, options) => onDevice.renamePreset(from, to, aimed(options)),
    deletePreset: (name, options) => onDevice.deletePreset(name, aimed(options)),
    recallSlot: (slot, options) => onDevice.recallSlot(slot, aimed(options)),
    saveSlot: (slot, confirmed, options) => onDevice.saveSlot(slot, confirmed, aimed(options)),
    applySetting: (name, options) => onDevice.applySetting(name, aimed(options)),
    connect: (options) => onDevice.connect(aimed(options)),
    identify: (options) => onDevice.identify(aimed(options)),

    /** The configuration's writes to a device. Their `options` go on as they are: the caller names
     * the device (`{ device }`), and a call that names none sends nothing. */
    renameDevice: (name, options) => onDevice.renameDevice(name, options),
    setProfile: (profile, options) => onDevice.setProfile(profile, options),
    setPassphrase: (passphrase, options) => onDevice.setPassphrase(passphrase, options),
    makeDefault: (options) => onDevice.makeDefault(options),
    removeDevice: (options) => onDevice.removeDevice(options),

    /** The EQ library (autoeq-actions.js). The search and the upload are about no side; each call
     * about one names it in its last argument, `{ device }` or `{ computer: true }`, and one that
     * names none sends nothing. */
    autoEqSearch: (query, request) => onDevice.autoEqSearch(query, request),
    autoEqUpload: (text, name, request) => onDevice.autoEqUpload(text, name, request),
    autoEqPreview: (variant, options) => onDevice.autoEqPreview(variant, options),
    autoEqImport: (variant, options) => onDevice.autoEqImport(variant, options),
    overwritePreset: (name, options) => onDevice.overwritePreset(name, options),

    /** A transport button of the host's player. */
    press,

    /** End this browser's sign-in and load the page as a visitor. */
    signOut,

    /** Say that a change needs a sign-in, with the way to one (a tap on an inert control). */
    signInPrompt,

    /** Show another device without loading the page: its model is read, the page replaced, the
     * address pushed (unless this is a step of the history). A device that cannot be shown
     * stays unchanged and says so. The newest call wins. */
    showDevice,

    /** The declared hops: each answer is the list as it now stands. */
    hops,
  };
}
