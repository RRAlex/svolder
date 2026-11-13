/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for core/popups/devices/settings.js, the settings part of a device's page as
// strings: the words of every refusal at each setting, and the draw of the name, the profile, the
// passphrase and the default mark for a default and a non-default device, each passphrase kind,
// the three choices of its form (Automatic, None, a code) and each per-setting state, with the base profiles left out, ids that hold across a rename, every
// string escaped and no passphrase anywhere. Run: node core-device-settings.mjs
import assert from 'node:assert/strict';
import { DEVICE_REFUSALS } from '../../../bins/svolderd/web/core/device-config-actions.js';
import {
  PASSPHRASE_CHOICES, SETTINGS, SETTING_WORDS, defaultHtml, nameHtml, passphraseHtml, profileHtml, profileOptions,
  refusalWords, settingId, settingsHtml,
} from '../../../bins/svolderd/web/core/popups/devices/settings.js';

let n = 0;
const ok = (name, fn) => { fn(); n++; console.log('ok', name); };

const SECRET = '8642';
const PROFILES = [
  { name: 'amp-common', base: true },
  { name: 'bifrost-3', display: 'Bifrost 3' },
  { name: 'lyr-plus', display: 'Lyr+' },
  { name: 'plain' },
];
const amp = (more = {}) => ({
  unit: '0011223344556677', name: 'Desk amp', profile: 'lyr-plus', passphrase: 'set',
  connected: true, ...more,
});
const refused = (words) => ({ status: 'refused', words });
const count = (html, piece) => html.split(piece).length - 1;
const tagOf = (html, id) => {
  const found = html.match(new RegExp(`<[^>]*\\sid="${id}"[^>]*>`));
  assert.ok(found, `no element with the id ${id}`);
  return found[0];
};

ok('every_reason_reads_as_words_at_every_setting', () => {
  for (const setting of SETTINGS) {
    for (const reason of DEVICE_REFUSALS) {
      const words = refusalWords(setting, { reason, status: 422, error: 'the daemon said so' });
      assert.ok(typeof words === 'string' && words.length > 10, `${setting} ${reason}: ${words}`);
      assert.ok(!words.includes(reason) || !reason.includes('_'), `${setting} ${reason} is raw`);
    }
  }
});

ok('taken_names_that_another_device_has_the_name', () => {
  assert.match(refusalWords('name', { reason: 'taken', status: 422, error: 'x' }),
    /another device/i);
});

ok('invalid_says_what_is_wrong_for_that_setting', () => {
  const of = (setting) => refusalWords(setting, { reason: 'invalid', status: 422, error: 'x' });
  assert.match(of('name'), /name/i);
  assert.match(of('profile'), /profile/i);
  assert.match(of('passphrase'), /four digits/i);
  assert.equal(new Set(SETTINGS.map(of)).size, SETTINGS.length, 'each setting has its own words');
});

ok('the_file_and_the_daemon_reasons_say_what_happened_and_whether_trying_again_helps', () => {
  const of = (reason) => refusalWords('name', { reason, status: 409, error: 'x' });
  assert.match(of('read_only'), /read-only/i);
  assert.match(of('read_only'), /will not help/i);
  assert.match(of('file_busy'), /try again/i);
  assert.match(of('stopping'), /stopping/i);
  assert.match(of('stopping'), /try again/i);
  assert.match(of('no_answer'), /did not answer/i);
  assert.match(of('no_answer'), /try again/i);
  assert.match(of('signed_out'), /sign in/i);
  assert.match(of('forbidden'), /turned off/i);
  assert.match(of('missing'), /no longer configured/i);
});

ok('a_missing_file_reads_in_the_daemons_words_not_as_read_only', () => {
  const error = '/x/config.toml is missing: the daemon keeps its running configuration; restore the '
    + 'file, then try again';
  for (const setting of SETTINGS) {
    assert.equal(refusalWords(setting, { reason: 'read_only', status: 409, error }), error);
  }
});

