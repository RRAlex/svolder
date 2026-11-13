<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->
# The Devices popup

Devices is one of the page's popups ([popups.md](popups.md) has what every popup is): the menu's row
*Devices…*, above *Help…*, for everyone (read-only for a reader). It lists the configured devices,
shows every one's key graphs at once (the Overview) and switches the device the page shows. The code
is `bins/svolderd/web/core/popups/devices/` (`draw.js`, `wire.js`), its entry in
`core/popups/listed.js` and the `fpd-*` rules of `core/frame.css`.

A device's page also holds that device's settings: its name, its profile, its passphrase,
*Make default* and *Remove…*, each by its own control (below), then the device's facts and
*Identify* (the themes' fold-outs show only a foot, which opens this page). The last entry,
*+ Add a device*, draws the daemon's walk ([add-a-device-page.md](add-a-device-page.md)). Its
other action is *Show on the page*.

## What it shows

- **The side list**, drawn again at each draw: **Overview**, a break, then one entry per
  configured device with a dot (full: connected; a ring: not; a screen reader hears the words)
  and the tag **Shown** on the device the page shows, and last, after a break, **+ Add a device**
  (page `add`). An entry's page is `d-<url>`, the device's URL segment, so the page is about that
  device whatever the page behind shows.
- **Overview** (also the pane beside the list in a wide window): every configured device in the
  list's order, each a block with its name, its state (`connected`, or `not connected since
  HH:MM` in the viewer's time zone when the daemon knows since when) and its key graphs over the
  last 30 minutes. A device that is not connected is dimmed: its name takes `--frame-dim` and
  its graphs' strokes `--frame-offline`. No opacity is used, so the words keep the contrast the
  theme gave those tokens.
- **A device's page**: the same block for that one device, with *Show on the page* at the right
  end of the name's line (wrapping under the name on a phone), its settings between its state line
  and its key graphs, then its facts and All readings. On the shown device the button is absent and
  the place says `This is the device the page shows.`

## The key graphs

A device's graphs are the series the daemon answers for it with no `ids`: its key readings,
which the profile marks ([profiles.md](profiles.md)). `ordered()` of `draw.js` is the one place
that says their order: the signal strength first, then the readings exactly in the daemon's
order, which is the order of the device's profile. The profile decides: nothing is sorted by
kind on the page, so a product that wants its meters before its states lists them so.

Each graph is a `<figure>`: its name and its last reading above a plot drawn by `sparkline` of
`core/plot.js` in the frame's `fp-graph` box. The plot itself is hidden from a screen reader and
the box is one image (`role="img"`) whose label is the text alternative: the name, the span, the
last reading and the range (`Volume, last 30 minutes: now -26.1 dB, between -27.1 dB and -26.1
dB`). The range stands under the plot in sight too. A series with no reading says `No reading in
the last 30 minutes.`, and one with a single reading says so in words (`No change in the last
30 minutes.`, or for the signal `One reading in the last 30 minutes, at 14:02.`): neither draws
an empty box.

The series' points are raw readings. The page words them through the device's model, as the
volume and the sliders are worded everywhere: `stepWords` of `core/words.js` (the model's own
words for a step of a level), on or off for a switch, the unit the route gives for the signal
(`dBm`). The last reading of a connected device is its reading now, which the model words for a
readout (its text) and a choice (the option that is on); an earlier value of those two has no
words in the model and is drawn as its number. A control's reading holds until the next, so its
line is drawn in steps, from the span's start and, while the device is connected, on to now.

The signal has points only for the advertisements the daemon heard, so a connected device's
signal graph is often one reading from before its connect.

## Which route each part reads

| Part | Route | When |
| --- | --- | --- |
| the list, a state's `since` | `GET api/v1/devices` (a writer's view; a reader's list is the fleet's) | at the opening, and at each `fleet` event of the stream |
| a device's graphs | `GET api/v1/devices/{device}/series` | when its block comes into the pane, then every 10 s (`REREAD_MS`) while the popup stands and the stream is open, and when the stream opens again (the page wakes, or the link is found again) |
| the dot, the state line | the store: the device's `link` part, else the fleet's row | each `part`, `device`, `model` and `fleet` event |
| *Show on the page* | `GET api/v1/devices/{device}/model` (`core.actions.showDevice(url, { push: false })`) | on the press |
| the facts | the store's `facts` part of the device | each `part` of `facts`; read afresh from `GET api/v1/devices/{device}/model` once the page's device model is in the store, since the daemon sends the part again only when a value changes |
| *Identify* | `POST api/v1/devices/{device}/identify` (`core.actions.identify({ device })`) | on the press; the toast is the daemon's words with its gap |

A configured device's URL segment is its fleet row's (the row of the same name). Until the list
has answered, and when it cannot be read (the words are shown above the pane), the fleet's
devices stand in for it.

**Every request and every reading names its device.** The wire asks `store.shown()` only to put
the tag and to decide whether *Show on the page* is offered. `make theme-css-check` rule 6 reads
every popup's wire for a device action without a `{ device }` and a store read without the
device's address, and the wire's node cases run with another device on show than the page is
about and read each request.

## The settings' writes

The core has the five writes a device's page sends, in `core/device-config-actions.js`, as members
of `core.actions` beside the other device actions. The page calls all five.

| Call | Request | Body |
| --- | --- | --- |
| `renameDevice(name, { device })` | `POST api/v1/devices/{device}/edit` | `{ "name" }` |
| `setProfile(profile, { device })` | `POST api/v1/devices/{device}/edit` | `{ "profile" }`, `null` to take the pin off |
| `setPassphrase(passphrase, { device })` | `POST api/v1/devices/{device}/edit` | `{ "passphrase" }` |
| `makeDefault({ device })` | `POST api/v1/devices/{device}/default` | `{}` |
| `removeDevice({ device })` | `POST api/v1/devices/{device}/remove` | `{}` |

`device` is the device's URL segment. These calls have no default target: `core/actions.js` hands
their `options` on as they are, so one that names no device sends nothing and answers the reason
`no_device`, and the shown device is never taken in its place. Rule 6 reads them from
`core/actions.js` with the other device actions (every member that hands its `options` on), so a
popup's call without a `{ device }` fails `make theme-css-check`.

A write answers `{ ok: true, status, devices, profiles }`, the list route's answer after the
change, so the page draws from it with no second read. A refusal answers `{ ok: false, status,
reason, error }`: `error` is the daemon's words and `reason` one of `DEVICE_REFUSALS`, a closed
list the page words at the control. The daemon's reasons are `taken`, `missing`, `invalid`,
`read_only`, `file_busy` and `stopping`. The others are the page's: `signed_out` (a 401; the
sign-in page loads, as for every request), `forbidden` (a 403), `missing` again for a 404 that
gives no reason, `no_answer` (no answer at all), `no_device`, and `failed` for anything else.
Nothing is toasted and no model is read again. A guest's call and a call on a page whose writes
are off are turned away before any request, as every write is, with the toast of
`core/write.js` and the reasons `signed_out` and `forbidden`.

A passphrase is in the request's body and nowhere else: no address, toast, console line or
history entry holds it, and a refusal whose words quote it answers `The passphrase was not
changed.` in their place.

## The settings, drawn

`core/popups/devices/settings.js` draws the settings part of a device's page: a model in, a
string out; `bodyHtml` draws it on a device's page, between the state line and the key graphs
(`deviceHtml`'s `settings`), and never on the Overview. `settingsHtml(device, profiles,
state)` takes the device's entry of the devices answer, the answer's `profiles` and each
setting's state, and draws a heading and one row per setting (`rows` of the frame):

- **Name**: a form of one field, holding the current name, and *Save*. The field carries
  `aria-invalid` (`false` until a write is refused) and, while refused, `aria-describedby` the
  refusal's words.
- **Profile**: a button that names the current profile (its display name, else its name;
  `Automatic` with no pin) and opens a list. `core/list.js` builds its panel in the page pane,
  so the draw holds the opener alone (`aria-haspopup="listbox"`) and `profileOptions(device,
  profiles)` gives `openList` its choices: no pin first (the value is empty), then every profile
  that is no base. The profile a device is pinned to is among them whatever it is.
- **Passphrase**: its kind in words (set, none, automatic) and *Change*. While the state is
  `open` the place holds a form instead, which offers what the TUI's row does: a `fieldset` of
  three radios named `device-passphrase-kind`, *Automatic* (`auto`), *None* (`none`) and *A
  4-digit code* (`code`), so a wire reads the choice off the form. The device's kind is chosen
  when it is `auto` or `none`, a set one chooses nothing (its code is never drawn), and
  `choice` in the state stands over both. The code's part (`settingId('passphrase', 'code')`)
  is `hidden` and its controls disabled unless the code is chosen: a `type="password"` field,
  `required` then, with no `value`, ever, and `autocomplete="new-password"` (a password manager
  fills no stored one in), and a toggle with `aria-pressed` that shows what is typed
  (`revealed`: the field is `type="text"`). *Save* and *Cancel* end the form.
- **Default device**: *Make default*, or on the default the tag `Default`.
- **Remove device**: *Remove…*, which asks (in `REMOVE_ASK_ID`, beside the place) before it sends.

A setting's state is `{ status, words }`: none or `idle`, `sending` (that setting's controls
alone are disabled and its line says `Saving…`) or `refused` with its words. Each setting has a
line under its controls (`role="status"`, empty at rest) that holds either. What is typed is in
no model: the wire keeps it.

Every id is made of the setting, never of the device's name, which a rename changes:
`settingId(setting)` is the setting's place (`popup-devices-set-name`), which a wire puts alone
with `nameHtml`, `profileHtml`, `passphraseHtml` or `defaultHtml`, and `settingId(setting,
part)` a control in it (`field`, `save`, `open`, `change`, `kind-auto`, `kind-none`, `kind-code`,
`code`, `reveal`, `cancel`, `make`, `words`).
The draw uses the frame's classes alone, and no new token: a pressed toggle
(`.fi-btn[aria-pressed="true"]`) takes `--frame-accent`.

`refusalWords(setting, refusal)` words a refused write at its control. `taken` says another
device has the name; `invalid` says what is wrong for that setting (a passphrase is four
digits); `read_only`, `file_busy`, `stopping` and `no_answer` say what happened and whether
trying again helps; `signed_out`, `forbidden` and `missing` (`This device is no longer
configured.`) say so; anything else reads the daemon's words, or that the setting was not
changed. No passphrase is in any of them: a passphrase's refusal never hands on words that hold
four digits in a row.

## All readings, drawn

`core/popups/devices/readings.js` draws the disclosure that stands under a device's key graphs: a
model in, a string out. The wire puts it under the facts on a device's page. `readingsHtml(device,
state)` is a native `<details>` (`READINGS_ID`), closed unless `state.open`, whose `<summary>`
reads `All readings` and holds no heading. Inside is one place (`READINGS_BODY_ID`), empty while
the disclosure is closed, so nothing is worded for a table nobody sees. The wire
listens for `toggle` (which does not bubble: on the frame, in the capture phase) and puts
`readingsBodyHtml` there when the disclosure opens (and reads the facts afresh), empties it when it
closes, and puts it again at each `facts` part while it is open; the `<details>` is never put
again, so it stays as the person left it. Whether it is open is the person's and is kept through
every put of the page; a page for another device starts closed.

Open, the place holds a table of every reading in the device's `facts` part, in the daemon's
order: a caption (`Readings of Desk amp`), three column headers and one row per reading, whose
name is the row's header (the profile's label; the reading's id where the profile names none),
then its value and its age. The value is the daemon's words: it words each reading through the
device's profile (`63 (−26.1 dB)`), and a reading names its function, not a control of the
model, so `stepWords` has nothing to take. The converter is a parameter (`say(row, model)`); a
reading it has no words for keeps the daemon's. An age is `ageWords` of `core/words.js` between
the reading's stamp and the part's clock; its cell carries the stamp (`data-heard-at`) and the
table the clock (`data-now`), which are the selectors `READINGS_AGES` gives
`core.clock.tickAges`. A reading with no stamp shows `—`.

A device that is not connected shows the readings its model still holds under the caption `Last
known readings of Desk amp (not connected)`, on a table marked `data-last-known`. A device with
no reading has a line instead of a table, which says so, and for one that is not connected that
none is known.

The ids are the part's own and hold across a rename. The table has rules of its own in
`frame.css` (`fpd-readings`, `fpd-readings-title`, `fpd-readings-table`, `fpd-reading-age`),
dressed by `--frame-dim`, `--frame-line` and `--frame-accent`, which every theme sets; its cells
wrap, so nothing scrolls sideways.

## The settings, wired

`wire.js` holds each device's settings' states (`{ name, profile, passphrase, default }`, each as
above) by the device's unit id, which a rename does not change, and the answer's profiles. Each
setting's place is put alone with `frame.put` when its state changes, the stream's updates put
all four, and a place that holds the same already is left as it is.

| Control | Request | Notes |
| --- | --- | --- |
| name: Enter or *Save* | `renameDevice(name, { device })` | the name is trimmed; the same name sends nothing; an empty one is refused at the field without a request |
| profile: the button | `setProfile(profile \| null, { device })` | `openList` of `core/list.js` in the pane (the pane is a place for lists), the panel inside the box; Escape and the arrows reach it through the modal's `onKey`; the profile it has sends nothing; *Automatic* sends `null` |
| passphrase: *Change* | `setPassphrase('auto' \| 'none' \| four digits, { device })` | the form of the draw; the code is sent in the body only; a form with no choice or a code that is not four digits is not sent |
| *Make default* | `makeDefault({ device })` | on the device that is not the default |
| *Remove…* | `removeDevice({ device })` | asks once; see below |

Every call names the page's device (`device.url`), never the shown one. While a write is on its
way its place says `Saving…` and its controls are disabled; the answer's `devices` and `profiles`
take the place of a second read (the list, the Overview, the entries and the page are drawn
from it). A refusal reads its words at its control (`refusalWords`); a name refused as `invalid`
or `taken` reads the daemon's own words when it gave some. The focus returns to the control
after a save or a refusal: the name's field, the profile's button, the passphrase's *Change*
button (or its field, or *Save*, after a refusal), *Make default* after a refusal (after a
success the button is gone and the focus is where the frame's redraw put it).

**Typed text is kept by the wire, with no `core.freeze`**: before it puts a settings place, the
pane's body or draws the box, it takes the fields of the page that differ from their markup (the
name, the passphrase's code), and after it gives them back, but only into a page about the same
device (by unit id). So the reveal toggle, another setting's save, a stream update and a rename
elsewhere leave what is typed. The name field that was not typed in takes a name changed
elsewhere. Neither a typed passphrase nor any other is a value in the markup or in the state.

**A setting changed elsewhere** (another tab, the TUI) reaches a device's page through the list's
read: the page reads `api/v1/devices` again on the graphs' timer (every 10 s while the stream is
open), when the stream opens again and at each `fleet` event. The Overview does not, except while
its config line stands: the answer's `config_changed` (the daemon's words that `config.toml`
changed since the last reload) is put alone into `popup-devices-config`, and the timer reads again
until a reload clears it; a step back from a device's page reads once.

**The profile's list** is held open: the profile's place is not put while its list is open, and
it is put when the list closes. The body's put (the devices in the pane changed) closes the list
and the question first, as a draw does, so a held one never keeps the next Escape.

**A passphrase change on the device that is playing** asks once, in the place beside the form
(`settingId('passphrase', 'ask')`, outside the place the wire puts): `<name> is playing: if the
new code is not the device's, it refuses the link and the sound stops until the code is right.`,
*Keep the current one* focused, *Change passphrase*. A device is playing when it is connected,
its card has a track that plays and its path does not say the sound goes elsewhere (class
`other`: another sink, or another input of the DAC). Keep or Escape sends nothing; the answer
closes the question and the refusal, if any, reads at the form.

### Remove…

*Remove…* asks once, in the place beside the button, with the inline ask: `Remove <name>? svolder
forgets its name, profile and passphrase; the device itself is not changed.`; on the device that
is playing (as above) `<name> is playing: removing it stops the sound that goes through it.` and
the same tail. *Keep* has the focus (Escape and *Keep* send nothing and give the focus back to
*Remove…*); *Remove* is styled as a danger and sends `removeDevice({ device })` naming the page's
device, found by its unit id when a rename came meanwhile. While it is on its way the place says
`Removing…` and the question's buttons are disabled. A refusal reads at the control with the
same words as the settings (`missing`: `This device is no longer configured.`) and the question
stays, so it can be tried again.

On yes the answer's list takes the entry out of the side list, and when the page is still about
that device the pane steps to the Overview (`router.step`, in place), which opens with a line,
`<name> was removed.`, that takes the focus (`popup-devices-notice`, `tabindex="-1"`,
`role="status"`) and stays until the pane steps to a device's page or the popup closes. If the
removed device was the one the page behind shows, the fleet's drop moves the page behind to the
fleet's first device and the popup stands; a rename of that device afterwards is followed as any
other (the stream forgets a stale drop). When the answer comes before the fleet's drop the wire
does not tell the stream the removed device again (it would reopen on a device the daemon
refuses): it moves the page behind to the first device the answer lists, quietly, and follows the
devices then. If it was the last device, the Overview says `No device is configured.` under the
line, the popup stands, and the page behind shows the daemon's card for no device (`No devices`, as at a load; a Rust test
holds its words to the daemon's). A device added back is not shown on its own, as after a load.

### A rename

The device's URL segment is its name, so a rename moves the page. The wire takes the answer's
list and then `followRename`: the page moves by the device's unit id to the new address, in place
(`router.step` from a page replaces the entry, so Back reaches no dead address), the list's label,
the tab's title and the form's field follow, and what is typed stays. When the renamed device is
the one the page behind shows, the page behind follows it too: the answer comes after the daemon
has reconciled, so the wire shows the renamed device at once (`core.actions.showDevice`, no push,
no toast if the daemon does not list it yet), unless the person showed another device while the
rename was in flight, and tells the stream with `core.stream.renamed(from, to)`. That covers the
fleet event that arrives before the answer, which is the usual order: the stream's drop moved the
page to the fleet's first device, and `renamed` then moves it on to the renamed one (the stream
remembers the last drop). Until the page is on the new address the wire leaves the stream's want
as it was (a stream reopened on the old address gets a 404 from the daemon; the follow covers
that). A name's address is the daemon's `segment` (everything but letters, digits and `-._~`
percent-encoded; `wire.js` exports the same), so a name with `(`, `)`, `!`, `'` or `*` keeps its
page.

