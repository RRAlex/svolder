// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

// The top-bar button and its popup. It draws what model.js gives it and runs the command lines
// the model names; it judges nothing itself.

import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import GObject from 'gi://GObject';
import St from 'gi://St';

import * as PanelMenu from 'resource:///org/gnome/shell/ui/panelMenu.js';
import * as PopupMenu from 'resource:///org/gnome/shell/ui/popupMenu.js';
import {Slider} from 'resource:///org/gnome/shell/ui/slider.js';

import {BoxGlyph, SignalGlyph, SpeakerGlyph} from './glyphs.js';
import {barView, COLORS, dbFromFraction, dbText, parseLine, popupView, sliderFraction} from './model.js';
import {
    clickFailure, ControlQueue, eqSwitchArgs, muteArgs, ScrollAccumulator, scrollArgs, volumeSetArgs,
} from './process.js';
import {Runner} from './runner.js';

const SLIDER_DEBOUNCE_MS = 400;

const markup = (label, text) => {
    label.clutter_text.use_markup = true;
    label.clutter_text.set_markup(text);
};

const dotIcon = () => new St.Widget({style_class: 'svolder-dot'});

// A row that opens a submenu, with the current value in grey before the arrow.
const ValueRow = GObject.registerClass(
class SvolderValueRow extends PopupMenu.PopupSubMenuMenuItem {
    _init(label) {
        super._init(label, false);
        this.value = new St.Label({
            style_class: 'svolder-row-value',
            y_expand: true,
            y_align: Clutter.ActorAlign.CENTER,
        });
        // Before the arrow, which is the row's last child.
        this._arrowBin = this.get_last_child();
        this.insert_child_below(this.value, this._arrowBin);
    }
});

// The Computer EQ row: name, preset in grey, the on/off switch, and the presets as a submenu. A
// click on the switch or Space toggles it; the rest of the row, Return and Right open the presets.
const EqRow = GObject.registerClass(
class SvolderEqRow extends ValueRow {
    _init(label) {
        super._init(label);
        this._hasPresets = false;
        this.switch = new PopupMenu.Switch(false);
        // The button owns the clicks: the switch's own handler only reacts to its knob.
        this.switch.reactive = false;
        this.toggleButton = new St.Button({
            style_class: 'svolder-eq-button',
            can_focus: true,
            accessible_name: 'Computer EQ on or off',
            y_align: Clutter.ActorAlign.CENTER,
            child: this.switch,
        });
        this.toggleButton.connect('clicked', () => this.switch.toggle());
        this.insert_child_below(this.toggleButton, this.get_last_child());
    }

    // The presets submenu is only there when there are presets; the switch works either way.
    setPresets(has) {
        this._hasPresets = has;
        this._arrowBin.visible = has;
        if (!has)
            this.menu.close();
    }

    // Both shells reach here for Space and Return (50 from its key handler, 51 from its key
    // bindings): Space toggles, a click or Return opens the presets.
    activate(event) {
        if (event?.type() === Clutter.EventType.KEY_PRESS && event.get_key_symbol() === Clutter.KEY_space)
            this.switch.toggle();
        else if (this._hasPresets)
            super.activate(event);
    }
});

