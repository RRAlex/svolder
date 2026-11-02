/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The words the daemon's readings are told in, for any theme: how old a reading is, the signal's
// line and the Identify answer's toast. Pure: numbers and answers in, text out.

/**
 * A span of `secs` seconds in the largest unit that keeps it readable, as the daemon words one:
 * seconds under 90 s, minutes under 90 min, hours under 36 h, then days; each count rounded down.
 */
export function ageWords(secs) {
  const s = Math.max(0, Math.floor(secs));
  if (s < 90) return `${s} s`;
  if (s < 90 * 60) return `${Math.floor(s / 60)} min`;
  if (s < 36 * 3600) return `${Math.floor(s / 3600)} h`;
  const days = Math.floor(s / 86400);
  return `${days} ${days === 1 ? 'day' : 'days'}`;
}

/** The signal as the daemon words it: `head` (`−62 dBm advert`), with its age after it; a live
 * signal's only once it is half a minute old. */
export function signalText(head, live, secs) {
  if (live && secs < 30) return head;
  return `${head}, ${ageWords(secs)} ago`;
}

/** The words an Identify answer is toasted with; `gap` is the daemon's wait between two cues, in
 * seconds, as the button carries it. */
export function identifyWords(answer, gap) {
  if (answer.status === 200) {
    return answer.sent ? 'Identify sent' : `Just sent — wait ${gap} s`;
  }
  return answer.error || 'The device did not answer.';
}