ok('failed_uses_the_daemons_words_and_a_setting_s_own_when_it_gave_none', () => {
  assert.equal(refusalWords('name', { reason: 'failed', status: 500, error: 'The disk is full.' }),
    'The disk is full.');
  assert.equal(refusalWords('profile', { reason: 'failed', status: 500, error: '' }),
    SETTING_WORDS.kept.profile);
  assert.equal(refusalWords('default', { reason: 'failed', status: 500 }),
    SETTING_WORDS.kept.default);
  assert.equal(refusalWords('name', null), SETTING_WORDS.kept.name);
  assert.equal(refusalWords('name', { reason: 'a_new_one', status: 418, error: 'Teapot.' }),
    'Teapot.');
});

ok('a_passphrase_is_in_no_refusal_s_words', () => {
  for (const reason of DEVICE_REFUSALS) {
    const words = refusalWords('passphrase',
      { reason, status: 422, error: `passphrase ${SECRET} is not taken` });
    assert.ok(!words.includes(SECRET), `${reason}: ${words}`);
  }
  assert.equal(refusalWords('passphrase', { reason: 'failed', status: 500, error: `bad ${SECRET}` }),
    SETTING_WORDS.kept.passphrase);
  assert.equal(refusalWords('passphrase', { reason: 'failed', status: 500, error: 'Error 500.' }),
    'Error 500.');
});

ok('a_device_that_is_not_the_default_has_make_default_and_the_default_says_so', () => {
  const other = settingsHtml(amp(), PROFILES);
  assert.match(tagOf(other, settingId('default', 'make')), /^<button type="button"/);
  assert.ok(other.includes(`>${SETTING_WORDS.makeDefault}</button>`));
  assert.ok(!other.includes('fp-tag'));
  const own = settingsHtml(amp({ default: true }), PROFILES);
  assert.ok(!own.includes(settingId('default', 'make')), 'the default has no button');
  assert.ok(own.includes('<span class="fp-tag" id="popup-devices-set-default-mark" tabindex="-1">Default</span>'));
});

ok('the_name_is_a_field_holding_the_current_name_with_a_save_button', () => {
  const html = nameHtml(amp());
  const field = tagOf(html, settingId('name', 'field'));
  assert.match(field, /^<input /);
  assert.match(field, / value="Desk amp"/);
  assert.match(field, / aria-invalid="false"/);
  assert.ok(!field.includes('aria-describedby'));
  assert.ok(!field.includes(' disabled'));
  assert.match(tagOf(html, settingId('name', 'save')), /^<button type="submit"/);
  assert.match(html, /^<form /);
  assert.ok(settingsHtml(amp(), PROFILES)
    .includes(`<label for="${settingId('name', 'field')}">`), 'the field has a label');
});

ok('a_refused_name_is_invalid_and_described_by_the_words', () => {
  const html = nameHtml(amp(), refused('Another device already has this name.'));
  const field = tagOf(html, settingId('name', 'field'));
  assert.match(field, / aria-invalid="true"/);
  assert.ok(field.includes(` aria-describedby="${settingId('name', 'words')}"`));
  const words = tagOf(html, settingId('name', 'words'));
  assert.ok(words.includes('fp-failed'));
  assert.ok(html.includes('>Another device already has this name.</p>'));
  assert.ok(!field.includes(' disabled'), 'the field can be typed in again');
});

ok('sending_disables_that_setting_s_controls_only_and_says_so', () => {
  const html = settingsHtml(amp(), PROFILES, { name: { status: 'sending' } });
  assert.match(tagOf(html, settingId('name', 'field')), / disabled/);
  assert.match(tagOf(html, settingId('name', 'save')), / disabled/);
  assert.equal(count(html, ' disabled'), 2, 'no other setting is disabled');
  assert.ok(html.includes(`>${SETTING_WORDS.sending}</p>`));
  assert.equal(count(html, SETTING_WORDS.sending), 1);
  for (const [setting, parts, draw] of [
    ['profile', ['open'], () => profileHtml(amp(), PROFILES, { status: 'sending' })],
    ['passphrase', ['change'], () => passphraseHtml(amp(), { status: 'sending' })],
    ['passphrase', ['kind-auto', 'kind-none', 'kind-code', 'field', 'reveal', 'save', 'cancel'],
      () => passphraseHtml(amp(), { status: 'sending', open: true, choice: 'code' })],
    ['default', ['make'], () => defaultHtml(amp(), { status: 'sending' })],
  ]) {
    const one = draw();
    for (const part of parts) assert.match(tagOf(one, settingId(setting, part)), / disabled/);
    assert.ok(one.includes(SETTING_WORDS.sending), `${setting} says it is saving`);
  }
});

