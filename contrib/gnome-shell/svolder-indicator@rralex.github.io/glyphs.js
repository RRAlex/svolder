// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

// The indicator's drawn glyphs: the DAC box, the four signal bars, the muted and unmuted speaker.
// Each is an St.DrawingArea whose cairo code works in the glyph's own units and is scaled to the
// physical size, so a glyph is the same shape at any scale factor.

import Clutter from 'gi://Clutter';
import St from 'gi://St';

import {COLORS, rgb} from './model.js';

// An area `w` x `h` logical px whose repaint calls `draw(cr, area)` in a w x h space.
function makeGlyph(w, h, draw) {
    const area = new St.DrawingArea({
        style: `width: ${w}px; height: ${h}px;`,
        y_align: Clutter.ActorAlign.CENTER,
    });
    area.connect('repaint', () => {
        const cr = area.get_context();
        const [sw, sh] = area.get_surface_size();
        cr.scale(sw / w, sh / h);
        draw(cr, area);
        cr.$dispose();
    });
    return area;
}

function source(cr, area, hex) {
    if (hex === null)
        cr.setSourceColor(area.get_theme_node().get_foreground_color());
    else
        cr.setSourceRGBA(...rgb(hex), 1);
}

function roundedRect(cr, x, y, w, h, r) {
    cr.newSubPath();
    cr.arc(x + w - r, y + r, r, -Math.PI / 2, 0);
    cr.arc(x + w - r, y + h - r, r, 0, Math.PI / 2);
    cr.arc(x + r, y + h - r, r, Math.PI / 2, Math.PI);
    cr.arc(x + r, y + r, r, Math.PI, 1.5 * Math.PI);
    cr.closePath();
}

// The DAC box, 18 x 14: an outline in the body tint (the panel's own colour when `outline` is
// null) and the LED, a filled dot or a hollow ring.
export class BoxGlyph {
    constructor() {
        this.actor = makeGlyph(18, 14, (cr, area) => this._draw(cr, area));
        this._state = {outline: null, led: 'ring', ledColor: COLORS.grey};
    }

    set(state) {
        this._state = state;
        this.actor.queue_repaint();
    }

    _draw(cr, area) {
        const {outline, led, ledColor} = this._state;
        source(cr, area, outline);
        cr.setLineWidth(1.8);
        roundedRect(cr, 1.5, 3, 15, 8, 2.2);
        cr.stroke();
        cr.setSourceRGBA(...rgb(ledColor), 1);
        if (led === 'dot') {
            cr.arc(13, 7, 1.7, 0, 2 * Math.PI);
            cr.fill();
        } else {
            cr.setLineWidth(1.1);
            cr.arc(13, 7, 1.3, 0, 2 * Math.PI);
            cr.stroke();
        }
    }
}

// Four rounded bars of rising height in a 16 x 12 space drawn into a `w` x 12 area, centred.
export class SignalGlyph {
    constructor(w = 14) {
        this._w = w;
        this.actor = makeGlyph(w, w * 12 / 14, (cr, area) => this._draw(cr, area));
        this._state = {filled: 0, color: COLORS.grey, filledOpacity: 1, emptyOpacity: 0.3};
    }

    set(state) {
        this._state = state;
        this.actor.queue_repaint();
    }

    _draw(cr) {
        const {filled, color, filledOpacity, emptyOpacity} = this._state;
        const scale = this._w / 16;
        // The 16 x 12 space is fitted into the area and centred, as an SVG `meet` does.
        cr.translate(0, (this._w * 12 / 14 - 12 * scale) / 2);
        cr.scale(scale, scale);
        for (let i = 0; i < 4; i++) {
            cr.setSourceRGBA(...rgb(color), i < filled ? filledOpacity : emptyOpacity);
            roundedRect(cr, i * 4, 9 - i * 3, 3, 3 + i * 3, 1);
            cr.fill();
        }
    }
}

// The speaker, 16 x 14: with a cross when muted, with a sound wave when not.
export class SpeakerGlyph {
    constructor() {
        this.actor = makeGlyph(16, 14, (cr, area) => this._draw(cr, area));
        this._muted = false;
        this._color = null;
    }

    set(muted, color = null) {
        this._muted = muted;
        this._color = color;
        this.actor.queue_repaint();
    }

    _draw(cr, area) {
        source(cr, area, this._color);
        cr.setLineWidth(1.6);
        cr.setLineCap(1);
        cr.setLineJoin(1);
        cr.moveTo(1.5, 5);
        cr.relLineTo(2.5, 0);
        cr.relLineTo(3.5, -3);
        cr.relLineTo(0, 10);
        cr.lineTo(4, 9);
        cr.lineTo(1.5, 9);
        cr.closePath();
        cr.fillPreserve();
        cr.stroke();
        if (this._muted) {
            cr.moveTo(10.5, 5);
            cr.relLineTo(4, 4);
            cr.moveTo(14.5, 5);
            cr.relLineTo(-4, 4);
            cr.stroke();
        } else {
            cr.arc(8.5, 7, 3.2, -Math.PI / 3, Math.PI / 3);
            cr.stroke();
        }
    }
}
