/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Talking to svolder: the sign-in ending, a JSON write and the words of its refusal, and the
// one-at-a-time sender. The core's actions and store import these; nothing here touches the stream or
// the page's parts.

/** What a write that switched the amp's mode to EQ tells the person. */
export const MODE_SET_WORDS = 'Mode set to EQ';

/** The words of a request that got no answer at all. */
export const NO_ANSWER = 'svolder did not answer.';

/** Load the sign-in page: the sign-in has ended. */
export function signedOut() {
  location.assign('./');
}

/** POST `body` as JSON to `path` (relative), answering the parsed JSON with `status`. */
export async function post(path, body) {
  let answer;
  try {
    answer = await fetch(path, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
      credentials: 'same-origin',
    });
  } catch {
    // Status 0 is no answer at all: a refusal for every caller, never a rejection.
    return { status: 0, error: NO_ANSWER };
  }
  if (answer.status === 401) signedOut();
  let json = {};
  try {
    json = await answer.json();
  } catch {
    json = {};
  }
  return { status: answer.status, ...json };
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
