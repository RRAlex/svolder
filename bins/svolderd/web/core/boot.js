/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The page's start: the core is made from the data blocks the server wrote, a link's code the page
// was opened with is spent, and the theme the server drew the frame for is mounted (the choice of
// theme the address and the browser hold is settled then, see switch.js); the stream opens once it
// is. `core` is exported for whoever needs the page's core (a check reaches it with
// `import(new URL('assets/core/boot.js', document.baseURI)).then((m) => m.core)`), `mounted`
// settles when the theme is on show.

import { createCore } from './core.js';
import { createSwitcher } from './switch.js';
import { toast } from './toast.js';

/** The JSON of the data block `id`, or `null` when there is none or it is not JSON. */
function block(id) {
  try {
    return JSON.parse(document.getElementById(id)?.textContent ?? 'null');
  } catch {
    return null;
  }
}

// A link's code rides in the fragment, which never reaches a server: it is dropped from the
// address bar before anything else. A link lands here as well as on the sign-in page (on a
// guest's page with guest = "read", or on a browser already signed in), so the page spends it
// too; see `arrive`.
const link = location.hash.match(/^#login=([A-Za-z0-9_-]{22})$/);
if (location.hash) history.replaceState(null, '', location.pathname + location.search);

let switcher = null;
// The scheme the server drew the frame in, before anything can change it.
const painted = document.documentElement.dataset.scheme ?? null;

export const core = createCore({
  page: block('svolder-page'),
  themes: block('svolder-themes') ?? [],
  window,
  painted,
  pick: (id) => switcher.to(id),
});
switcher = createSwitcher({ core, window, root: document.getElementById('theme') });

// The entry the page opened on knows its device, so a step back to it shows that device whatever
// its address says.
history.replaceState({ device: core.model.shown() }, '', location.href);

/**
 * Spend a link the page was opened with, and on a guest's page ask the session once: a browser
 * signed in that arrives from another site sends no Strict cookie, so it is served as a guest
 * until it reloads. Either way a sign-in reloads the page as it is.
 */
async function arrive() {
  let session = {};
  try {
    session = await (await fetch('api/v1/session', { credentials: 'same-origin' })).json();
  } catch {
    return;
  }
  if (link && !session.signed_in) {
    let answer;
    let said = {};
    try {
      answer = await fetch('login', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ link: link[1] }),
        credentials: 'same-origin',
      });
      if (answer.ok) {
        location.replace(location.pathname + location.search);
        return;
      }
      said = await answer.json();
    } catch {
      said = { error: answer ? undefined : 'svolder did not answer.' };
    }
    toast(said.error || 'The link did not sign this browser in.',
      { label: 'Sign in', href: './?sign-in' });
  } else if (document.body.classList.contains('guest') && session.signed_in) {
    location.replace(location.pathname + location.search);
  }
}

/** Mount the theme the frame was drawn for, then open the stream the theme asks for. */
async function start() {
  const mounted = await switcher.start(document.documentElement.dataset.theme);
  if (mounted) core.stream.open();
  return mounted;
}

export const mounted = start();
arrive();
