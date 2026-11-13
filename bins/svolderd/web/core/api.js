/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Talking to svolder: the sign-in ending, a JSON write, a read, the words of a
// refusal, and the one-at-a-time sender. The core's actions and store import these; nothing here
// touches the stream or the page's parts.

/** What a write that switched the amp's mode to EQ tells the person. */
export const MODE_SET_WORDS = 'Mode set to EQ';

/** The words of a request that got no answer at all. */
export const NO_ANSWER = 'svolder did not answer.';

/** Load the sign-in page: the sign-in has ended. */
export function signedOut() {
  location.assign('./');
}

/**
 * One request to the daemon: the answer's parsed JSON with its `status`. A 401 loads the sign-in
 * page (and is still answered); no answer at all is status 0 with the words NO_ANSWER, a refusal
 * for every caller, never a rejection; a request ended through `io.signal` is that answer with
 * `aborted: true`; a body that is no JSON leaves the status alone, so a non-JSON 200 answers
 * `{ status: 200 }`, like an empty one. `io` may give the `fetch`, the `signedOut` and the abort
 * `signal` to use (the page's own fetch and sign-in by default).
 */
async function call(path, init, io = {}) {
  const send = io.fetch ?? ((...args) => globalThis.fetch(...args));
  const out = io.signedOut ?? signedOut;
  let answer;
  try {
    answer = await send(path, {
      ...init, credentials: 'same-origin', ...(io.signal ? { signal: io.signal } : {}),
    });
  } catch {
    return io.signal?.aborted ? { status: 0, error: NO_ANSWER, aborted: true }
      : { status: 0, error: NO_ANSWER };
  }
  if (answer.status === 401) out();
  let json = {};
  try {
    json = await answer.json();
  } catch {
    json = {};
  }
  return { status: answer.status, ...json };
}

/** POST `body` as JSON to `path` (relative), answering the parsed JSON with `status`. */
export function post(path, body, io) {
  return call(path, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  }, io);
}

/** GET `path` (relative), answering the parsed JSON with `status`. */
export function get(path, io) {
  return call(path, {}, io);
}

/** The words of a refused write's answer (no answer at all is status 0), or nothing. */
export function refusal(answer) {
  const failed = answer.status === 0 || (answer.status >= 400 && answer.status !== 401);
  return failed ? answer.error || 'Refused.' : null;
}

/**
 * One write at a time, the newest waiting: `send(value)` runs `write(value)` unless one is in
 * flight, when it replaces the value waiting (the writer is newest-wins too). `write` answers
 * a refusal's words, or nothing; `refused` hears them, and drops what was waiting.
 */
export function newest(write, refused) {
  let waiting = null;
  let sending = false;
  return async (value) => {
    waiting = value;
    if (sending) return;
    sending = true;
    try {
      while (waiting !== null) {
        const next = waiting;
        waiting = null;
        let why = null;
        try {
          why = await write(next);
        } catch {
          why = NO_ANSWER;
        }
        if (why) {
          waiting = null;
          refused(why);
        }
      }
    } finally {
      sending = false;
    }
  };
}