ok('every_setting_shows_its_refusal_at_its_own_control', () => {
  const state = {
    profile: refused('P words.'), passphrase: refused('S words.'), default: refused('D words.'),
  };
  const html = settingsHtml(amp(), PROFILES, state);
  for (const [setting, part, words] of [
    ['profile', 'open', 'P words.'], ['passphrase', 'change', 'S words.'],
    ['default', 'make', 'D words.'],
  ]) {
    const id = settingId(setting, 'words');
    assert.ok(html.includes(`id="${id}" role="status">${words}</p>`), `${setting}: ${html}`);
    assert.ok(tagOf(html, settingId(setting, part)).includes(` aria-describedby="${id}"`));
  }
  assert.equal(count(html, 'fp-failed'), 3);
  assert.match(tagOf(html, settingId('name', 'field')), / aria-invalid="false"/);
});

ok('the_profile_names_the_current_one_on_a_control_that_opens_a_list', () => {
  const open = tagOf(profileHtml(amp(), PROFILES), settingId('profile', 'open'));
  assert.match(open, /^<button type="button"/);
  assert.match(open, / aria-haspopup="listbox"/);
  assert.match(open, / aria-expanded="false"/);
  assert.ok(profileHtml(amp(), PROFILES).includes('>Lyr+</button>'));
  assert.ok(profileHtml(amp({ profile: undefined }), PROFILES)
    .includes(`>${SETTING_WORDS.autoProfile}</button>`));
  assert.ok(profileHtml(amp({ profile: 'gone' }), PROFILES).includes('>gone</button>'));
});

ok('the_list_leaves_the_base_profiles_out_but_for_the_one_a_device_is_pinned_to', () => {
  assert.deepEqual(profileOptions(amp(), PROFILES), [
    { value: '', text: SETTING_WORDS.autoProfile, on: false },
    { value: 'bifrost-3', text: 'Bifrost 3', on: false },
    { value: 'lyr-plus', text: 'Lyr+', on: true },
    { value: 'plain', text: 'plain', on: false },
  ]);
  const pinned = profileOptions(amp({ profile: 'amp-common' }), PROFILES);
  assert.deepEqual(pinned.filter((one) => one.on), [
    { value: 'amp-common', text: 'amp-common', on: true }]);
  assert.equal(pinned.length, 5);
  assert.ok(profileHtml(amp({ profile: 'amp-common' }), PROFILES).includes('>amp-common</button>'));
  const auto = profileOptions(amp({ profile: null }), PROFILES);
  assert.equal(auto[0].on, true);
  assert.equal(auto.length, 4);
  assert.equal(profileOptions(amp({ profile: 'gone' }), undefined).length, 2);
});

ok('each_passphrase_kind_is_said_in_words_with_change', () => {
  for (const [kind, words] of [
    ['set', SETTING_WORDS.kinds.set], ['none', SETTING_WORDS.kinds.none],
    ['auto', SETTING_WORDS.kinds.auto], ['unknown', SETTING_WORDS.kinds.unknown],
    ['later', SETTING_WORDS.kinds.unknown],
  ]) {
    const html = passphraseHtml(amp({ passphrase: kind }));
    assert.ok(html.includes(`>${words}</span>`), `${kind}: ${html}`);
    assert.match(tagOf(html, settingId('passphrase', 'change')), /^<button type="button"/);
    assert.ok(!html.includes('<input'), 'no field until Change');
  }
  assert.equal(new Set(Object.values(SETTING_WORDS.kinds)).size, 4);
});

