<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->
# The EQ library popup

`core/popups/eq-library/` is a popup of the menu, a writer's: the row *EQ library…* stands
between *Devices…* and *Help…*, and a guest or a page with `[web] writes` off has neither the row
nor a way in ([popups.md](popups.md#the-list-of-popups)). It is where a person finds a headphone's
correction in AutoEq, sees how well the bands of the device (or the computer's EQ) can follow it,
saves it as a preset, saves a parametric EQ file as one, and writes the curve that is on over a
preset of their own. The daemon does every piece of work ([autoeq.md](autoeq.md)); the page asks it
and draws its numbers. Recall, rename and delete stay where each theme has them.

## The side

The popup is about one side of the EQ, the one the page shows when it opens: the device on show (its
name is the target of every call) or the computer (`core.computer.side()`). The title names it in
every draw, `EQ library · Kara` or `EQ library · This computer`, because the title of the last
draw is the one the frame keeps. It is not a `stays` popup: a change of the shown device closes it
(the router's path for every popup that is about the shown device), and the side cannot flip under
it because the page is inert. A model that changes the side anyway (the computer became
unavailable) draws the popup again for the side now: the search and its answer are kept, the
preview, the import and the overwrite of the other side are dropped and their requests stopped.

Every call that is about a side names it in the call itself, `{ device }` (the device's URL segment,
sent as its name) or `{ computer: true }`; one that names neither sends nothing (`no_target`). The
shown device is never taken in its place. `make theme-css-check` rule 6 reads the calls of the
`autoEq*` actions and accepts `{ computer: true }` as naming a side for them alone.

## Pages

The side list holds three pages; a wide window shows the first at the list.

**AutoEq** is the search, the preview of the chosen measurement and the import.

- *Search field.* A form with a search input (60 characters at most, the daemon's limit) and a
  button. Typing sends after a pause of `DEBOUNCE_MS` (300 ms); Enter and the button send at once; a
  field in which an input method is composing sends nothing until the composition ends. An empty
  query is no search. The field is never drawn again by a put, so what is typed and the caret
  stay.
- *Answers that arrive late.* Each search has a turn and an `AbortController`. A newer search aborts
  the one in flight and an answer that is not the newest turn's is dropped, whether or not the
  browser could stop it. An aborted request is no failure and says nothing. The daemon's answer
  carries no query, so the turn is how an answer is matched to the one it answers.
- *The status line* (a `role="status"` region) says what the field holds: the hint, `Searching…`,
  the count (`2 headphones`, then `14 more headphones match: narrow the search.` when the daemon
  left some out past 200), `Nothing in AutoEq matches “zzz”.`, or the refusal as words. Under it,
  the index's size and the date its copy was fetched, and the daemon's `note` when a refresh failed
  and the cached copy was used.
- *Results* group by headphone, as the daemon groups them (the exact normalised name, never
  guessed): a `<details>` for each, its summary the headphone and how many measurements it has,
  its body a button for each measurement, `source · form[ · rig]`, AutoEq's recommended one first
  and tagged. A disclosure is open when the answer has three headphones or fewer or when it holds
  the chosen measurement; one the person opens or closes keeps its state until the next answer.

**Preview.** Choosing a measurement sends `preview` for the side and draws the answer under the
results: a graph, the numbers, and a chip. Choosing another measurement aborts the request of the
first, and an answer for a measurement that is not the chosen one is dropped, in either order.

- *Graph.* `core.plot` draws the daemon's numbers on its grid and computes none: the 0 dB line and
  the grid, the gap between the fit and the target as a shaded area (`plot.gapSpans` over the
  spans the daemon lists, never a band of tolerance), the target dashed (`fp-graph-target`), the fit
  (`fp-graph-series`), and the second measurement's target dotted (`fp-graph-compare`). The gap's
  fill is `--frame-graph-gap` and the second target's stroke `--frame-graph-compare`; every theme
  sets both, in each scheme it has. A key under the graph names each line, so the lines are told
  apart by their dash and shade as well as their colour.
- *Numbers.* For a device: `Fit` (the RMS miss), `Worst miss` with where it is (`3.1 dB at 6.3 kHz`)
  and, when the fitted bands boost (`peak_db` at least `PEAK_SHOWN_DB`), `Lower the volume by 2.4
  dB`. For the computer there is no fit: the graph is the correction as the computer plays it, the
  preamp is a row and a line says how far it boosts.
- *Compare with…* is a chip in the page pane (the pane is the place for lists, so the panel has
  room; the head holds no list). It opens a list (the core's `list.js`, [frame.md](frame.md#modals))
  of the other measurements of the same headphone; a pick asks for the preview again with `compare`,
  draws the second target and gives its numbers under a heading of its own. *Stop comparing* asks
  for the one alone.

**Import.** Under the preview, a form: the name field and *Save as a preset*. The field holds the
headphone's name and is editable; the name is sent only once it differs from that, so the daemon
shortens the headphone's own name itself. Import only saves: it never recalls.

- *Saved* says what was saved and that it is not on: `Saved “Sennheiser HD 650” (6 filters). It is
  saved only: it is not on Kara yet. Recall it where the presets are.` and how many lines of the
  file the parser left out.
- *Taken.* The daemon's `taken` is worded at the field: `aria-invalid="true"`, the field described
  by the line, the focus on it, what was typed kept, and a button that offers `NAME (source)` (the
  TUI's suggestion), which fills the field. `bad_name` reads the daemon's own words.
- A name that is empty sends nothing. A refusal that is not about the name keeps the focus on
  *Save*.
- The saved list is read from the store at every put, so the answer and the model's event that lists
  the new preset reach the page in either order.

**Upload a file** reads a file in the page and sends its text. The file input takes `.txt`
and text (Equalizer APO parametric text, the format `svolder eq import` reads); a file over 64
KiB (the daemon's limit) is refused in the page with no request. The preset is named after the
file, and the name is editable; a name typed while the file is read is kept. *Save the file as a
preset* is `aria-disabled` while there is no file, not `disabled`, so it stays a stop of the
keyboard and the line under it says why (`Choose a file first.`); pressed anyway it says it again.
A parse refusal is shown as the daemon words it. The input and the name's form are not put together:
the file input is drawn once, so the chosen file stays through every put.

**Saved presets** lists the presets of the side, each with where it lives and *Overwrite with
current…*.

- *Where it lives.* `In the library`, or `On Kara now` for the preset the device's bands match (the
  tone view's `PresetStatus`, `on` on the row), with a `Built in` or `Imported` tag. The page never
  says a preset is saved on the device: the device does not say which preset a slot holds,
  so no mark could be true.
- *Overwrite with current…* asks once, in an inline question with *Keep it* focused, and sends
  `POST api/v1/devices/DEVICE/presets` with `force`. It is `aria-disabled` and described by its
  reason (the TUI's `u`, in the same words): a built-in preset (`Flat is built in: save the curve
  under a new name.`), an imported one (`imported`, a field of the preset's row), the computer's
  side (`The computer's points are saved as a new preset, not over one.`), and no live curve: the
  device is not connected, it has no tone control, or its tone control has not been read yet. A
  reason that holds for every row stands once above the list and describes each button; one that is
  the row's own stands under it. A refusal is worded under the list and the question stays.
- The list is not drawn while its question is open (the question lives in it); it is drawn when the
  question closes.
- On the computer side the rows are the computer's presets (`host.computer.presets`); a preset the
  computer cannot hold says why in the daemon's words (`playable`, `why`), and Overwrite is
  disabled on every row.

## Routes each control sends

| control | route | body |
|---|---|---|
| search | `POST api/v1/autoeq/search` | `{ query }` |
| a measurement, *Compare with…* | `POST api/v1/autoeq/preview` | `{ variant, target, compare? }` |
| *Save as a preset* | `POST api/v1/autoeq/import` | `{ variant, target, name? }` |
| *Save the file as a preset* | `POST api/v1/autoeq/upload` | `{ text, name }` |
| *Overwrite* (yes) | `POST api/v1/devices/DEVICE/presets` | `{ name, force: true }` |

`variant` is the four keys of a measurement the search answered (`name`, `source`, `form`, `rig`),
`target` is `{ "side": "device", "device": NAME }` or `{ "side": "computer" }`. The calls are
`autoEqSearch`, `autoEqPreview`, `autoEqImport`, `autoEqUpload` and `overwritePreset` of
`core/autoeq-actions.js`; each answers `{ ok, status, ... }` or `{ ok: false, reason, error }` and
sends a `signal` that ends the request when the popup closes or a newer one replaces it.

## Refusals

Every refusal reason is words a person reads, at the control that made the request: `busy` (other
downloads are running), `not_listed` (the index lists it and the file is not there), `unreachable`,
`taken`, `bad_name` and `invalid` (the daemon's own words for what is wrong with a name, a file or
a correction too long for the computer), `missing`, `not_found_yet`, `not_connected`, `read_only`,
`file_busy`, `stopping`, `internal`, `too_large`, and the page's own `no_answer`, `signed_out`,
`forbidden` and `no_target`.

## Checks

`node/core-eq-library.mjs` runs the draws and the wire on the real core over a jsdom page and a fake
daemon (the page's `fetch`): each case has a device on show with neighbours, and the cases read each
request. It holds the debounce, a stale answer dropped in either order with the abort delivered or
not, the import's two orders, each refusal and the focus. `eq_library.py` runs it in a browser, in
every theme, wide and at 390 px, against a daemon whose `SVOLDER_AUTOEQ_URL` is a loopback server of
two headphones' files under `scripts/web-checks/fixtures/autoeq/`: the numbers the page words are
compared with the daemon's own preview route, the graph's colours with the theme's tokens, every
preset a case makes is removed. `popups.py` has the popup's row, and no check reaches AutoEq's
servers.
