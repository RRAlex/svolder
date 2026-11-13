/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Moving the page: ending this browser's sign-in, the prompt a tap on an inert control gives, and
// showing another device without loading the page.

import { refusal } from './api.js';
import { SIGN_IN, SIGN_IN_WORDS } from './write.js';

const NOT_SHOWN = 'That device could not be shown.';

/**
 * The navigation over `store`, posting with `post`, saying things with `toast`, moving the address
 * with `history` and loading pages with `location`; `needs()` gives the feeds the page asks the
 * stream for (the shown device's strip is read with its model only when `strip` is one).
 */
export function createNavigation({ store, post, toast, history, location, needs }) {
  /** What shows the newest `showDevice` call: an older call that answers later gives way. */
  let showing = 0;

  const navigation = {
    /** End this browser's sign-in and load the page as a visitor. */
    async signOut() {
      const answer = await post('logout', {});
      const why = refusal(answer);
      if (why) {
        toast(why);
        return { ok: false, ...answer, error: why };
      }
      location.assign('./');
      return { ok: true, ...answer };
    },

    /** Say that a change needs a sign-in, with the way to one (a tap on an inert control). */
    signInPrompt() {
      toast(SIGN_IN_WORDS, SIGN_IN);
    },

    /** Show another device without loading the page: its model is read, the page replaced, the
     * address pushed; with `push: false` (a step of the history, a device the fleet dropped, a
     * switch from a popup that stays) no entry is pushed and the entry the page is on is told
     * its device (`history.shown`). A device that cannot be shown stays unchanged and says so.
     * With `quiet: true` a failure is not toasted. The newest call wins. */
    async showDevice(url, { push = true, quiet = false } = {}) {
      const mine = ++showing;
      const answer = await store.read(url, { strip: needs().includes('strip') });
      if (mine !== showing) return { ok: false, status: 0, superseded: true };
      if (answer.status === 401) return { ok: false, status: 401 };
      if (answer.status !== 200 || !answer.device) {
        const words = answer.status === 0 ? answer.error : (answer.error || NOT_SHOWN);
        if (!quiet) toast(words);
        return { ok: false, status: answer.status, error: words };
      }
      const page = store.page();
      const { [url]: _, ...followed } = page.followed ?? {};
      store.replace({
        ...page,
        shown: url,
        device: answer.device,
        followed,
        card: null,
        strip: answer.strip ?? null,
      });
      if (push) history.pushState({ device: url }, '', `./?device=${url}`);
      else history.shown?.(url, `./?device=${url}`);
      return { ok: true, status: 200 };
    },
  };

  return navigation;
}
