/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The Devices popup's wire: thin. It reads the configured devices (GET `api/v1/devices`, a
// writer's view) and, for each device the pane shows, its key series (GET
// `api/v1/devices/{device}/series`), and draws the list, the Overview and a device's page
// (draw.js) into the popup's frame.
//
// Every request and every reading names its device. The popup is about the configured devices,
// not about the one on show: it asks `store.shown()` only to put the tag Shown on that device's
// entry and to offer *Show on the page* on every other.
//
// Live: while the popup stands it owns one stream want, `popup:devices`, that follows every
// listed device, so the store holds each one's model (its link, and the controls that turn a raw
// reading into words). A fleet event (a device appeared, connected, dropped, was removed or
// renamed) reads the list again. Only the first show of a page draws the whole box. After
// that each thing is put in its own place, and a place that holds the same already is left as
// it is, so the focus and a control under the hand stay: the side list alone when its entries
// changed (`frame.list`), the pane's body when the devices in it changed, else an entry's dot
// and tag, a state line, a device's graphs (worded again when its model arrives), the place of
// *Show on the page*. So a device that appears or leaves never takes what is typed or asked on
// another device's page. The graphs are read again every REREAD_MS while the popup stands and
// the stream is open, and when the stream opens again (the page wakes, or the link is found
// again after it was lost): one request per device in view, whatever the readings do.
//
// A device's page also holds its settings (settings.js): the name, the profile, the passphrase,
// *Make default* and *Remove…*, each in a place of its own that is put alone when its state
// changes. Each is sent by its own control through `core.actions` (`renameDevice`, `setProfile`,
// `setPassphrase`, `makeDefault`, `removeDevice`, the last after a question), naming the page's
// device; the answer carries the devices and the profiles after the write, which the popup takes in
// place of a second read. What is typed (the name, the passphrase's code) is kept by this wire: a
// put takes the dirty fields of the page's device before it draws and gives them back after, so no
// put or stream update takes it away (no `core.freeze`: a section the popup puts itself needs no
// hold). The configuration is read again with the graphs while a device's page stands, so a setting
// changed elsewhere (another tab, the TUI) reaches the page. Under the graphs are the device's
// facts and *Identify* (facts.js) and the All readings disclosure (readings.js), each put in its
// own place, and only when the page's own device's facts change.
//
// The last entry of the side list is *+ Add a device*, the daemon's device walk drawn (walk.js, and
// walk-wire.js for the wire of it, which this file starts and stops with the page); while the popup
// stands it owns a second stream want, `popup:devices-walk`, for the walk's states.
//
// A reader (a guest, or a signed-in viewer of a page with writes off: `mayWrite` of write.js, read
// at each draw) has the same popup with nothing in it that writes: its list is the page's fleet,
// kept current by the stream's `fleet` event, and never `api/v1/devices` (a writer's view); a
// device's page holds its state, key graphs, facts and All readings, and *Show on the page*, which
// only picks the device the reader's own page shows. No settings, *Make default*, *Remove…*,
// *Identify* or *+ Add a device* are drawn, and no walk is heard. A viewer that changes under the
// popup draws it again in the new mode at once.
//
// Its one other action is the switch of the shown device, which pushes no entry of the history; the
// popup stays through it (`stays: true` in listed.js).

import { get } from '../../api.js';
import { mayWrite } from '../../write.js';
import { NOT_CHANGED } from '../../device-config-actions.js';
import { closeAsk, askQuestion } from '../../inline-ask.js';
import { closeList, closeOf, openList } from '../../list.js';
import { entryId } from '../frame.js';
import {
  BODY_ID, CONFIG_ID, ENTRIES, NAME, NOTICE_ID, OVERVIEW, SHOW_ID, blockId, bodyHtml, configHtml,
  deviceOf, entriesOf, entryInner, graphsHtml, graphsId, pageHtml, pageOf, showHtml, stateHtml, stateId,
} from './draw.js';
import { FACTS_AGES, FACTS_ID, IDENTIFY_ID, factsHtml } from './facts.js';
import {
  READINGS_AGES, READINGS_BODY_ID, READINGS_ID, readingsBodyHtml,
} from './readings.js';
import {
  ASK_ID, PASSPHRASE_CHOICE_NAME, REMOVE_ASK_ID, SETTING_WORDS, SETTINGS, profileOptions,
  refusalWords,
  settingHtml, settingId,
} from './settings.js';
import { ADD } from './walk.js';
import { createWalk } from './walk-wire.js';

/** A device's name as the daemon's URL segment: everything but letters, digits and `-._~` is
 * percent-encoded (`encodeURIComponent` also leaves `!'()*`). */