## The facts and Identify

`core/popups/devices/facts.js` draws them for the wire's place (`FACTS_ID`) under the key graphs of
a device's page, from the `facts` part of the device's model: a description list (`dl.fpd-facts`,
each `dt` and `dd` in a `div`) of Product (`Bifrost 3 (product 5)`, or `product 5` for a product
the profile does not name), Unit, Firmware, DSP (`—` until read), Profile, Link,
Signal and, when the daemon has one, Passphrase. Each value's `dd` carries its label in
`data-fact`; the Signal's carries `data-signal`, its head and its stamp. With no facts nothing is
drawn. *Identify* stands under the list when the profile offers it: `aria-disabled` (it keeps the
focus and sends nothing) with its reason beside it (`aria-describedby`) while the device is not
connected or the daemon says it cannot be asked now; the press calls `core.actions.identify({
device })` for the page's device, never the shown one.

The wire puts the place at each `facts` part, and reads the device's model afresh once, when a
device's page is shown and its model is in the store, taking the part in only when it is not older
than the store's: the daemon sends the part again only when a value changes, so a part may be
old, and the ages are counted from the part's own clock. The ages are kept by one
`core.clock.tickAges` call over the popup's frame (`FACTS_AGES` and `READINGS_AGES`), made when the
popup opens and stopped when it closes; it is ticked after each draw and put.

