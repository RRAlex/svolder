/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The menu the frame's button opens: one row for the theme (its name and a small preview; it opens
// the picker), then the rows the theme itself offers (`theme.menuRows(core)`), then the rows every
// theme has: the device shown (it opens a list of devices when there are two or more) and
// Sign out, or a guest's Sign in. The rows are drawn when a theme is mounted and again each time
// the menu opens (the theme's `menuOpening()`, if it has one, runs first: the place for work the
// theme does only for a person who looks), so they say what the theme and the page are then; in
// between `sync` keeps them in step.
//
// A row the theme gives is one of
//   { kind: 'heading', text }
//   { kind: 'radio', id, label, dataset, checked(), disabled() /* why, or null */, choose() }
//   { kind: 'check', id, label, dataset, checked(), available(), flip() }
//   { kind: 'action', id, label, key, elementId, available(), run() }
//   { kind: 'sep' }  (hidden when every row after it, up to the next separator, is hidden)
// `dataset` is copied onto the row's button; only an action row may carry an element id.
//
// The keys are the menu's while it is open: the arrows move between the rows that can be chosen
// (round the ends), Home and End jump, Escape or `q` closes, Tab leaves.

const ROLE = { radio: 'menuitemradio', check: 'menuitemcheckbox', action: 'menuitem' };

/** Whether a node is inside the menu, its button included. */
export function inMenu(node) {
  return Boolean(node?.closest?.('.fxmenu'));
}

/** Whether a key press is the menu's own, not the page's: any key while the focus is inside the
 * open popover, and on the closed button the four that open it (Enter, Space, Down, Up). The
 * closed button's other keys reach the page. */
export function menuOwns(event) {
  const target = event.target;
  if (target?.closest?.('.fxpop')) return !target.closest('.fxpop').hidden;
  if (!target?.closest?.('.fxbtn')) return false;
  return ['Enter', ' ', 'ArrowDown', 'ArrowUp'].includes(event.key);
}

/**
 * The menu inside `wrap`, with its `button` and its popover `pop`, for `core`; `openPicker` shows
 * the picker of themes. Returns `{ open, close(back), opened, sync, rebuild }`.
 */
