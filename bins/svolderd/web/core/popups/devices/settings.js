/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The settings part of a device's page in the Devices popup, as draws: model in, string out.
// Pure: no page, no core, no request. Five settings, each in a place of its own
// (`settingId(setting)`) that a wire can put alone: the name (a field and Save), the profile (a
// button that names the current one and opens a list of `profileOptions`), the passphrase (its
// kind in words and Change, which is a small form while `open`: Automatic, None or a code of four
// digits), the default mark (*Make default*, or the word on the device that is the default) and
// *Remove…*, which opens a question (the wire's, in `REMOVE_ASK_ID`) and sends nothing itself.
//
// A device here is its entry of the devices answer: `{ name, profile?, passphrase, default?,
// connected }`, `passphrase` being a kind (`set`, `none`, `auto`), never a passphrase. `profiles`
// is the answer's list (`{ name, display?, base? }`). A setting's state is `{ status, words }`:
// `status` none or `idle`, `sending` (its controls are disabled and the place says so) or
// `refused` with the `words` of `refusalWords`; the passphrase's has `open`, `revealed` and
// `choice` (which of `PASSPHRASE_CHOICES` the person chose since the form opened) too.
// What is typed is in no model: the wire keeps it, and no field of the passphrase has a value.

import { attr, esc } from '../../html.js';
import { rows } from '../frame.js';

/** The settings, in the order they stand. */
export const SETTINGS = Object.freeze(['name', 'profile', 'passphrase', 'default', 'remove']);

/** What the passphrase's form offers, in the order it stands: the code the device advertises, no
 * passphrase, or a code of four digits. Each is the `value` of a radio named
 * `PASSPHRASE_CHOICE_NAME`, so a wire reads the choice off the form. */
export const PASSPHRASE_CHOICES = Object.freeze(['auto', 'none', 'code']);
/** The `name` of the radios of the passphrase's choices. */
export const PASSPHRASE_CHOICE_NAME = 'device-passphrase-kind';

/** The id of the place of `setting`, or of its `part` (a control, its words). It is made of the
 * setting alone: a device's name changes on a rename. */
export const settingId = (setting, part = null) => `popup-devices-set-${setting}`
  + `${part ? `-${part}` : ''}`;
/** The id of the settings' section. */
export const SETTINGS_ID = 'popup-devices-settings';

/** The words of the settings. */
export const SETTING_WORDS = {
  heading: 'Settings',
  labels: {
    name: 'Name', profile: 'Profile', passphrase: 'Passphrase', default: 'Default device',
    remove: 'Remove device',
  },
  save: 'Save',
  cancel: 'Cancel',
  change: 'Change',
  sending: 'Saving…',
  removing: 'Removing…',
  autoProfile: 'Automatic',
  kinds: {
    set: 'Set', none: 'None: the device takes no passphrase',
    auto: 'Automatic: read from what the device advertises', unknown: 'Unknown',
  },
  choose: 'New passphrase',
  choices: { auto: 'Automatic', none: 'None', code: 'A 4-digit code' },
  newPassphrase: 'Code, four digits',
  reveal: 'Show code',
  /** The question asked before a passphrase is changed on a device that is playing. */
  playing: (name) => `${name} is playing: if the new code is not the device's, it refuses the link`
    + ' and the sound stops until the code is right.',
  answers: { change: 'Change passphrase', keep: 'Keep the current one', remove: 'Remove' },
  removeOpen: 'Remove…',
  /** The question asked before a device is removed; the words name the device and, when it is
   * playing, say what the removal does to the sound. */
  removeAsk: (name, playing) => (playing
    ? `${name} is playing: removing it stops the sound that goes through it. `
    : `Remove ${name}? `)
    + 'svolder forgets its name, profile and passphrase; the device itself is not changed.',
  /** What the Overview says once a device was removed. */
  removed: (name) => `${name} was removed.`,
  makeDefault: 'Make default',
  isDefault: 'Default',
  /** What a refusal with no words of its own says. */
  kept: {
    name: 'The name was not changed.', profile: 'The profile was not changed.',
    passphrase: 'The passphrase was not changed.', default: 'The default device was not changed.',
    remove: 'The device was not removed.',
  },
  taken: 'Another device already has this name.',
  invalid: {
    name: 'This is not a name a device can have: a name cannot be empty.',
    profile: 'The daemon does not know this profile any more. Pick another.',
    passphrase: 'A passphrase is four digits.',
    default: 'This device cannot be made the default.',
    remove: 'This device cannot be removed.',
  },
  read_only: 'The configuration file is read-only, so nothing was saved. Trying again will not '
    + 'help until the file can be written.',
  file_busy: 'The configuration file is being written by something else. Try again in a moment.',
  stopping: 'The daemon is stopping, so nothing was saved. Try again once it runs.',
  no_answer: 'The daemon did not answer. Try again.',
  signed_out: 'You are signed out. Sign in to change this.',
  forbidden: 'Changes are turned off on this page.',
  missing: 'This device is no longer configured.',
};