export const segment = (name) => encodeURIComponent(name)
  .replace(/[!'()*]/g, (c) => `%${c.charCodeAt(0).toString(16).toUpperCase()}`);

/** The path verdicts that say the sound does not go through this device (the judge's keys). */
const OTHER_SINK = new Set(['not_this_dac', 'other_input']);

/** The owner of the popup's stream want, and of the one for the device walk's states. */
export const OWNER = 'popup:devices';
export const WALK_OWNER = 'popup:devices-walk';

/** How often the graphs of the devices in view are read again, in milliseconds. */
export const REREAD_MS = 10000;

/** The Devices popup's part for the router: `{ show(page), hide(), onKey(event) }`, over `core`,
 * drawn into `frame` (popups/frame.js). */
export function wire({ core, frame }) {
  const store = core.model;
  const doc = frame.el.ownerDocument;
  const view = doc.defaultView;
  const io = { fetch: (...args) => view.fetch(...args) };
  /** Whether the popup stands. */
  let open = false;
  let page = null;
  /** The configured devices as the daemon listed them last (`null` before its first answer),
   * and why the last read failed. */
  let rows = null;
  let error = null;
  /** The series answers by device address: the answer, or `{ error }`. */
  const series = new Map();
  /** The devices whose series is on its way. */
  const asked = new Set();
  /** Counts the openings: an answer of an earlier one is dropped. */
  let opening = 0;
  let release = null;
  let offs = [];
  let timer = 0;
  /** The shapes of the side list and of the pane's body as they stand, each as one string: one
   * that reads the same is not drawn again. */
  let listed = '';
  let body = '';
  /** Counts the reads of the list: an answer that is not the newest read's is dropped. */
  let listing = 0;
  /** The profiles a device can be pinned to, as the daemon's last answer lists them. */
  let profiles = [];
  /** The state of each device's settings by the device's key (`keyOf`): `{ name, profile,
   * passphrase, default, remove }`, each `{ status, words, ... }` as settings.js says. Nothing
   * typed is in it. */
  const settings = new Map();
  /** The profile list that is open, the passphrase question that is open, and what stops them. */
  let pick = null;
  let asking = null;
  let stopping = new AbortController();
  /** The id of the control that takes the focus when the next draw or put is done. */
  let wantFocus = null;
  /** The line the Overview opens with after a removal (`null` when none): it goes when the pane
   * steps to a device's page. */
  let notice = null;
  /** The daemon's words that its configuration file changed since its last reload (`null` when
   * it did not), as the last devices answer said them. */
  let configChanged = null;
  /** Whether the All readings disclosure is open: the person's, so it is kept through every put of
   * the page and goes back to closed when the page shows another device. */
  let readingsOpen = false;
  /** What the facts and the readings were last put from (`null` after a draw or a body put). */
  let factsPut = null;
  /** The core's ticker of the ages on the page (the signal's, kept current), while the popup
   * stands. */
  let ages = null;
  /** Whether the step in hand is the one that shows `notice`. */
  let showingNotice = false;
  /** The new address of the shown device that was renamed, until the fleet lists it: the stream
   * stays on the shown device's old address meanwhile (an open stream that is told the new one
   * cannot be opened on the old, and the fleet event that moves the page behind comes on it). */
  let awaiting = null;
  /** The removed devices the page behind still shows, until it has moved off each (whatever moved
   * it): the stream is not told the devices again meanwhile, it would reopen on a device the
   * daemon 404s. */
  const holding = new Set();
  /** The unit of the device the page was about at the last draw or put: what is typed belongs to
   * that device, and goes back only into a page about it. */
  let typedUnit = null;
  /** What takes back the popup's want for the walk's states and its hearing of them. */
  let releaseWalk = null;
  let offWalk = null;

  /** The add page's walk: the device the popup lists under `name` is the one a walk wrote, and a
   * walk that wrote a device has the popup read its list. */
  const walkPart = createWalk({
    core,
    frame,
    deviceNamed: (name) => {
      const one = devices().find((row) => row.name === name);
      return one
        ? { name: one.name, url: one.url, connected: one.connected, since: one.since } : null;
    },
    written: () => { list(); },
    isOpen: () => open,
  });

  /** Whether the popup is a reader's now: the page's viewer may not write. */
  const readOnly = () => !mayWrite(store.page());
  /** The devices the popup last settled on, which a reader's rename is told from. */
  let seen = null;
  /** The mode the box was last drawn in (`null` before the first draw). */
  let drawnReadOnly = null;

  /** The fleet's row of the device called `name` (the page's model lists every device the
   * daemon holds, with its address and its link). */
  const fleetRow = (name) => (store.page().fleet?.devices ?? []).find((one) => one.name === name);

  /** Whether the sound is going through the device `model` describes: its card has a track that
   * plays and a path that does not say the sound goes elsewhere (verdict `not_this_dac` or
   * `other_input`: another sink, or another input of the DAC). */
  const playsThrough = (model) => (model?.playing?.track
    ? model.playing.track.status === 'playing'
    // no MPRIS player: a sound plays when its stream is judged through hops
    : (model?.playing?.path?.hops?.length ?? 0) > 0)
    && !OTHER_SINK.has(model.playing.path?.verdict);

  /** What a device's facts and readings are drawn from: when it is the same, they are not put. */
  const factsKey = (device) => (device
    ? JSON.stringify([device.url, device.connected, device.model?.facts ?? null]) : null);

  /** The devices as the draws take them, in the list's order: the configured ones once the
   * daemon has answered, the fleet's until then. */
  function devices() {
    const listed = rows ?? (store.page().fleet?.devices ?? []).map((one) => ({ name: one.name }));
    const shown = store.shown();
    return listed.map((row) => {
      const fleet = fleetRow(row.name);
      const url = fleet?.url ?? segment(row.name);
      const link = store.part('link', url) ?? fleet?.link ?? null;
      const connected = link ? link.state === 'on' : row.connected === true;
      return {
        url,
        name: row.name,
        unit: row.unit ?? null,
        key: fleet?.key ?? null,
        connected,
        since: connected ? null : (row.not_connected_since_ms ?? null),
        shown: url === shown,
        profile: row.profile ?? null,
        passphrase: row.passphrase ?? null,
        default: row.default === true,
        playing: connected && playsThrough(store.device(url)),
        model: store.device(url),
        series: series.get(url) ?? null,
      };
    });
  }

  /** What a device's settings are kept by: its unit, which a rename does not change. */
  const keyOf = (device) => device.unit ?? device.url;
  const settingsOf = (device) => settings.get(keyOf(device)) ?? {};
  /** Give `setting` of `device` a new state. */
  function setState(device, setting, state) {
    settings.set(keyOf(device), { ...settingsOf(device), [setting]: state });
  }
  /** The page's device, as `devices()` has it now. */
  const pageDevice = () => devices().find((one) => one.url === deviceOf(page)) ?? null;
  const unitOfPage = () => {
    const device = pageDevice();
    return device ? keyOf(device) : null;
  };
  /** The settings' states as the draw takes them. */
  const settingsFor = () => Object.fromEntries(settings);

  /** The fields of the page's device that hold what is typed: id and value, for each one that
   * differs from what its markup says. */
  function typedFields() {
    const out = [];
    for (const input of frame.el.querySelectorAll('input[id]')) {
      if (['text', 'password'].includes(input.type) && input.value !== input.defaultValue) {
        out.push([input.id, input.value, doc.activeElement === input]);
      }
    }
    return out;
  }
  /** Run `change`, which puts or draws markup, and give back what was typed when the page is
   * still about the same device. */
  function keepTyped(change) {
    const typed = typedFields();
    const unit = typedUnit;
    change();
    typedUnit = unitOfPage();
    if (unit === null || unit !== typedUnit) return;
    for (const [id, value, focused] of typed) {
      const field = doc.getElementById(id);
      if (!field || !frame.el.contains(field) || field.value === value) continue;
      field.value = value;
      if (focused && doc.activeElement === field) {
        field.setSelectionRange?.(value.length, value.length);
      }
    }
  }
  /** Give the focus to the control `id`, if it is there and takes it. */
  function focusOn(id) {
    const el = id ? doc.getElementById(id) : null;
    if (el && frame.el.contains(el) && !el.disabled) el.focus({ preventScroll: true });
  }
  /** The pending focus, once what it names is drawn. */
  function giveFocus() {
    if (wantFocus === null) return;
    focusOn(wantFocus);
    wantFocus = null;
  }

  /** Put the place of `setting` on `device`'s page (the profile's, not while its list is open:
   * it is put when the list closes). */
  function putSetting(device, setting) {
    if (setting === 'profile' && pick) return;
    keepTyped(() => frame.put(settingId(setting), settingHtml(
      setting, device, profiles, settingsOf(device)[setting] ?? null,
    )));
  }
  const putSettings = (device) => { for (const setting of SETTINGS) putSetting(device, setting); };

  /** Put what stands in the All readings disclosure's place: nothing while it is closed. The
   * `<details>` itself is never put, so it stays as the person left it. */
  function putReadings(device) {
    frame.put(READINGS_BODY_ID, readingsBodyHtml(device, { open: readingsOpen }));
    ages?.tick();
  }

  /** The devices whose graphs the pane shows: the page's one, all on the Overview, none on the
   * add page. */
  function inView(list) {
    if (page === ADD) return [];
    const url = deviceOf(page);
    return url === null ? list : list.filter((one) => one.url === url);
  }

  /** Follow every listed device, so the store holds each one's model. */
  function follow(list) {
    release = core.stream.want(OWNER, { needs: [], follow: list.map((one) => one.url) });
  }

  /** What the side list is made of beyond its live dots and tags: the entries and labels. */
  const listShape = (entries) => JSON.stringify(entries.map((one) => [one.id, one.label]));
  /** What the pane's body is made of beyond its live places: the devices in it, and the words
   * of a list that could not be read. */
  const bodyShape = (list) => JSON.stringify([
    page, inView(list).map((one) => [one.url, one.name]), error, notice,
  ]);

  /** Draw the whole box: the opening, and a step to a page. */
  function draw() {
    const list = devices();
    const reader = readOnly();
    seen = list;
    drawnReadOnly = reader;
    const entries = entriesOf(list, undefined, { readOnly: reader });
    listed = listShape(entries);
    body = bodyShape(list);
    factsPut = factsKey(list.find((one) => one.url === deviceOf(page)));
    keepTyped(() => frame.draw({
      entries,
      page,
      body: pageHtml({
        page, devices: list, error, profiles, settings: settingsFor(), notice, configChanged,
        readings: { open: readingsOpen }, walk: reader ? '' : walkPart.html(), readOnly: reader,
      }),
    }));
    giveFocus();
    ages?.tick();
  }

  /** Bring what stands in step with the devices, each thing in its own place: the side list
   * when its entries changed (a device appeared, left or was renamed), the pane's body when the
   * devices in it changed, else each live place. The frame leaves a place that holds the same
   * already, so nothing a person has the focus on is drawn again for another device's event. */
  function update() {
    if (!open) return;
    if (readOnly() !== drawnReadOnly) {
      changeMode();
      return;
    }
    const reader = drawnReadOnly;
    const list = devices();
    const entries = entriesOf(list, undefined, { readOnly: reader });
    if (listShape(entries) !== listed) {
      listed = listShape(entries);
      frame.list(entries);
    } else {
      for (const entry of entries.slice(ENTRIES.length).filter((one) => one.id !== ADD)) {
        frame.put(entryId(NAME, entry.id), entryInner(entry));
      }
    }
    if (bodyShape(list) !== body) {
      body = bodyShape(list);
      // the body's markup goes: a question or a list open in it goes first, as a draw does
      closeAsk(frame.el);
      closeList(frame.el);
      pick = null;
      asking = null;
      factsPut = factsKey(list.find((one) => one.url === deviceOf(page)));
      keepTyped(() => frame.put(BODY_ID, bodyHtml({
        page, devices: list, error, profiles, settings: settingsFor(), notice, configChanged,
        readings: { open: readingsOpen }, walk: reader ? '' : walkPart.html(), readOnly: reader,
      })));
      giveFocus();
      ages?.tick();
      return;
    }
    for (const device of inView(list)) {
      frame.put(stateId(device.url), stateHtml(device));
      const block = doc.getElementById(blockId(device.url));
      if (block && frame.el.contains(block)) block.classList.toggle('fpd-away', !device.connected);
      // a model that came after the series words the graphs
      if (device.series) frame.put(graphsId(device.url), graphsHtml(device));
    }
    // the words alone: a put of a place that holds the same changes nothing, and none takes the
    // focus
    if (!reader) frame.put(CONFIG_ID, configHtml(configChanged));
    const on = list.find((one) => one.url === deviceOf(page));
    if (on) {
      frame.put(SHOW_ID, showHtml(on));
      if (!reader) putSettings(on);
      // the facts and the readings are put when the page's own device's facts changed, never for
      // another device's event: a put makes the ticker take the new element's arrival as now, and
      // every age would start again from the put
      const stamp = factsKey(on);
      if (stamp !== factsPut) {
        factsPut = stamp;
        frame.put(FACTS_ID, factsHtml(on, undefined, { readOnly: reader }));
        putReadings(on);
      }
    }
    typedUnit = unitOfPage();
    refreshPageFacts();
    walkPart.refresh();
  }

  /** Read the model of the device at `url` for its facts, and take them in when they are newer than
   * the store's: the daemon sends the part again only when a value changes, so those the page
   * holds may be old when someone looks, and the ages are counted from the part's own clock. */
  async function refreshFacts(url) {
    const mine = opening;
    const answer = await store.read(url);
    if (!open || mine !== opening || answer.status !== 200 || !answer.device?.facts) return;
    const had = store.device(url)?.facts;
    if (had && Number(had.now_ms) > Number(answer.device.facts.now_ms)) return;
    store.applyPart(url, 'facts', answer.device.facts);
  }

  /** Read the key series of the device at `url` and put its graphs. */
  async function read(url) {
    if (asked.has(url)) return;
    const mine = opening;
    asked.add(url);
    const answer = await get(`api/v1/devices/${url}/series`, io);
    if (mine === opening) asked.delete(url);
    if (!open || mine !== opening) return;
    // a device that left the list meanwhile has no graphs to hold
    if (!devices().some((one) => one.url === url)) return;
    series.set(url, answer.status === 200 ? answer
      : { error: answer.error || 'The readings could not be read.' });
    const device = devices().find((one) => one.url === url);
    frame.put(graphsId(url), graphsHtml(device));
  }

  /** Read the series of every device in view. */
  function readInView() {
    for (const device of inView(devices())) read(device.url);
  }

  /** The devices whose facts were read afresh since their page was last shown. */
  const refreshed = new Set();

  /** The facts of the device the page is about, read afresh once its model is in the store (the
   * stream brings it a moment after the popup opens). */
  function refreshPageFacts() {
    const url = deviceOf(page);
    if (url === null || refreshed.has(url) || !store.device(url)) return;
    refreshed.add(url);
    refreshFacts(url);
  }

  /** The device the page is about goes by its unit (a reader's by the fleet's key, which a rename
   * keeps): when a rename gave it another address, the page follows it there, in place. */
  function followRename(before) {
    const url = deviceOf(page);
    if (url === null || !before) return false;
    const was = before.find((one) => one.url === url);
    const list = devices();
    const unit = was?.unit ?? typedUnit;
    if (list.some((one) => one.url === url)) return false;
    const key = was?.key ?? null;
    const now = (unit ? list.find((one) => one.unit === unit) : null)
      ?? (key ? list.find((one) => one.key === key) : null);
    if (!now) return false;
    core.router.step(pageOf(now.url));
    return true;
  }

  /** Read the configured devices and bring the popup in step with them. */
  async function list() {
    // a reader's list is the fleet's, which the stream keeps: nothing is asked
    if (readOnly()) return;
    const mine = opening;
    listing += 1;
    const turn = listing;
    const before = rows ? devices() : null;
    const answer = await get('api/v1/devices', io);
    // two reads can answer out of order: only the newest one's list is the list
    if (!open || mine !== opening || turn !== listing) return;
    if (answer.status === 200 && Array.isArray(answer.devices)) {
      take(answer, before);
    } else {
      error = answer.error || 'The configured devices could not be read.';
      settle(before);
    }
  }

  /** Take the configured devices and the profiles of an answer (a read, or a write's) in place. */
  function take(answer, before = rows ? devices() : null) {
    // a read that was asked before this answer came is out of date
    listing += 1;
    // a write answered after the viewer lost the right: a reader's list is the fleet's
    if (readOnly()) {
      settle(before);
      return;
    }
    rows = answer.devices;
    if (Array.isArray(answer.profiles)) profiles = answer.profiles;
    configChanged = typeof answer.config_changed === 'string' ? answer.config_changed : null;
    error = null;
    settle(before);
  }

  /** Bring everything in step with `rows`: the stream's follow, the series, the page's device
   * after a rename, each place. Says whether the page moved to another address. */
  function settle(before) {
    const now = devices();
    const fleet = store.page().fleet?.devices ?? [];
    if (awaiting && fleet.some((one) => one.url === awaiting)) awaiting = null;
    // a removed shown device is held off the stream's address until the page has moved off it
    // (or the fleet is empty)
    if (fleet.length === 0) holding.clear();
    releaseHeld();
    if (!awaiting && holding.size === 0) follow(now);
    for (const url of [...series.keys()]) {
      if (!now.some((one) => one.url === url)) series.delete(url);
    }
    const moved = followRename(before ?? (readOnly() ? seen : null));
    seen = now;
    update();
    for (const device of inView(now)) if (!series.has(device.url)) read(device.url);
    return moved;
  }

  /** Let go of the removed devices the page behind no longer shows, whatever moved it (the
   * stream's drop or the wire's own show). True when none is held now. */
  function releaseHeld() {
    for (const url of [...holding]) if (store.shown() !== url) holding.delete(url);
    return holding.size === 0;
  }

  /** The page's viewer changed under the popup: it is drawn again in the new mode, with nothing
   * of the old one left (what was typed, asked or opened, the configured devices' list). */
  function changeMode() {
    const reader = readOnly();
    closeAsk(frame.el);
    closeList(frame.el);
    pick = null;
    asking = null;
    settings.clear();
    profiles = [];
    typedUnit = null;
    wantFocus = null;
    notice = null;
    configChanged = null;
    error = null;
    rows = null;
    listing += 1;
    const wanted = core.stream.batch();
    if (reader) {
      walkPart.leave({ quiet: true });
      offWalk?.();
      offWalk = null;
      releaseWalk?.();
      releaseWalk = null;
    } else if (releaseWalk === null) {
      releaseWalk = core.stream.want(WALK_OWNER, { needs: ['walk'] });
      offWalk = walkPart.listen();
    }
    const onAdd = reader && page === ADD;
    if (onAdd) page = OVERVIEW;
    follow(devices());
    wanted();
    draw();
    // the router names the page that is drawn
    if (onAdd) core.router.step(OVERVIEW);
    readInView();
    if (!reader) list();
  }

  /** The popup is opening: follow the devices the page knows, hear what changes, read the
   * list. */
  function begin() {
    open = true;
    opening += 1;
    // one reopening of the stream for both wants
    const wanted = core.stream.batch();
    follow(devices());
    if (!readOnly()) releaseWalk = core.stream.want(WALK_OWNER, { needs: ['walk'] });
    wanted();
    if (!readOnly()) offWalk = walkPart.listen();
    stopping = new AbortController();
    // the one ticker of the ages on the page: it looks for what it keeps current at each tick,
    // the facts' clock first and the readings' table's when the facts are not there
    ages = core.clock.tickAges(frame.el, {
      stamp: `${FACTS_AGES.stamp}, ${READINGS_AGES.stamp}`,
      ages: READINGS_AGES.ages,
      signal: FACTS_AGES.signal,
    }, { signal: stopping.signal, timers: view });
    offs = [
      store.on('fleet', () => { if (readOnly()) settle(null); else list(); }),
      store.on('model', update),
      store.on('device', update),
      store.on('part', ({ name }) => { if (['link', 'facts'].includes(name)) update(); }),
    ];
    offs.push(store.on('state', (state) => {
      // a page that wakes, or a stream found again after the link was lost, reads at once what
      // it did not read meanwhile
      if (state === 'open') {
        readInView();
        if (deviceOf(page) !== null) list();
        refreshed.clear();
        refreshPageFacts();
      }
    }));
    timer = view.setInterval(() => {
      if (store.state() !== 'open') return;
      readInView();
      // a setting changed elsewhere reaches a device's page, and a reload that clears the config
      // line reaches the Overview that shows it
      if (deviceOf(page) !== null || configChanged !== null) list();
    }, REREAD_MS);
    list();
  }

  // ---- the settings' controls ----

  /** What a refusal says at its control: the daemon's own words for a name it refused (`invalid`
   * or `taken`) when it gave some, else the draw's. */
  function wordsOf(setting, answer) {
    const own = setting === 'name' && ['invalid', 'taken'].includes(answer.reason)
      && answer.error && answer.error !== NOT_CHANGED;
    return own ? answer.error : refusalWords(setting, answer);
  }

  /** Save `setting` of the page's device `device` by `send()` (one of `core.actions`' writes,
   * which names the device): the place says it is saving, the answer's devices and profiles take
   * the place of a read, and the focus returns to `focus` (an id, or a function of the answer's
   * success) once the place is drawn. Resolves the answer. */
  async function save(device, setting, send, { focus, kept = {}, done = {} }) {
    const mine = opening;
    setState(device, setting, { ...kept, status: 'sending' });
    putSetting(device, setting);
    const answer = await send();
    if (!open || mine !== opening) return answer;
    const key = keyOf(device);
    const now = devices().find((one) => keyOf(one) === key) ?? device;
    wantFocus = focus(answer.ok);
    if (answer.ok) {
      setState(now, setting, { ...done, status: 'idle' });
      if (setting === 'name' && device.shown) {
        // the page behind follows the renamed device to its new address
        const to = answer.devices?.find((row) => row.unit === device.unit)?.name;
        if (to !== undefined) {
          const target = fleetRow(to)?.url ?? segment(to);
          core.stream.renamed(device.url, target);
          // the daemon has reconciled when it answers: unless the person has shown another
          // device meanwhile, the page behind moves at once (a stream reopened on the old
          // address would not bring the fleet event), quietly, and the stream is told the devices
          // it follows again; if the daemon does not list it yet the stream's own path recovers
          if (store.shown() === device.url) {
            awaiting = target;
            core.actions.showDevice(target, { push: false, quiet: true }).then((shown) => {
              if (!open || awaiting !== target || !shown.ok) return;
              awaiting = null;
              follow(devices());
            });
          }
        }
      }
      const moved = take(answer);
      if (!moved) giveFocus();
    } else {
      setState(now, setting, { ...kept, status: 'refused', words: wordsOf(setting, answer) });
      putSetting(now, setting);
      giveFocus();
    }
    return answer;
  }

  /** The device the settings' controls are about: the page's, or none. */
  const mine = () => (open && !readOnly() ? pageDevice() : null);

  /** The name: Enter or Save. */
  function saveName(device, form) {
    const field = form.querySelector('input');
    const name = field.value.trim();
    if (name === device.name) {
      field.value = device.name;
      return;
    }
    if (name === '') {
      setState(device, 'name', {
        status: 'refused', words: refusalWords('name', { reason: 'invalid' }),
      });
      putSetting(device, 'name');
      focusOn(settingId('name', 'field'));
      return;
    }
    save(device, 'name', () => core.actions.renameDevice(name, { device: device.url }), {
      focus: () => settingId('name', 'field'),
    });
  }

  /** The profile: the opener lists what the device can be pinned to. */
  function chooseProfile(device, opener) {
    if (closeOf(opener)) return;
    const options = profileOptions(device, profiles);
    pick = openList({
      opener, label: SETTING_WORDS.labels.profile, options, signal: stopping.signal,
      choose: (value) => {
        const on = options.find((one) => one.on)?.value;
        if (value === on) return;
        save(device, 'profile', () => core.actions.setProfile(value === '' ? null : value,
          { device: device.url }), { focus: () => settingId('profile', 'open') });
      },
      onClose: () => {
        pick = null;
        const now = pageDevice();
        if (now) putSetting(now, 'profile');
      },
    });
  }

  /** The state of the passphrase's form now: what it keeps from the one before. */
  const formState = (device, more = {}) => {
    const was = settingsOf(device).passphrase ?? {};
    const said = was.status === 'refused' ? { status: 'refused', words: was.words } : {};
    return { open: true, revealed: was.revealed === true, choice: was.choice, ...said, ...more };
  };

  /** Close the passphrase question, if one is open. */
  function dropAsk() {
    closeAsk(frame.el);
    asking = null;
  }

  /** The passphrase's form was sent: the choice, and the code when the code is chosen. */
  function savePassphrase(device, form) {
    const chosen = form.querySelector(`input[name="${PASSPHRASE_CHOICE_NAME}"]:checked`)?.value;
    const typed = form.querySelector('input[name="device-passphrase"]');
    const value = chosen === 'code' ? typed.value : chosen;
    if (!value || (chosen === 'code' && !/^\d{4}$/.test(value))) {
      setState(device, 'passphrase', formState(device, {
        status: 'refused', words: refusalWords('passphrase', { reason: 'invalid' }),
      }));
      putSetting(device, 'passphrase');
      focusOn(chosen === 'code' ? settingId('passphrase', 'field') : null);
      return;
    }
    const kept = formState(device);
    const run = () => save(device, 'passphrase',
      () => core.actions.setPassphrase(value, { device: device.url }), {
        kept,
        done: {},
        focus: (ok) => settingId('passphrase', ok ? 'change'
          : (chosen === 'code' ? 'field' : 'save')),
      });
    dropAsk();
    if (!device.playing) {
      run();
      return;
    }
    // the device is playing: a wrong code stops the sound, so it is asked once
    const opener = chosen === 'code' ? typed
      : form.querySelector(`#${settingId('passphrase', 'save')}`);
    asking = askQuestion({
      within: doc.getElementById(ASK_ID),
      text: SETTING_WORDS.playing(device.name),
      yes: 'change', no: 'keep', words: SETTING_WORDS, signal: stopping.signal, opener,
      go: async () => {
        await run();
        return { ok: true };
      },
      onClose: () => { asking = null; },
    });
  }

  /** The removal: ask once (the safe answer has the focus), then send it. On yes the list loses
   * the device and the pane goes to the Overview, which opens with a line that says what was
   * removed and takes the focus. A refusal is worded at the control, and the question stays. */
  function askRemove(device, opener) {
    dropAsk();
    setState(device, 'remove', { status: 'idle' });
    putSetting(device, 'remove');
    const key = keyOf(device);
    asking = askQuestion({
      within: doc.getElementById(REMOVE_ASK_ID),
      text: SETTING_WORDS.removeAsk(device.name, device.playing),
      yes: 'remove', no: 'keep', words: SETTING_WORDS, signal: stopping.signal, opener,
      go: async () => {
        const turn = opening;
        const named = () => devices().find((one) => keyOf(one) === key) ?? device;
        setState(device, 'remove', { status: 'sending' });
        putSetting(device, 'remove');
        const answer = await core.actions.removeDevice({ device: named().url });
        if (!open || turn !== opening) return answer;
        if (!answer.ok) {
          setState(named(), 'remove', { status: 'refused', words: wordsOf('remove', answer) });
          putSetting(named(), 'remove');
          return answer;
        }
        const there = unitOfPage() === key || deviceOf(page) === device.url;
        settings.delete(key);
        const gone = named().url;
        // the device the page behind shows is the one removed: the page moves to the first
        // device the answer lists at once, quietly, as a rename's does
        const pick = store.shown() === gone ? answer.devices?.[0]?.name : undefined;
        if (store.shown() === gone) holding.add(gone);
        take(answer);
        if (pick !== undefined) {
          const target = fleetRow(pick)?.url ?? segment(pick);
          core.actions.showDevice(target, { push: false, quiet: true }).then(() => {
            if (open && releaseHeld()) follow(devices());
          });
        }
        if (there) {
          notice = SETTING_WORDS.removed(device.name);
          wantFocus = NOTICE_ID;
          showingNotice = true;
          core.router.step(OVERVIEW);
          showingNotice = false;
        }
        return answer;
      },
      onClose: () => { asking = null; },
    });
  }

  /** A press on one of the settings' buttons: whether it was one. */
  function pressed(button) {
    const device = mine();
    if (!device) return false;
    const id = button.id;
    if (id === settingId('profile', 'open')) {
      chooseProfile(device, button);
    } else if (id === settingId('remove', 'open')) {
      askRemove(device, button);
    } else if (id === settingId('passphrase', 'change')) {
      setState(device, 'passphrase', { open: true, revealed: false });
      wantFocus = null;
      putSetting(device, 'passphrase');
      focusOn(settingId('passphrase', 'kind-auto'));
    } else if (id === settingId('passphrase', 'cancel')) {
      dropAsk();
      setState(device, 'passphrase', { status: 'idle' });
      putSetting(device, 'passphrase');
      focusOn(settingId('passphrase', 'change'));
    } else if (id === settingId('passphrase', 'reveal')) {
      const was = formState(device);
      setState(device, 'passphrase', { ...was, revealed: !was.revealed });
      putSetting(device, 'passphrase');
    } else if (id === settingId('default', 'make')) {
      save(device, 'default', () => core.actions.makeDefault({ device: device.url }), {
        focus: (ok) => settingId('default', ok ? 'mark' : 'make'),
      });
    } else {
      return false;
    }
    return true;
  }

  // `toggle` does not bubble: it is caught on the way down. Opening the disclosure draws the table
  // and reads the facts afresh; closing it empties the place.
  frame.el.addEventListener('toggle', (event) => {
    // a late event of a disclosure that was drawn away is none of the page's now
    if (!open || event.target.id !== READINGS_ID) return;
    if (event.target !== doc.getElementById(READINGS_ID)) return;
    readingsOpen = event.target.open;
    const device = pageDevice();
    if (!device) return;
    putReadings(device);
    if (readingsOpen) {
      refreshed.delete(device.url);
      refreshPageFacts();
    }
  }, true);

  frame.el.addEventListener('submit', (event) => {
    const form = event.target.closest?.('form[data-setting]');
    if (!form) return;
    // stopped before anything else can return: a submit the browser took as its own would be a
    // GET of the page with the form's fields, a passphrase among them, in the address
    event.preventDefault();
    const device = mine();
    if (!device) return;
    if (form.dataset.setting === 'name') saveName(device, form);
    else if (form.dataset.setting === 'passphrase') savePassphrase(device, form);
  });
  frame.el.addEventListener('change', (event) => {
    const radio = event.target;
    const device = mine();
    if (!device || radio.name !== PASSPHRASE_CHOICE_NAME) return;
    dropAsk();
    setState(device, 'passphrase', formState(device, { choice: radio.value }));
    putSetting(device, 'passphrase');
  });

  frame.el.addEventListener('click', (event) => {
    const identify = event.target.closest?.(`#${IDENTIFY_ID}`);
    if (identify && open) {
      const device = mine();
      if (device && identify.getAttribute('aria-disabled') !== 'true') {
        core.actions.identify({ device: device.url });
      }
      return;
    }
    const control = event.target.closest?.('button[id^="popup-devices-set-"]');
    if (control && open && pressed(control)) return;
    const button = event.target.closest?.('[data-show]');
    if (!button || !open) return;
    // the switch is no entry of the history, and the popup stays
    core.actions.showDevice(button.dataset.show, { push: false });
  });

  return {
    show(asked) {
      if (!open) begin();
      // a reader has no add page: an address that names it opens the Overview
      const to = asked === ADD && readOnly() ? OVERVIEW : asked;
      if (!showingNotice) notice = null;
      refreshed.delete(deviceOf(to));
      if (deviceOf(to) !== deviceOf(page)) readingsOpen = false;
      const before = page;
      // stepping back to the Overview from a device's page reads the list, so a hand edit made
      // meanwhile shows in the config line
      const backToOverview = deviceOf(page) !== null && deviceOf(to) === null && to !== ADD;
      page = to;
      if (before === ADD && to !== ADD) walkPart.leave();
      draw();
      if (to === ADD) walkPart.enter();
      readInView();
      refreshPageFacts();
      if (backToOverview) list();
    },
    hide() {
      if (!open) return;
      open = false;
      walkPart.leave();
      walkPart.close();
      offWalk?.();
      offWalk = null;
      stopping.abort();
      ages = null;
      pick = null;
      asking = null;
      settings.clear();
      profiles = [];
      typedUnit = null;
      wantFocus = null;
      notice = null;
      configChanged = null;
      readingsOpen = false;
      factsPut = null;
      awaiting = null;
      holding.clear();
      refreshed.clear();
      // both wants go in one reopening of the stream
      const given = core.stream.batch();
      release?.();
      release = null;
      releaseWalk?.();
      releaseWalk = null;
      given();
      for (const off of offs) off();
      offs = [];
      view.clearInterval(timer);
      timer = 0;
      rows = null;
      error = null;
      series.clear();
      asked.clear();
      listed = '';
      body = '';
    },
    onKey: (event) => Boolean(asking?.key(event) || pick?.key(event)),
  };
}