export function createMenu({ document, window, core, wrap, button, pop, openPicker }) {
  /** The rows in the popover now: `{ el, def }`. */
  let rows = [];
  /** A Tab pressed in the menu: the focus leaving it closes it. */
  let tabbing = false;

  const opened = () => !pop.hidden;

  /** A row's button, or the element given, with its role and its words. */
  function item(role, text, tag = 'button') {
    const el = document.createElement(tag);
    if (tag === 'button') el.type = 'button';
    el.setAttribute('role', role);
    el.tabIndex = -1;
    el.textContent = text;
    return el;
  }

  /** A span of class `name` holding `text`. */
  function span(name, text = '') {
    const el = document.createElement('span');
    el.className = name;
    el.textContent = text;
    return el;
  }

  /** The element for the row `def`. */
  function draw(def) {
    if (def.kind === 'heading') {
      const el = span('fxh', def.text);
      el.setAttribute('aria-hidden', 'true');
      return el;
    }
    if (def.kind === 'sep') {
      const el = span(def.own ? 'menu-sep' : 'fxsep');
      el.setAttribute('role', 'separator');
      return el;
    }
    const el = def.tag === 'a' ? item('menuitem', def.label, 'a') : item(ROLE[def.kind], def.label);
    if (def.href) el.href = def.href;
    if (def.kind === 'radio' || def.kind === 'check') el.setAttribute('aria-checked', 'false');
    if (def.kind === 'action' && def.elementId) el.id = def.elementId;
    Object.assign(el.dataset, def.dataset ?? {});
    if (def.key) el.append(span('mkey', def.key));
    if (def.go) el.append(' ', span('mgo', '›'));
    return el;
  }

  /** The row `def` made and kept. */
  function add(def) {
    const row = { el: draw(def), def };
    rows.push(row);
    return row;
  }

  /** What the page and the theme say now, in the rows: checked, disabled with a reason, hidden
   * while there is nothing for a row to do, a rule hidden when what follows it is. */
  function sync() {
    for (const { el, def } of rows) {
      if (def.kind === 'radio') {
        el.setAttribute('aria-checked', String(Boolean(def.checked())));
        const why = def.disabled?.() ?? null;
        if (why) {
          el.setAttribute('aria-disabled', 'true');
          el.title = why;
        } else {
          el.removeAttribute('aria-disabled');
          el.removeAttribute('title');
        }
      } else if (def.kind === 'check') {
        const here = Boolean(def.available());
        el.hidden = !here;
        if (here) el.setAttribute('aria-checked', String(Boolean(def.checked())));
      } else if (def.kind === 'action') {
        el.hidden = def.available ? !def.available() : false;
      }
    }
    rows.forEach((row, at) => {
      if (row.def.kind !== 'sep') return;
      let more = false;
      for (const next of rows.slice(at + 1)) {
        if (next.def.kind === 'sep') break;
        if (next.def.kind !== 'heading' && !next.el.hidden) more = true;
      }
      row.el.hidden = !more;
    });
  }

  /** The theme's rows, or none when it has none or its rows throw. */
  function themeRows(theme) {
    try {
      return theme?.menuRows?.(core) ?? [];
    } catch (error) {
      console.warn('svolder: the theme\'s menu rows threw, so the menu shows only the frame\'s', error);
      return [];
    }
  }

  /** The device list in place of the rows: a radio for each device, the one shown checked. */
  function showDevices() {
    const page = core.model.page();
    rows = [];
    add({ kind: 'heading', text: 'Device' });
    for (const device of page.fleet?.devices ?? []) {
      add({
        kind: 'radio',
        label: device.name,
        dataset: { device: device.url },
        checked: () => core.model.shown() === device.url,
        disabled: () => null,
        choose: () => core.actions.showDevice(device.url),
      });
    }
    pop.replaceChildren(...rows.map((row) => row.el));
    sync();
    (pop.querySelector('[aria-checked=true]') ?? choices()[0])?.focus();
  }

  /** The rows the frame adds to every theme's: the device, then Sign out or Sign in. */
  function frameRows(page) {
    const devices = page.fleet?.devices ?? [];
    const shown = devices.find((device) => device.url === page.shown);
    const out = [];
    if (devices.length > 0) {
      const many = devices.length > 1;
      out.push({
        kind: 'action',
        label: `Device: ${shown?.name ?? 'none'}`,
        elementId: 'menu-device',
        go: true,
        stay: many,
        disabled: !many,
        run: showDevices,
      });
    }
    if (page.viewer?.guest) {
      out.push({
        kind: 'action', tag: 'a', label: 'Sign in', href: './?sign-in', elementId: 'menu-sign-in',
        run() {},
      });
    } else if (page.viewer?.sign_out) {
      out.push({
        kind: 'action', label: 'Sign out', elementId: 'menu-sign-out',
        run: () => core.actions.signOut(),
      });
    }
    return out;
  }

  /** The rows of the menu, drawn from the theme and the page as they are now. */
  function rebuild() {
    rows = [];
    const theme = core.theme;
    const row = add({
      kind: 'action', label: `Theme: ${theme?.label ?? ''}`, elementId: 'menu-theme', go: true,
      run: openPicker,
    });
    const thumb = span('mthumb');
    row.el.prepend(thumb);
    try {
      theme?.preview?.(thumb);
    } catch (error) {
      console.warn('svolder: the theme\'s preview threw', error);
    }
    for (const def of themeRows(theme)) add(def);
    const own = frameRows(core.model.page());
    if (own.length > 0) add({ kind: 'sep', own: true });
    for (const def of own) {
      const made = add(def);
      if (def.disabled) made.el.setAttribute('aria-disabled', 'true');
    }
    pop.replaceChildren(...rows.map((one) => one.el));
    sync();
  }

  /** The rows the arrows reach, in order: those that can be chosen and show. */
  function choices() {
    return [...pop.querySelectorAll('[role^=menuitem]')].filter(
      (el) => el.getAttribute('aria-disabled') !== 'true' && !el.hidden,
    );
  }

  /** Open the menu under its button, inside the viewport, with the focus on the checked radio. */
  function open() {
    if (opened()) return;
    try {
      core.theme?.menuOpening?.(core);
    } catch (error) {
      console.warn('svolder: the theme\'s menuOpening threw', error);
    }
    rebuild();
    pop.hidden = false;
    button.setAttribute('aria-expanded', 'true');
    const box = wrap.getBoundingClientRect();
    const width = pop.offsetWidth;
    const right = button.getBoundingClientRect().right;
    const room = document.documentElement.clientWidth - 8 - width;
    const left = Math.max(8, Math.min(right - width, room));
    pop.style.setProperty('left', `${(left - box.left).toFixed(1)}px`);
    (pop.querySelector('[role=menuitemradio][aria-checked=true]') ?? choices()[0])?.focus();
  }

  /** Close the menu; with `back`, the focus returns to its button unless it went to another
   * control. */
  function close(back) {
    if (!opened()) return;
    pop.hidden = true;
    // No `style` is kept on the popover: its place is worked out again each time it opens.
    pop.removeAttribute('style');
    button.setAttribute('aria-expanded', 'false');
    const at = document.activeElement;
    if (back && (!at || at === document.body || pop.contains(at))) button.focus();
  }

  /** The person chooses the row `el`. */
  function choose(el) {
    const row = rows.find((one) => one.el === el);
    if (!row || el.getAttribute('aria-disabled') === 'true') return;
    const { def } = row;
    if (def.kind === 'radio') {
      def.choose();
      sync();
      close(true);
    } else if (def.kind === 'check') {
      def.flip();
      sync();
      close(true);
    } else if (def.stay) {
      def.run();
    } else {
      close(true);
      def.run();
    }
  }

  document.addEventListener('click', (event) => {
    const target = event.target;
    const onButton = target.closest?.('.fxbtn');
    // Most clicks are the page's own: nothing to do while the menu is closed.
    if (!onButton && !opened()) return;
    if (onButton) {
      if (opened()) close(true);
      else open();
      return;
    }
    const el = target.closest?.('.fxpop [role^=menuitem]');
    if (el) {
      // The sign-in row is a link: the browser takes it from here.
      if (el.tagName !== 'A') choose(el);
    } else if (!wrap.contains(target)) {
      close(true);
    }
  });
  // A step of the history shows another device: the menu closes first.
  window.addEventListener('popstate', () => close(false));
  // On the window, so the theme's own key listeners on the document see the key first, while the
  // menu still stands (they leave a key that is the menu's alone, see `menuOwns`).
  window.addEventListener('keydown', (event) => {
    const key = event.key;
    // Only a Tab pressed in the menu counts, and only until the next key or focus.
    tabbing = key === 'Tab' && pop.contains(event.target);
    if (opened() && (key === 'Escape' || (key.toLowerCase() === 'q' && !event.ctrlKey
      && !event.altKey && !event.metaKey))) {
      event.preventDefault();
      close(true);
      return;
    }
    if (event.target.closest?.('.fxbtn') && (key === 'ArrowDown' || key === 'ArrowUp')) {
      event.preventDefault();
      open();
      return;
    }
    if (!pop.contains(event.target)) return;
    if (key === ' ' && event.target.tagName === 'A') {
      event.preventDefault();
      event.target.click();
      return;
    }
    const list = choices();
    const at = list.indexOf(event.target);
    const to = {
      ArrowDown: list[(at + 1) % list.length],
      ArrowRight: list[(at + 1) % list.length],
      ArrowUp: list[(at - 1 + list.length) % list.length],
      ArrowLeft: list[(at - 1 + list.length) % list.length],
      Home: list[0],
      End: list[list.length - 1],
    }[key];
    if (to) {
      event.preventDefault();
      to.focus();
    }
  });
  document.addEventListener('focusout', (event) => {
    if (!opened() || !pop.contains(event.target)) return;
    const to = event.relatedTarget;
    if (to && (!wrap.contains(to) || tabbing)) close(false);
    tabbing = false;
  });
  document.addEventListener('focusin', () => {
    tabbing = false;
  });

  return { open, close, opened, sync, rebuild };
}