/** The phrase of the daemon's words for a configuration file that is gone (it answers those as
 * `read_only` too): they say the step, so the page shows them as they are. */
const FILE_MISSING = ' is missing: the daemon keeps its running configuration; restore the file';

/** Four digits in a row: what a passphrase looks like. */
const LIKE_A_PASSPHRASE = /\d{4}/;

/**
 * The words a person reads at the control of `setting` when its write was refused: `refusal` is
 * the write's `{ reason, status, error }`, `reason` one of `DEVICE_REFUSALS`. Each reason of the
 * daemon and of the page has words of the page's own; any other (`failed`, a call that named no
 * device) reads the daemon's words (`error`), or that the setting was not changed. No passphrase
 * is in any of them: the passphrase's refusal never hands on words that hold four digits, except the daemon's
 * missing-file words, which are built from a path alone (device-config-actions.js has already
 * swapped any words that quote the passphrase).
 */
export function refusalWords(setting, refusal, words = SETTING_WORDS) {
  const reason = refusal?.reason;
  // the walk's reasons come to no setting; the closed list holds them, and they read as not changed
  if (['busy', 'not_waiting', 'no_such_job'].includes(reason)) return words.kept[setting];
  if (reason === 'invalid') return words.invalid[setting] ?? words.kept[setting];
  if (reason === 'read_only' && typeof refusal?.error === 'string'
    && refusal.error.includes(FILE_MISSING)) return refusal.error;
  if (['taken', 'read_only', 'file_busy', 'stopping', 'no_answer', 'signed_out', 'forbidden',
    'missing'].includes(reason)) return words[reason];
  const said = typeof refusal?.error === 'string' ? refusal.error : '';
  if (!said || (setting === 'passphrase' && LIKE_A_PASSPHRASE.test(said))) {
    return words.kept[setting];
  }
  return said;
}

const sending = (state) => state?.status === 'sending';
const refused = (state) => state?.status === 'refused';

/** The line under a setting's controls, always there so a screen reader hears what comes into
 * it: empty, that the setting is being saved, or the words of a refusal. */
function wordsHtml(setting, state, words) {
  let said = '';
  if (sending(state)) said = setting === 'remove' ? words.removing : words.sending;
  else if (refused(state)) said = state.words;
  return `<p class="fp-note${refused(state) ? ' fp-failed' : ''}"`
    + `${attr('id', settingId(setting, 'words'))} role="status">${esc(said)}</p>`;
}

/** What ties a control to its setting's refusal, while there is one. */
const described = (setting, state) => attr('aria-describedby',
  refused(state) ? settingId(setting, 'words') : null);

/** A button of `setting`: `part` is its id's part and `more` further attributes. */
function button(setting, part, label, state, { type = 'button', more = '' } = {}) {
  return `<button type="${type}" class="fi-btn"${attr('id', settingId(setting, part))}${more}`
    + `${attr('disabled', sending(state))}>${esc(label)}</button>`;
}

/** The name: a form of one field, which holds the current name, and Save. A refused name marks
 * the field invalid and describes it by the refusal. */