**`core.driver`** (the checks' way to the facts) opens the popup at the device's page through the
router, opens *All readings* for a reading row and reads the cell (`data-fact`, or
`tr[data-reading] td`): web-themes.md says what it refuses.

## The stream

At its first `show` the wire takes one want, `core.stream.want('popup:devices', { needs: [],
follow })`, with every listed device's URL segment, and gives it back in `hide()`. The stream's
address then carries `follow=` (the shown device is among the names; the daemon follows a device
once) and the store holds each followed device's model, which is where a block's live state and
the words of its readings come from. For a device that is not on show the daemon sends its model
whole when its shape changes (`device`), its parts as they change (`part`: the link, the
controls, the facts at most once a second) and its `volume` and `level` readings, at most 20 a
second; it sends no meters and no strip (the popup's other want, `popup:devices-walk`, asks for
`walk`, the add page's states, taken and given back in one reopening of the stream with this one).
The cost is one more
task on the daemon per followed device and open page, for as long as the popup stands. At the
release the store forgets every device nobody follows any more, and a device the fleet drops is
forgotten at that event.

The graphs are **read again on a timer**, not built from the stream's readings: the stream
carries raw readings for the volume and the sliders only (the other key readings arrive as drawn
parts), and the daemon's ring is what bins a second's samples and joins them by the series'
rule. So the cost is one request per device in view every 10 s, whatever the readings do, and
none while the page rests.

Only a `show` (the opening, a step to a page) draws the whole box. Every live change is put in
its own place, and the frame leaves a place that already holds the same, so an event of one
device never draws another's controls again or takes the focus from them:

- the entries changed (a device appeared, was removed or renamed): the side list alone, with
  `frame.list(entries)`. The pane stays as it stands, with its scroll, its focus and whatever is
  typed or asked on a device's page;
- the devices in the pane changed (the Overview gained or lost a block; the page's own device
  left): the pane's body, one place (`popup-devices-body`), is put;
- anything less: an entry's dot and tag, a state line, a block's dimming, the place of *Show on
  the page* (its button has an id, so the focus finds it when it is drawn again), and a
  device's graphs, which are worded again when its model arrives after its series (at an
  opening the series request races the stream).

Two reads of the list can answer out of order: only the newest read's answer is taken. The
page of a device that was renamed follows it to its new address in place (the router's `step`
from a page replaces the entry; the device is known by its unit id), and the page of a device
that was removed says `This device is no longer configured.`

## The stay rule

Devices is listed with `stays: true`. Every other popup closes when another device comes on
show; this one stands, with its page and its focus, because it is about the devices and makes
the switch itself ([frame.md](frame.md#modals)). *Show on the page* shows the device with no
push: the entry the page is on takes the device and its address, the popup's other entries and
the entry it was opened from take it as a step reaches them, and Back still steps page → list →
closed on the device now shown ([frame.md](frame.md#the-history)). The same holds when the
fleet drops the shown device under the popup (it was removed, or renamed): the page moves to
the first device left and the popup stands.

## Who has it

The row is `sees: 'everyone'`; the mode is `mayWrite(page)` (write.js), read at each draw.
**Readers** (a guest, a viewer of a page with writes off) get the popup with nothing that writes.
Its list is the page's fleet (`devices()` with `rows` null), re-read from the store on
each `fleet` event; it never asks `api/v1/devices` (the daemon answers a guest 401 and a
writes-off viewer 403 on it and every write route). A device's page holds its state, key graphs,
facts (the model's: no Unit for a guest, Identify off), All readings and *Show on the page*
(navigation: it picks the device the reader's own page shows); no settings, *Make default*,
*Remove…*, Identify, config line, *+ Add a device* or walk want (an address naming the add page
opens the Overview). A reader's open page follows a rename by the fleet's `key` (an opaque unit
hash); a device with no row under it is said gone. A viewer change redraws it in the new mode.

## Checks

`node/core-devices.mjs` holds the draws as strings and the wire on the real core;
`node/core-device-config.mjs` the settings' writes (each request with another device on show,
every reason, the passphrase);
`node/core-device-settings.mjs` the settings' draw and the refusals' words, as strings;
`node/core-device-readings.mjs` the readings' disclosure, as strings;
`node/core-device-walk.mjs` the add page ([add-a-device-page.md](add-a-device-page.md));
`node/core-device-wire.mjs` the settings on the real core over a jsdom page, with another device
on show than the page's (each control's request, the refusals, the in-flight state, the focus,
typed text kept, a rename, the shown device's rename, a setting changed elsewhere, the question
on a playing device); `devices_popup.py` the same in a browser, in every theme, wide and phone;
`node/core-stay.mjs` the stay rule on the real core; `popups.py` has Devices as a row, with the
Overview, *Show on the page* by real clicks and a guest's read-only popup, in every theme
(`scripts/web-checks/COVERAGE.md` lists the cases).