ok('change_offers_automatic_none_or_a_code_as_radios_of_one_name', () => {
  const html = passphraseHtml(amp({ passphrase: 'auto' }), { open: true });
  assert.match(html, /^<form [^>]*><fieldset[^>]*><legend>[^<]+<\/legend>/);
  for (const kind of PASSPHRASE_CHOICES) {
    const radio = tagOf(html, settingId('passphrase', `kind-${kind}`));
    assert.match(radio, /^<input type="radio"/);
    assert.ok(radio.includes(' name="device-passphrase-kind"'), radio);
    assert.ok(radio.includes(` value="${kind}"`), radio);
    assert.match(radio, / required/);
    assert.ok(html.includes(`> ${SETTING_WORDS.choices[kind]}</label>`), `${kind}: ${html}`);
  }
  assert.deepEqual([...PASSPHRASE_CHOICES], ['auto', 'none', 'code']);
  assert.equal(count(html, 'type="radio"'), 3);
  assert.equal(new Set(Object.values(SETTING_WORDS.choices)).size, 3);
  assert.match(SETTING_WORDS.choices.code, /4-digit code/);
});

ok('the_device_s_kind_is_chosen_when_it_is_automatic_or_none_and_a_set_one_chooses_nothing', () => {
  const chosen = (html) => PASSPHRASE_CHOICES.filter((kind) => / checked/
    .test(tagOf(html, settingId('passphrase', `kind-${kind}`))));
  const open = (kind, more = {}) => passphraseHtml(amp({ passphrase: kind }),
    { open: true, ...more });
  assert.deepEqual(chosen(open('auto')), ['auto']);
  assert.deepEqual(chosen(open('none')), ['none']);
  assert.deepEqual(chosen(open('set')), [], 'a code is never drawn, so it is not chosen either');
  assert.deepEqual(chosen(open('unknown')), []);
  assert.deepEqual(chosen(open(SECRET)), []);
  // what the person chose since stands over the device's kind
  assert.deepEqual(chosen(open('auto', { choice: 'code' })), ['code']);
  assert.deepEqual(chosen(open('set', { choice: 'none' })), ['none']);
  assert.deepEqual(chosen(open('none', { choice: 'later' })), ['none']);
});

ok('the_code_s_field_and_toggle_are_there_for_the_code_alone', () => {
  const part = settingId('passphrase', 'code');
  for (const [kind, choice] of [['auto', undefined], ['none', undefined], ['set', undefined],
    ['set', 'auto'], ['set', 'none']]) {
    const html = passphraseHtml(amp({ passphrase: kind }), { open: true, choice });
    assert.match(tagOf(html, part), / hidden/, `${kind} ${choice}`);
    const field = tagOf(html, settingId('passphrase', 'field'));
    assert.match(field, / disabled/, `${kind} ${choice}`);
    assert.ok(!field.includes(' required'), `${kind} ${choice}`);
    assert.match(tagOf(html, settingId('passphrase', 'reveal')), / disabled/);
    assert.ok(!tagOf(html, settingId('passphrase', 'save')).includes(' disabled'));
  }
  const html = passphraseHtml(amp({ passphrase: 'auto' }), { open: true, choice: 'code' });
  assert.ok(!tagOf(html, part).includes(' hidden'));
  const field = tagOf(html, settingId('passphrase', 'field'));
  assert.ok(!field.includes(' disabled'));
  assert.match(field, / required/);
  assert.ok(!tagOf(html, settingId('passphrase', 'reveal')).includes(' disabled'));
  // the field and its toggle stand inside the code's part, after the three choices
  const inner = html.slice(html.indexOf(`id="${part}"`));
  assert.ok(inner.includes(settingId('passphrase', 'field')));
  assert.ok(inner.includes(settingId('passphrase', 'reveal')));
  assert.ok(!inner.includes('type="radio"'));
});