export function nameHtml(device, state = null, words = SETTING_WORDS) {
  return '<form class="fi-form" data-setting="name">'
    + `<input class="fi-input" type="text"${attr('id', settingId('name', 'field'))}`
    + ' name="device-name" required autocomplete="off" spellcheck="false" enterkeyhint="done"'
    + `${attr('aria-invalid', String(refused(state)))}${described('name', state)}`
    + `${attr('value', device.name)}${attr('disabled', sending(state))}> `
    + `${button('name', 'save', words.save, state, { type: 'submit' })}</form>`
    + `${wordsHtml('name', state, words)}`;
}

/** What a profile is called: its display name, else its name. */
const profileText = (one) => one.display || one.name;

/**
 * The choices of a device's profile, as `openList` of core/list.js takes them (`{ value, text,
 * on }`): no pin first (the value is empty), then every profile that is no base. The profile
 * the device is pinned to is among them whatever it is, a base or one the list does not hold.
 */
export function profileOptions(device, profiles, words = SETTING_WORDS) {
  const pinned = device.profile ?? null;
  const offered = (profiles ?? []).filter((one) => !one.base || one.name === pinned);
  if (pinned !== null && !offered.some((one) => one.name === pinned)) {
    offered.push({ name: pinned });
  }
  return [
    { value: '', text: words.autoProfile, on: pinned === null },
    ...offered.map((one) => ({
      value: one.name, text: profileText(one), on: one.name === pinned,
    })),
  ];
}

/** The profile: a button that names the current one and opens the list of the choices. */
export function profileHtml(device, profiles, state = null, words = SETTING_WORDS) {
  const current = profileOptions(device, profiles, words).find((one) => one.on).text;
  const more = ' aria-haspopup="listbox" aria-expanded="false"'
    + `${attr('aria-label', `${words.labels.profile}: ${current}`)}${described('profile', state)}`;
  return `<p class="fi-set">${button('profile', 'open', current, state, { more })}</p>`
    + `${wordsHtml('profile', state, words)}`;
}

/** Which choice the open form stands on: what the person chose since (`state.choice`), else the
 * device's kind when it is `auto` or `none`, else none. A device whose passphrase is set stands
 * on none: its code is not known here, so *code* is not chosen for it. */
function chosenKind(device, state) {
  if (PASSPHRASE_CHOICES.includes(state?.choice)) return state.choice;
  return ['auto', 'none'].includes(device.passphrase) ? device.passphrase : null;
}

/**
 * The passphrase: its kind in words and Change, or while `state.open` the form Change shows: the
 * three choices as radios of one name in a `fieldset` (Automatic, None, a code), the code's part
 * (`settingId('passphrase', 'code')`), Save and Cancel. The code's part is hidden, and its field
 * and toggle disabled, unless the code is chosen: it holds a field that is never filled (no
 * value is drawn, and a password manager is told it is a new one) and a toggle that shows what
 * is typed (`state.revealed`).
 */
export function passphraseHtml(device, state = null, words = SETTING_WORDS) {
  const tail = wordsHtml('passphrase', state, words);
  if (!state?.open) {
    const kind = Object.hasOwn(words.kinds, device.passphrase) ? words.kinds[device.passphrase]
      : words.kinds.unknown;
    return `<p class="fi-set"><span>${esc(kind)}</span> `
      + `${button('passphrase', 'change', words.change, state,
        { more: described('passphrase', state) })}</p>${tail}`;
  }
  const field = settingId('passphrase', 'field');
  const revealed = state.revealed === true;
  const chosen = chosenKind(device, state);
  const code = chosen === 'code';
  const off = sending(state) || !code;
  const radios = PASSPHRASE_CHOICES.map((kind) => '<label><input type="radio"'
    + `${attr('id', settingId('passphrase', `kind-${kind}`))}`
    + `${attr('name', PASSPHRASE_CHOICE_NAME)}${attr('value', kind)} required`
    + `${attr('checked', chosen === kind)}${attr('disabled', sending(state))}> `
    + `${esc(words.choices[kind])}</label>`).join(' ');
  return '<form class="fi-form" data-setting="passphrase">'
    + `<fieldset${described('passphrase', state)}><legend>${esc(words.choose)}</legend>`
    + `${radios}</fieldset> `
    + `<span${attr('id', settingId('passphrase', 'code'))}${attr('hidden', !code)}>`
    + `<label${attr('for', field)}>${esc(words.newPassphrase)}</label> `
    + `<input class="fi-input"${attr('type', revealed ? 'text' : 'password')}${attr('id', field)}`
    + ' name="device-passphrase" autocomplete="new-password" inputmode="numeric"'
    + ` pattern="[0-9]{4}" maxlength="4"${attr('required', code)}`
    + ' autocapitalize="none" spellcheck="false"'
    + `${attr('aria-invalid', String(code && refused(state)))}`
    + `${code ? described('passphrase', state) : ''}${attr('disabled', off)}> `
    + `<button type="button" class="fi-btn"${attr('id', settingId('passphrase', 'reveal'))}`
    + `${attr('aria-pressed', String(revealed))}${attr('aria-controls', field)}`
    + `${attr('disabled', off)}>${esc(words.reveal)}</button></span> `
    + `${button('passphrase', 'save', words.save, state, { type: 'submit' })} `
    + `${button('passphrase', 'cancel', words.cancel, state)}</form>${tail}`;
}

