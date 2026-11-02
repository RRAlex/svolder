/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The host's analysis frames (`GET api/v1/host/frames`) for any theme that draws the music: one
// stream shared by everyone holding it, open while at least one holds it. A holder gives what it
// hears (`hello` with the daemon's clock, `capture` with the capture's state, `frame` with a
// frame's text as it came); a holder that joins late is told the last `hello` and `capture` at
// once. A stream the browser gave up on is asked for again, after a wait that doubles at each
// failure and starts over at a good open.

/** The first wait before asking again, and the longest, in milliseconds. */
const RETRY_MS = 3000;
const RETRY_MAX_MS = 30000;

/** Parse the JSON of a stream event, or nothing for text that is not. */
function parse(text) {
  try {
    return JSON.parse(text);
  } catch {
    return null;
  }
}

/** The analysis over `EventSource` at `url`. Returns `{ open(listeners), holders }`; `open`
 * returns `{ close() }`. */
export function createAnalysis({ EventSource, url = 'api/v1/host/frames' }) {
  const holders = new Set();
  let source = null;
  let retry = 0;
  let retryIn = RETRY_MS;
  let hello = null;
  let capture = null;

  /** Tell one holder; one that throws does not stop the rest. */
  function tell(holder, name, data) {
    try {
      holder[name]?.(data);
    } catch (error) {
      globalThis.console?.error?.('svolder: a holder of the analysis threw', error);
    }
  }

  function each(name, data) {
    for (const holder of [...holders]) tell(holder, name, data);
  }

  function shut() {
    clearTimeout(retry);
    retry = 0;
    source?.close();
    source = null;
    hello = null;
    capture = null;
  }

  function connect() {
    shut();
    const mine = new EventSource(url);
    source = mine;
    mine.addEventListener('hello', (event) => {
      const data = parse(event.data);
      if (!data || !Number.isFinite(data.now_ms)) return;
      hello = data;
      capture = data;
      each('hello', data);
      each('capture', data);
    });
    mine.addEventListener('capture', (event) => {
      const data = parse(event.data);
      if (!data) return;
      capture = data;
      each('capture', data);
    });
    mine.addEventListener('frame', (event) => each('frame', event.data));
    mine.addEventListener('open', () => { retryIn = RETRY_MS; });
    mine.addEventListener('error', () => {
      if (source !== mine || mine.readyState !== EventSource.CLOSED) return;
      source = null;
      retry = setTimeout(() => {
        retry = 0;
        if (holders.size > 0 && !source) connect();
      }, retryIn);
      retryIn = Math.min(RETRY_MAX_MS, retryIn * 2);
    });
  }

  return {
    get holders() { return holders.size; },

    /** Hold the stream, hearing with `{ hello, capture, frame }` (each optional). */
    open(listeners) {
      const holder = { ...listeners };
      holders.add(holder);
      if (holders.size === 1) {
        retryIn = RETRY_MS;
        connect();
      } else {
        if (hello) tell(holder, 'hello', hello);
        if (capture) tell(holder, 'capture', capture);
      }
      return {
        close() {
          if (!holders.delete(holder)) return;
          if (holders.size === 0) shut();
        },
      };
    },
  };
}
