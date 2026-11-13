<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->
# The Devices popup's *+ Add a device*

The side list of the Devices popup ([devices-popup.md](devices-popup.md)) ends with *+ Add a
device*: the daemon's device walk ([device-walk.md](device-walk.md)) drawn on the web page, so a
fresh install adds its first device from a phone with no terminal. The page draws what the daemon
sends and nothing of its own: the prompt's title, its body lines, its choices, a text or a secret
field as the prompt says it takes, the default an empty line stands for, and the words the daemon
gave for a refusal. It has no copy of the walk: no default name, no validation, no step logic. The
few words that are the page's (what a button is called, that the daemon is working) are in
`WALK_WORDS` of `walk.js`.

The code is `core/walk-actions.js` (the calls), `core/popups/devices/walk.js` (the draws),
`walk-wire.js` (the wire) and the `fpw-*` rules of `core/frame.css`, which read only the
`--frame-*` tokens.

## The calls

`actions.walk` has `start()`, `answer({ choice } or { text }, { job })`, `back({ job })` and
`cancel({ job })`, each one POST of [the walk's routes](../usage/web-ui.md). Every call but `start`
names its job and sends nothing without one (`no_job`): the shown walk is never taken in its
place, and rule 6 of `scripts/theme-css-check.py` reads a popup's files for `{ job }` as it reads
them for `{ device }`. A call resolves `{ ok, status, job? }` or `{ ok: false, status, reason,
error }`, `reason` one of `WALK_REFUSALS`: the daemon's `busy`, `stopping`, `invalid`,
`not_waiting` and `no_such_job`, and the page's `signed_out`, `forbidden`, `no_answer`, `no_job`
and `failed`. The three walk reasons are in `DEVICE_REFUSALS` too, the one closed list of the
configuration's writes. A passphrase is in the body only; words of a refusal that quote it are
not handed on.

## The page

Five places, each put alone (`popup-devices-walk-` `head`, `rows`, `controls`, `status`,
`refusal`), so what is typed and the focus stay when a state changes only another place:

- **Prompt**: the title and the body lines; for a text or secret prompt the last line is the
  default, shown as the field's hint, never as its value.
- **Controls** by `takes`: `choice` is a button per choice (the unit a choice names and that is
  configured is not offered: `aria-disabled`); `text` a field and *Continue*; `secret` a password
  field never filled, with a *Show* toggle (`aria-pressed`), sent in the body only. *Back* is there
  when the walk can step back and *Stop adding* always. A walk that works (`phase: working`) shuts
  them with `aria-disabled` and `readonly` (so the focus stays) and the status line says what it
  does; the controls also stay shut from an answer until the state that follows it.
- **Rows**: while the walk is at its scan, one row per unit heard, keyed by its unit id, with
  product, signal and, for a configured unit, which device it is.
- **The warning**: the walk's first prompt when any configured device holds a link names it
  (`Go on`, `Cancel`); the safe answer, the last choice, has the focus.
- **Ended**: the status line (a live region, so the outcome is read) says the device's name and
  its link as the fleet reports it, or the daemon's words for a cancel, a failure or a quiet scan;
  the controls are *Open its page* (a `router.step`, a gesture) and *Start again*.

A step takes the focus when its key (step, title, kind) changes, and a refusal takes it back to
the field it is about. A put of the controls keeps what is typed in the same field; a code is
dropped when its answer was refused and is cleared as it is sent. No step is a history push: the
router's gesture warning stays silent.

## Whose walk it is

Entering the page starts a walk (`start`) and the page owns its job: it answers it and stops it
by leaving, from the moment the start answered, even before a state has come. The job is kept in
the tab's session store, so a reload takes the same walk up again where it stands. Another walk
running answers `busy`; the page then looks at the states the stream replays (`needs=walk`, the
popup's second want, `popup:devices-walk`): a walk that is not this tab's (the terminal's, a
socket's, another tab's) is shown read-only, as words: who started it, its step's title, nothing to
press (a walk of this page's origin that is not this tab's reads "started in another browser
tab"; a duplicated tab copies the session store, so both copies take the walk as theirs), and it
is never stopped from here. A page left on its card (ended, or a refused start) does
not follow a walk that starts later: *Start again* joins. The stream replays the last state of up
to sixteen ended walks when it opens, and each walk's last again at every reopening: a state told
again changes nothing, and only the walk in hand shows how it ended. Job ids restart with each
daemon run, so every walk event and the start's answer carry the run's stamp: the page keys the
walks it holds by it, forgets them all when it changes (the session store holds `run:job`), and
names the run in its answer, back and cancel, which the daemon refuses `no_such_job` when it is
another's. A busy start with no walk seen while the stream is closed or lost says so in the status
line and starts again when the stream opens. A start that answers after the page was left is
cancelled at once, and a walk the page stopped by leaving is waited for, never taken over.

Leaving the page (another entry, Back, Escape, the popup closed) cancels the walk this page
started, by its job. A walk it only watches is left alone.

The answer and the stream's state come in either order. Both are whole; the page applies
whichever comes and keeps the controls shut until the state after an answer. A job the daemon no
longer knows (`no_such_job`) is said once with *Start again*; nothing is retried.

## The first success

With no device configured the walk starts at its scan (nothing is linked, no warning). When it
writes the first device and the page behind shows none, the page shows the new device (`showDevice`
with no push) once the fleet lists it, and the empty card goes. The popup reads its list again
when a walk wrote a device, so the side list and the done card gain it.

## Checks

`node/core-device-walk.mjs` holds the calls, the draws over fixtures copied from svolderd's own
`walk` events (`node/fixtures/device-walk.json`), and the wire on the real core over a jsdom page
(the orders, the 404, another origin, leaving, a reload, the first device). `devices_popup.py`
walks a real daemon on the fake transport in a browser: the warning, the rows, Escape and a
phone's Back, a whole walk to *Open its page*, a reload, a walk of the socket, a fresh install,
a guest's and a read-only page's no add page. The fake cannot refuse the advertised code, so a wrong
passphrase is node's.