ok('change_shows_a_form_whose_field_is_never_filled', () => {
  const html = passphraseHtml(amp(), { open: true, choice: 'code' });
  const field = tagOf(html, settingId('passphrase', 'field'));
  assert.match(field, /^<input /);
  assert.match(field, / type="password"/);
  assert.match(field, / autocomplete="new-password"/);
  assert.ok(!field.includes(' value='), 'no value is drawn');
  assert.ok(html.includes(`<label for="${settingId('passphrase', 'field')}">`));
  const reveal = tagOf(html, settingId('passphrase', 'reveal'));
  assert.match(reveal, /^<button type="button"/);
  assert.match(reveal, / aria-pressed="false"/);
  assert.ok(reveal.includes(` aria-controls="${settingId('passphrase', 'field')}"`));
  assert.match(tagOf(html, settingId('passphrase', 'save')), /^<button type="submit"/);
  assert.match(tagOf(html, settingId('passphrase', 'cancel')), /^<button type="button"/);
  assert.ok(!html.includes(settingId('passphrase', 'change')), 'Change is the form now');
  const shown = passphraseHtml(amp(), { open: true, choice: 'code', revealed: true });
  assert.match(tagOf(shown, settingId('passphrase', 'field')), / type="text"/);
  assert.match(tagOf(shown, settingId('passphrase', 'reveal')), / aria-pressed="true"/);
  assert.ok(!tagOf(shown, settingId('passphrase', 'field')).includes(' value='));
  const bad = passphraseHtml(amp(),
    { open: true, choice: 'code', ...refused('A passphrase is four digits.') });
  const badField = tagOf(bad, settingId('passphrase', 'field'));
  assert.match(badField, / aria-invalid="true"/);
  assert.ok(badField.includes(settingId('passphrase', 'words')));
  // a refused Automatic or None is described at the choices, and the unused field is not invalid
  const kept = passphraseHtml(amp({ passphrase: 'auto' }),
    { open: true, ...refused('The passphrase was not changed.') });
  assert.match(kept, /<fieldset[^>]* aria-describedby="popup-devices-set-passphrase-words"/);
  assert.match(tagOf(kept, settingId('passphrase', 'field')), / aria-invalid="false"/);
});

ok('the_ids_come_from_the_setting_and_hold_across_a_rename', () => {
  const ids = (html) => [...html.matchAll(/\sid="([^"]+)"/g)].map((found) => found[1]);
  const state = { passphrase: { open: true, choice: 'code' } };
  const before = ids(settingsHtml(amp(), PROFILES, state));
  const after = ids(settingsHtml(amp({ name: 'Bedroom amp' }), PROFILES, state));
  assert.deepEqual(after, before);
  assert.equal(new Set(before).size, before.length, 'no id twice');
  for (const setting of SETTINGS) assert.ok(before.includes(settingId(setting)), setting);
  assert.ok(before.every((id) => id.startsWith('popup-devices-set')), before.join(' '));
  assert.equal(settingId('name', 'field'), 'popup-devices-set-name-field');
});

ok('every_string_is_escaped', () => {
  const NASTY = '<img src=x onerror="a()">\'&';
  const html = settingsHtml(
    amp({ name: NASTY, profile: NASTY, passphrase: NASTY }),
    [{ name: NASTY, display: NASTY }],
    {
      name: refused(NASTY), profile: refused(NASTY), default: refused(NASTY),
      passphrase: { open: true, ...refused(NASTY) },
    },
  );
  assert.ok(!html.includes('<img'), html);
  assert.ok(!html.includes('onerror="'), html);
  assert.ok(!/'/.test(html), 'no bare quote');
  assert.ok(html.includes('&lt;img src=x onerror=&quot;a()&quot;&gt;&#39;&amp;'));
  assert.ok(!defaultHtml(amp(), { status: 'sending', words: NASTY }).includes('<img'));
});

ok('no_passphrase_is_anywhere_in_the_draw', () => {
  // a model that wrongly carried one: nothing of it is drawn
  const leaky = amp({ passphrase: SECRET, secret: SECRET, code: SECRET });
  for (const state of [
    {}, { passphrase: { open: true } }, { passphrase: { open: true, revealed: true } },
    { passphrase: { open: true, choice: 'code', revealed: true, code: SECRET } },
    { passphrase: { open: true, choice: SECRET } },
    { passphrase: { open: true, status: 'sending', typed: SECRET, value: SECRET } },
    { passphrase: { status: 'refused', words: 'The passphrase was not changed.', typed: SECRET } },
  ]) {
    const html = settingsHtml(leaky, PROFILES, state);
    assert.ok(!html.includes(SECRET), JSON.stringify(state));
  }
});

console.log(`${n} core-device-settings checks passed`);