export const Indicator = GObject.registerClass(
class SvolderIndicator extends PanelMenu.Button {
    _init() {
        super._init(0.0, 'svolder', false);
        this._view = null;
        this._failure = null;
        this._clickLine = '';
        this._signatures = new Map();
        this._queue = new ControlQueue();
        this._scroll = new ScrollAccumulator();
        this._sliderTimer = 0;
        this._guard = false;
        this._dragging = false;
        this._muteWanted = null;
        this._runner = null;

        this._buildBar();
        this._buildMenu();
        this.connect('scroll-event', (actor, event) => this._onScroll(event));
        this.menu.connect('open-state-changed', (menu, open) => this._onOpen(open));
        this._render();
    }

    start() {
        this._runner = new Runner({
            onLine: line => this._onLine(line),
            onFailure: failure => this._onFailure(failure),
        });
        this._runner.start();
    }

    // Everything the extension started: the child, its restart timer, the slider's timer. The
    // widgets go with destroy().
    stop() {
        this._runner?.stop();
        this._runner = null;
        this._queue.clear();
        if (this._sliderTimer) {
            GLib.Source.remove(this._sliderTimer);
            this._sliderTimer = 0;
        }
    }

    _buildBar() {
        const bar = new St.BoxLayout({style_class: 'svolder-bar', y_align: Clutter.ActorAlign.CENTER});
        this._boxGlyph = new BoxGlyph();
        this._signalGlyph = new SignalGlyph(14);
        this._mutedGlyph = new SpeakerGlyph();
        this._mutedGlyph.set(true, COLORS.white);
        this._figure = new St.Label({style_class: 'svolder-figure', y_align: Clutter.ActorAlign.CENTER});
        bar.add_child(this._boxGlyph.actor);
        bar.add_child(this._signalGlyph.actor);
        bar.add_child(this._mutedGlyph.actor);
        bar.add_child(this._figure);
        this._bar = bar;
        this.add_child(bar);
    }

    _buildMenu() {
        const menu = this.menu;

        this._failureItem = new PopupMenu.PopupBaseMenuItem({reactive: false, can_focus: false});
        this._failureLabel = new St.Label({style_class: 'svolder-failure', x_expand: true});
        this._failureLabel.clutter_text.line_wrap = true;
        this._failureItem.add_child(this._failureLabel);
        menu.addMenuItem(this._failureItem);

        this._headerItem = new PopupMenu.PopupBaseMenuItem({reactive: false, can_focus: false, style_class: 'svolder-wide'});
        this._hBox = new BoxGlyph();
        this._hName = new St.Label({style_class: 'svolder-name', y_align: Clutter.ActorAlign.CENTER});
        this._hRate = new St.Label({style_class: 'svolder-dim', y_align: Clutter.ActorAlign.CENTER});
        this._hSignal = new SignalGlyph(14);
        this._hDbm = new St.Label({style_class: 'svolder-dim', y_align: Clutter.ActorAlign.CENTER});
        this._hSpacer = new St.Widget({x_expand: true});
        for (const w of [this._hBox.actor, this._hName, this._hRate, this._hSpacer, this._hSignal.actor, this._hDbm])
            this._headerItem.add_child(w);
        menu.addMenuItem(this._headerItem);

        this._volumeItem = new PopupMenu.PopupBaseMenuItem({activate: false, style_class: 'svolder-wide'});
        this._muteButton = new St.Button({style_class: 'svolder-mute-button', can_focus: true, accessible_name: 'Mute'});
        this._muteGlyph = new SpeakerGlyph();
        this._muteButton.child = this._muteGlyph.actor;
        this._muteButton.connect('clicked', () => this._onMute());
        this._slider = new Slider(0);
        this._slider.accessible_name = 'Volume';
        this._slider.connect('drag-begin', () => {
            this._dragging = true;
        });
        this._slider.connect('drag-end', () => {
            this._dragging = false;
            this._commitSlider();
        });
        this._slider.connect('notify::value', () => this._onSliderValue());
        this._volumeText = new St.Label({style_class: 'svolder-volume-text', y_align: Clutter.ActorAlign.CENTER});
        this._volumeText.clutter_text.line_wrap = false;
        this._volumeItem.add_child(this._muteButton);
        this._volumeItem.add_child(this._slider);
        this._volumeItem.add_child(this._volumeText);
        menu.addMenuItem(this._volumeItem);

        this._inputRow = new ValueRow('Input');
        this._presetRow = new ValueRow('DAC preset');
        this._eqRow = new EqRow('Computer EQ');
        this._eqRow.switch.connect('notify::state', () => this._onEqSwitch());
        for (const row of [this._inputRow, this._presetRow, this._eqRow])
            menu.addMenuItem(row);

        this._pathItem = new PopupMenu.PopupBaseMenuItem({reactive: false, can_focus: false});
        this._strip = new St.BoxLayout({style_class: 'svolder-strip', y_align: Clutter.ActorAlign.CENTER});
        this._segments = [0, 1, 2].map(() => {
            const seg = new St.Widget({style_class: 'svolder-segment'});
            this._strip.add_child(seg);
            return seg;
        });
        this._pathOff = new St.Label({style_class: 'svolder-path-off', y_align: Clutter.ActorAlign.CENTER});
        this._pathItem.add_child(this._strip);
        this._pathItem.add_child(this._pathOff);
        menu.addMenuItem(this._pathItem);

        this._sep = new PopupMenu.PopupSeparatorMenuItem();
        menu.addMenuItem(this._sep);

        this._scenesRow = new ValueRow('Scenes');
        menu.addMenuItem(this._scenesRow);

        this._clickItem = new PopupMenu.PopupBaseMenuItem({reactive: false, can_focus: false});
        this._clickLabel = new St.Label({style_class: 'svolder-failure', x_expand: true});
        this._clickItem.add_child(this._clickLabel);
        menu.addMenuItem(this._clickItem);

        this._openItem = new PopupMenu.PopupMenuItem('Open svolder');
        this._openItem.add_child(new St.Label({
            text: '↗', style_class: 'svolder-dim', x_expand: true, x_align: Clutter.ActorAlign.END,
            y_align: Clutter.ActorAlign.CENTER,
        }));
        this._openItem.connect('activate', () => this._openTui());
        menu.addMenuItem(this._openItem);
    }

    // -- stream ---------------------------------------------------------------------------

    _onLine(line) {
        const {view, failure} = parseLine(line);
        if (view) {
            this._view = view;
            this._failure = null;
        } else if (failure) {
            this._view = null;
            this._failure = failure;
        } else {
            return;
        }
        this._render();
    }

    _onFailure(failure) {
        this._view = null;
        this._failure = failure;
        this._render();
    }

    // -- drawing --------------------------------------------------------------------------

    _render() {
        const bar = barView(this._view);
        this._boxGlyph.set(bar.box);
        this._signalGlyph.set(bar.signal);
        this._mutedGlyph.actor.visible = bar.figure.kind === 'muted';
        this._figure.visible = bar.figure.kind === 'text';
        if (bar.figure.kind === 'text')
            markup(this._figure, bar.figure.markup);
        this.accessible_name = bar.accessibleName;

        const pop = popupView(this._view, Date.now(), this._failure);
        const hasView = pop.header !== null;
        this._failureItem.visible = pop.failure !== null || pop.note !== null;
        if (pop.failure) {
            const detail = pop.failure.detail ? `\n${pop.failure.detail}` : '';
            this._failureLabel.text = pop.failure.headline + detail;
        } else if (pop.note) {
            this._failureLabel.text = pop.note;
        }
        this._headerItem.visible = hasView;
        this._volumeItem.visible = hasView && pop.volume !== null;
        this._inputRow.visible = hasView && pop.input !== null;
        this._presetRow.visible = hasView && pop.preset !== null;
        this._eqRow.visible = hasView && pop.computerEq !== null;
        this._pathItem.visible = hasView && pop.path !== null;
        this._sep.visible = hasView;
        this._scenesRow.visible = hasView;
        this._clickItem.visible = this._clickLine !== '';
        this._clickLabel.text = this._clickLine;
        if (!hasView)
            return;

        const h = pop.header;
        this._hBox.set({...h.box, outline: null});
        markup(this._hName, h.nameMarkup);
        this._hRate.visible = h.rate !== null;
        if (h.rate)
            markup(this._hRate, h.rate.markup);
        this._hSignal.set(h.signal.glyph);
        markup(this._hDbm, h.signal.markup);

        if (pop.volume)
            this._renderVolume(pop.volume);
        this._renderChoices(this._inputRow, 'input', pop.input);
        this._renderChoices(this._presetRow, 'preset', pop.preset);
        this._renderEq(pop.computerEq);
        this._renderPath(pop.path);
        this._renderScenes(pop.scenes);
    }

    _renderVolume(volume) {
        this._muteGlyph.set(volume.muted);
        this._muteButton.visible = volume.canMute;
        this._muteButton.accessible_name = volume.muted ? 'Unmute' : 'Mute';
        markup(this._volumeText, volume.textMarkup);
        this._slider.visible = volume.fraction !== null;
        if (volume.fraction !== null && !this._dragging && !this._sliderTimer) {
            this._guard = true;
            this._slider.value = volume.fraction;
            this._guard = false;
        }
    }

    // A value row's choices: rebuilt only when the list, or the one that is on, changed.
    _renderChoices(row, key, data) {
        if (!data)
            return;
        row.value.text = data.value;
        const sig = JSON.stringify(data.choices);
        if (this._signatures.get(key) === sig)
            return;
        this._signatures.set(key, sig);
        row.menu.removeAll();
        for (const choice of data.choices) {
            const item = new PopupMenu.PopupMenuItem(choice.name);
            item.setOrnament(choice.on ? PopupMenu.Ornament.DOT : PopupMenu.Ornament.NONE);
            item.connect('activate', () => this._runAction(key, choice.run));
            row.menu.addMenuItem(item);
        }
        if (key !== 'eq')
            row.setSensitive(data.choices.length > 0);
    }

    _renderEq(eq) {
        if (!eq)
            return;
        this._renderChoices(this._eqRow, 'eq', eq);
        this._eqRow.setPresets(eq.choices.length > 0);
        this._guard = true;
        this._eqRow.switch.state = eq.on;
        this._guard = false;
    }

    _renderPath(path) {
        if (!path)
            return;
        this._pathItem.accessible_name = path.label;
        path.segments.forEach((seg, i) => {
            this._segments[i].style = `background-color: ${seg.color};`;
        });
        this._pathOff.visible = path.off !== '';
        if (path.off !== '')
            markup(this._pathOff, path.offMarkup);
    }

    _renderScenes(scenes) {
        this._scenesRow.value.text = scenes.on ?? '';
        const sig = JSON.stringify(scenes.choices);
        if (this._signatures.get('scenes') === sig)
            return;
        this._signatures.set('scenes', sig);
        this._scenesRow.menu.removeAll();
        for (const choice of scenes.choices) {
            const item = new PopupMenu.PopupMenuItem(choice.name);
            item.setOrnament(choice.on ? PopupMenu.Ornament.DOT : PopupMenu.Ornament.NONE);
            if (choice.on)
                item.add_style_class_name('svolder-scene-on');
            item.connect('activate', () => this._runAction('scene', choice.run));
            this._scenesRow.menu.addMenuItem(item);
        }
        this._scenesRow.setSensitive(scenes.choices.length > 0);
    }

    _onOpen(open) {
        if (open) {
            this._render();
        } else if (this._clickLine !== '') {
            this._clickLine = '';
            this._render();
        }
    }

    // -- actions --------------------------------------------------------------------------

    // One call for `key`, one at a time; what comes of it shows as one line in the popup.
    _runAction(key, args) {
        this._queue.submit(key, () => this._call(args));
    }

    // Runs one click's command; true when it succeeded.
    async _call(args) {
        if (!this._runner?.live)
            return false;
        const {code, stderrLines, skipped} = await this._runner.run(args);
        if (!this._runner || skipped)
            return false;
        const line = clickFailure(code, stderrLines) ?? '';
        if (line !== this._clickLine) {
            this._clickLine = line;
            this._render();
        }
        return line === '';
    }

    _onMute() {
        const volume = this._view?.device?.volume;
        if (!volume || volume.muted == null)
            return;
        // Each click flips the state last asked for, so a quick second click undoes the first.
        if (this._muteWanted === null || this._muteWanted.shownAs !== volume.muted)
            this._muteWanted = {shownAs: volume.muted, wanted: volume.muted};
        this._muteWanted.wanted = !this._muteWanted.wanted;
        const wanted = this._muteWanted.wanted;
        const args = muteArgs(this._view.actions, !wanted);
        this._queue.submit('mute', async () => {
            // A failed call leaves the state as shown: the next click starts from it.
            if (!await this._call(args))
                this._muteWanted = null;
        });
    }

    _onEqSwitch() {
        if (this._guard || !this._view)
            return;
        // The switch has already moved; a failing call puts it back with the next render.
        this._runAction('eq', eqSwitchArgs(this._view.actions, !this._eqRow.switch.state));
    }

    _onSliderValue() {
        if (this._guard)
            return;
        const volume = this._view?.device?.volume;
        if (!volume || sliderFraction(volume) === null)
            return;
        markup(this._volumeText, dbText(dbFromFraction(this._slider.value, volume)));
        if (this._dragging)
            return;
        // A key or a scroll on the slider: write once after it stops.
        if (this._sliderTimer)
            GLib.Source.remove(this._sliderTimer);
        this._sliderTimer = GLib.timeout_add(GLib.PRIORITY_DEFAULT, SLIDER_DEBOUNCE_MS, () => {
            this._sliderTimer = 0;
            this._commitSlider();
            return GLib.SOURCE_REMOVE;
        });
    }

    _commitSlider() {
        const volume = this._view?.device?.volume;
        if (!volume || sliderFraction(volume) === null)
            return;
        const db = dbFromFraction(this._slider.value, volume);
        const actions = this._view.actions;
        this._scroll.take();
        this._queue.submit('volume', () => this._call(volumeSetArgs(actions, db)), 'set');
    }

    _onScroll(event) {
        if (event.get_flags() & Clutter.EventFlags.FLAG_POINTER_EMULATED)
            return Clutter.EVENT_PROPAGATE;
        const direction = event.get_scroll_direction();
        if (direction === Clutter.ScrollDirection.UP) {
            this._scroll.addNotch(1);
        } else if (direction === Clutter.ScrollDirection.DOWN) {
            this._scroll.addNotch(-1);
        } else if (direction === Clutter.ScrollDirection.SMOOTH) {
            const [, dy] = event.get_scroll_delta();
            const inverted = event.get_scroll_flags() & Clutter.ScrollFlags.INVERTED;
            this._scroll.addSmooth(inverted ? dy : -dy);
        } else {
            return Clutter.EVENT_PROPAGATE;
        }
        this._queue.submit('volume', () => {
            const net = this._scroll.take();
            const args = this._view && scrollArgs(this._view.actions, net);
            return args ? this._call(args) : null;
        }, 'step');
        return Clutter.EVENT_STOP;
    }

    _openTui() {
        this.menu.close();
        try {
            const app = Gio.AppInfo.create_from_commandline(
                'svolder tui', 'svolder', Gio.AppInfoCreateFlags.NEEDS_TERMINAL);
            app.launch([], global.create_app_launch_context(0, -1));
        } catch (e) {
            this._clickLine = `could not open svolder: ${e.message}`;
            this._render();
        }
    }
});
