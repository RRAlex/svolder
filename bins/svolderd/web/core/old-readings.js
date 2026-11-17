/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The mark on values that are from before a link dropped. While the shown device's link is down
// (the lost-link card stands over its readings) or the page lost its stream (the daemon is gone),
// what every theme shows of the device is the last it was told, and a badge of the frame says so:
// fixed at the bottom left, out of the flow, so nothing around it moves, and dressed from the
// `--frame-*` tokens each theme sets (frame.css), so a theme has no logic of its own for it. The
// first fresh reading (the card going, the stream opening) takes it away. `<html>` carries
// `data-readings-old` while it shows, for a theme that wants to hold something back as well.

/** The words the badge's title and its accessible name carry. */
export const OLD_WORDS = 'readings from before the link dropped';
/** The words it shows. */
export const OLD_SHORT = 'old readings';

/** Whether what the page shows of the device `url` (the shown one by default) is from before a link
 * dropped: its link is down, or the daemon has not heard it since the link came back (`link.old`), or
 * the page's stream is lost while the device is shown. A device with no readings (the `Looking for...`
 * card, a refused passphrase) has none to be old. `store` is the page's store. */
export function readingsOld(store, url = store.shown()) {
  const device = store.device(url);
  if (url === store.shown() && store.state() === 'lost' && deviceHeard(device)) return true;
  return deviceOld(device);
}

/** Whether a device's model has readings at all: not the `Looking for...` card, not a refusal. */
const deviceHeard = (device) => Boolean(device) && device.card?.kind !== 'dim';

/** Whether the model of one device (shown or followed) says its readings are from before a drop. */
export const deviceOld = (device) => deviceHeard(device) && device.link?.old === true;

/**
 * Make the badge in `root` (the frame's element) and keep it in step with `store`. Returns
 * `{ el, sync }`; `sync()` reads the store again.
 */
export function createOldBadge({ document, root, store }) {
  const el = document.createElement('div');
  el.id = 'fx-old';
  el.className = 'fxold';
  el.title = OLD_WORDS;
  el.hidden = true;
  // The words are announced by a polite live region that is in the page before they change.
  const say = document.createElement('div');
  say.className = 'fxsr';
  say.setAttribute('role', 'status');
  say.setAttribute('aria-live', 'polite');
  const text = document.createElement('span');
  text.setAttribute('aria-hidden', 'true');
  text.textContent = OLD_SHORT;
  el.append(text);
  root.append(el, say);

  function sync() {
    const old = readingsOld(store);
    el.hidden = !old;
    say.textContent = old ? OLD_WORDS : '';
    if (old) document.documentElement.dataset.readingsOld = 'true';
    else delete document.documentElement.dataset.readingsOld;
  }

  for (const event of ['model', 'device', 'part', 'state']) store.on(event, sync);
  sync();
  return { el, sync };
}
