/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Whether something on the page is on show: the page is the tab in front (Page Visibility) and,
// when there is an element to look at, that element is inside the viewport (IntersectionObserver).
// A stream that costs the daemon something (the visualizer's frames, the device's own events) is
// held open only while it is on show: it opens at once when it is, and closes once it has not
// been for `GRACE_MS`, so a glance away or a quick scroll does not close and reopen it.
//
//   watchVisible({ element, onVisible, onShow, onHide, graceMs })   starts watching; returns
//                      `watch`; `element` is the element whose place counts, if any
//     onVisible(shown)   at once, whenever the answer changes: stop and start drawing
//     onShow()           the first time it is on show, and again after an `onHide`
//     onHide()           once it has been off show for the grace: close the stream
//   watch.observe(element)   the element whose place on the page counts (a new element that
//                            stands where the old one did, after a swap, is handed over here;
//                            `null` for a watch of the page alone)
//   watch.visible      whether it is on show now (before the grace)
//   watch.active       between `onShow` and `onHide`
//   watch.stop()       stops watching, with no callback

/** How long it must be off show before `onHide` runs, in milliseconds. */
export const GRACE_MS = 2000;

/** Whether the tab is the one in front. */
export const pageShown = () => document.visibilityState === 'visible';

/** Watch the page (and, once `observe` is given one, an element) as described above. */
export function watchVisible({ element: first = null, onVisible, onShow, onHide,
  graceMs = GRACE_MS } = {}) {
  let element = null;
  let observer = null;
  // Whether the element is in the viewport; a page with no element counts as in it. A new
  // element that takes the place of one keeps the last answer until the observer says otherwise,
  // so a swap does not blink.
  let inView = first === null || !('IntersectionObserver' in window);
  let visible = false;
  let active = false;
  let timer = 0;

  function evaluate() {
    const now = pageShown() && inView;
    if (now !== visible) {
      visible = now;
      onVisible?.(visible);
    }
    if (visible) {
      clearTimeout(timer);
      timer = 0;
      if (!active) {
        active = true;
        onShow?.();
      }
    } else if (active && !timer) {
      timer = setTimeout(() => {
        timer = 0;
        active = false;
        onHide?.();
      }, graceMs);
    }
  }

  function observe(next) {
    observer?.disconnect();
    observer = null;
    if (next === null) inView = true;
    else if (element === null) inView = !('IntersectionObserver' in window);
    element = next;
    if (element && 'IntersectionObserver' in window) {
      observer = new IntersectionObserver((entries) => {
        inView = entries[entries.length - 1].isIntersecting;
        evaluate();
      });
      observer.observe(element);
    }
    evaluate();
  }

  document.addEventListener('visibilitychange', evaluate);
  if (first !== null) observe(first);
  else evaluate();

  return {
    observe,
    get visible() { return visible; },
    get active() { return active; },
    stop() {
      document.removeEventListener('visibilitychange', evaluate);
      observer?.disconnect();
      observer = null;
      clearTimeout(timer);
      timer = 0;
      visible = false;
      active = false;
    },
  };
}
