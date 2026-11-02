/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The sign-in page: a browser already signed in (one that arrived from another site, whose
// SameSite=Strict cookie the navigation did not carry) goes straight to the page; the token
// form, a pairing code or a link's code signs one in; Show and Hide uncover the token while it
// is typed.

// A link's code rides in the fragment, which never reaches a server: it is dropped from the
// address bar before anything else, then spent.
const link = location.hash.match(/^#login=([A-Za-z0-9_-]{22})$/);
const garbled = location.hash.startsWith('#login=') && !link;
if (location.hash) history.replaceState(null, '', location.pathname + location.search);

const said = document.getElementById('said');

/** Say `text` under the forms, as a warning when `warn`. */
function say(text, warn = true) {
  said.textContent = text;
  said.classList.toggle('warn', warn);
}

/** POST `body` as JSON to `path`; the answer's JSON, with `ok` from its status. No answer at
 * all is not `ok` and says so: the forms never meet a rejection. */
async function post(path, body) {
  let answer;
  try {
    answer = await fetch(path, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
      credentials: 'same-origin',
    });
  } catch {
    return { ok: false, error: 'svolder did not answer.' };
  }
  let json = {};
  try {
    json = await answer.json();
  } catch {
    json = {};
  }
  return { ok: answer.ok, ...json };
}

/** Go to the page, keeping the device it was asked for. */
function enter() {
  const device = new URLSearchParams(location.search).get('device');
  location.replace(device ? `./?device=${encodeURIComponent(device)}` : './');
}

/** A browser that is signed in already goes to the page; a guest asked for this page stays. */
async function checkSession() {
  try {
    const answer = await fetch('api/v1/session', { credentials: 'same-origin' });
    const session = await answer.json();
    if (session.signed_in) enter();
  } catch {
    // No answer: the forms are there to use.
  }
}

document.getElementById('show').addEventListener('click', (event) => {
  const field = document.getElementById('token');
  const hidden = field.type === 'password';
  field.type = hidden ? 'text' : 'password';
  event.currentTarget.textContent = hidden ? 'Hide' : 'Show';
});

document.getElementById('token-form').addEventListener('submit', async (event) => {
  event.preventDefault();
  const token = document.getElementById('token').value.trim();
  if (!token) return say('Paste the token first.');
  const answer = await post('login', { token });
  if (answer.ok) enter();
  else say(answer.error || 'The sign-in failed.');
});

document.getElementById('pair-form').addEventListener('submit', async (event) => {
  event.preventDefault();
  const code = document.getElementById('code').value.replace(/\s+/g, '');
  if (!/^\d{6}$/.test(code)) return say('The code is six digits.');
  const answer = await post('pair', { code });
  if (answer.ok) enter();
  else say(answer.error || 'The pairing failed.');
});

if (link) {
  post('login', { link: link[1] }).then((answer) => {
    if (answer.ok) enter();
    else {
      say(answer.error || 'The link did not sign this browser in.');
      checkSession();
    }
  });
} else {
  if (garbled) say('This link is not whole. Make another with `svolder web`.');
  checkSession();
}