/** The default mark: the word on the device that is the default, *Make default* on another. */
export function defaultHtml(device, state = null, words = SETTING_WORDS) {
  const mark = device.default
    ? `<span class="fp-tag"${attr('id', settingId('default', 'mark'))} tabindex="-1">`
      + `${esc(words.isDefault)}</span>`
    : button('default', 'make', words.makeDefault, state, { more: described('default', state) });
  return `<p class="fi-set">${mark}</p>${wordsHtml('default', state, words)}`;
}

/** The remove: *Remove…*, which asks (in `REMOVE_ASK_ID`, beside this place) before anything is
 * sent. */
export function removeHtml(device, state = null, words = SETTING_WORDS) {
  return `<p class="fi-set">${button('remove', 'open', words.removeOpen, state,
    { more: described('remove', state) })}</p>${wordsHtml('remove', state, words)}`;
}

/** The place of the question *Remove…* asks: beside the remove's place, never inside it. */
export const REMOVE_ASK_ID = settingId('remove', 'ask');

/** The place of the question a passphrase change on a playing device asks: beside the
 * passphrase's place, never inside it, so a put of the form leaves it as it stands. */
export const ASK_ID = settingId('passphrase', 'ask');

/** The draw of one `setting` of `device`, for the wire to put into `settingId(setting)`. */
export function settingHtml(setting, device, profiles, state = null, words = SETTING_WORDS) {
  if (setting === 'name') return nameHtml(device, state, words);
  if (setting === 'profile') return profileHtml(device, profiles, state, words);
  if (setting === 'passphrase') return passphraseHtml(device, state, words);
  if (setting === 'remove') return removeHtml(device, state, words);
  return defaultHtml(device, state, words);
}

/** The place a setting's question opens in, beside it: the passphrase's and the remove's. */
const askPlace = (setting) => {
  const id = { passphrase: ASK_ID, remove: REMOVE_ASK_ID }[setting];
  return id ? `<div${attr('id', id)}></div>` : '';
};

/**
 * The settings of `device`: a heading (at `level`) and one row per setting, each a label and
 * the setting's place. `state` holds each setting's state by its name; one that is absent is
 * idle.
 */
export function settingsHtml(device, profiles, state = {}, words = SETTING_WORDS, level = 4) {
  const label = (setting) => (setting === 'name'
    ? `<label${attr('for', settingId('name', 'field'))}>${esc(words.labels.name)}</label>`
    : esc(words.labels[setting]));
  return `<section${attr('id', SETTINGS_ID)}>`
    + `<h${level} class="fp-heading">${esc(words.heading)}</h${level}>`
    + `${rows(SETTINGS.map((setting) => ({
      label: label(setting),
      value: `<div${attr('id', settingId(setting))}>`
        + `${settingHtml(setting, device, profiles, state[setting], words)}</div>`
        + `${askPlace(setting)}`,
    })))}`
    + '</section>';
}
