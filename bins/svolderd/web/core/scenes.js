/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The scenes of the page, for the Scenes popup: the daemon's own view (`ScenesView`: the scenes,
// which one is on, the one last recalled, and whether "on" is settled) read from the page's model
// (`scenes`, a writer's page only) as the store keeps it, the stream's `scenes` event replacing
// it. Nothing here judges: the list, the parts of a scene, which scene is on and what a name may
// be are the daemon's. What this file adds is words and plain data from that view, as the popup
// draws them: the chips of a scene, the line that says what a save takes (from the fleet and the
// host, which the page knows), and the line that says how a recall went.

/** The words for a part that is the computer, as the daemon names it. */
export const COMPUTER = 'computer';

/** A string field as it is, else none. */
const text = (value) => (typeof value === 'string' ? value : '');

/** The view of `raw` (the page's `scenes`, or the stream's event) as the popup reads it:
 * `{ scenes: [{ name, parts: [{ who, present }] }], on, recalled, known }`, or `null` for a page
 * with no scenes (a guest's or a read-only page's, or an older daemon's). */
export function viewOf(raw) {
  if (!raw || !Array.isArray(raw.scenes)) return null;
  return {
    scenes: raw.scenes.map((scene) => ({
      name: text(scene?.name),
      parts: (Array.isArray(scene?.parts) ? scene.parts : []).map((part) => ({
        who: text(part?.who), present: part?.present === true,
      })),
    })),
    on: typeof raw.on === 'string' ? raw.on : null,
    recalled: typeof raw.recalled === 'string' ? raw.recalled : null,
    // a view that does not say is read as unsettled: no lamp is lit on a guess
    known: raw.known === true,
  };
}

/** The scenes over `store`: `{ view(), watch(fn) }`. `view()` is the page's view now (`null`
 * when it has none); `watch(fn)` calls `fn()` when the page is replaced, the scenes change or the
 * fleet or the host (which the save's line reads) do, and returns the way to stop. */
export function createScenes({ store }) {
  return {
    view: () => viewOf(store.page().scenes),
    watch(fn) {
      const stops = ['model', 'scenes', 'fleet', 'host'].map((event) => store.on(event, fn));
      return () => stops.forEach((stop) => stop());
    },
  };
}

/** The chips of a scene's parts, in the scene's order: `{ who, label, present }`, where the label
 * is the daemon's `who`, and an absent part says it is not connected. The computer's part is
 * always the first word a person reads, so it is not reordered here. */
export function chipsOf(scene) {
  return scene.parts.map((part) => ({
    who: part.who,
    label: part.present ? part.who : `${part.who} · not connected`,
    present: part.present,
  }));
}

/** The name of a scene that is on, only when the view says it is settled. */
export const onNow = (view) => (view?.known ? view.on : null);

/** `a`, `a and b`, `a, b and c`. */
export function joined(items) {
  if (items.length <= 1) return items.join('');
  return `${items.slice(0, -1).join(', ')} and ${items[items.length - 1]}`;
}

/** What a save takes, from the page's fleet and host: the computer and the devices that are
 * connected, with those that are not named as left out. `{ takes: [word], left: [name] }`; the
 * computer is a word (`the computer`), the devices are their configured names. */
export function takesOf(page) {
  const takes = [];
  if (page?.host?.computer) takes.push('the computer');
  const left = [];
  for (const device of page?.fleet?.devices ?? []) {
    if (device?.link?.state === 'on') takes.push(device.name);
    else left.push(device.name);
  }
  return { takes, left };
}

/** The line under the save field: what a save takes, what it leaves out and what it never
 * holds. */
export function saveHint(page) {
  const { takes, left } = takesOf(page);
  const words = [];
  if (takes.length > 0) words.push(`Saves ${joined(takes)}.`);
  else words.push('Nothing is connected to save.');
  if (left.length === 1) words.push(`${left[0]} is not connected and is left out.`);
  else if (left.length > 1) words.push(`${joined(left)} are not connected and are left out.`);
  words.push('No volume, gain, mute or power is saved.');
  return words.join(' ');
}

/** The part's name as a sentence starts it: the computer's part is `the computer`. */
const whoWords = (who) => (who === COMPUTER ? 'the computer' : who);
const sentence = (words) => words.charAt(0).toUpperCase() + words.slice(1);

/** `words` as a sentence ends: with a full stop when none of `.`, `!`, `?` or `…` ends it. */
export const stop = (words) => (/[.!?…]$/.test(words) ? words : `${words}.`);

/** `words` as the daemon's refusal or note is shown: a capital first letter and a full stop. */
export const said = (words) => words.charAt(0).toUpperCase() + stop(words).slice(1);

/** How a recall went, from the answer's outcome (`[PartOutcome]`): `{ headline, set, problems }`.
 * `away` names the parts the view says are not connected: a part that was only left because it
 * is away does not make the recall partial. `headline` is `Movie recalled.`, `Movie recalled in
 * part.` (a part really failed or was set in part) or `Movie was not recalled.` (nothing was
 * set); `set` is the sentence about the parts that were set whole (empty when none); `problems`
 * has a sentence for each part that was not, the daemon's own notes first. */
export function outcomeOf(name, outcome, away = []) {
  const parts = Array.isArray(outcome) ? outcome : [];
  const done = parts.filter((part) => part.status === 'done');
  const rest = parts.filter((part) => part.status !== 'done');
  const missing = (part) => part.status === 'left' && away.includes(part.who);
  const real = rest.filter((part) => !missing(part));
  const touched = done.length + parts.filter((part) => part.status === 'partial').length;
  let headline = `${name} recalled.`;
  if (real.length > 0) {
    headline = touched > 0 ? `${name} recalled in part.` : `${name} was not recalled.`;
  } else if (parts.length > 0 && done.length === 0) {
    headline = `${name} was not recalled.`;
  }
  const setWho = sentence(joined(done.map((part) => whoWords(part.who))));
  const set = done.length > 0 ? `${setWho} ${done.length === 1 ? 'is' : 'are'} set.` : '';
  const fallback = {
    partial: 'was set in part', left: 'was left as it was', failed: 'could not be set',
  };
  const problems = rest.map((part) => {
    const notes = (Array.isArray(part.notes) ? part.notes : []).filter((one) => text(one) !== '');
    if (notes.length > 0) return notes.map(said).join(' ');
    return `${sentence(whoWords(part.who))} ${fallback[part.status] ?? 'gave no answer'}.`;
  });
  return { headline, set, problems };
}
