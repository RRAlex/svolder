// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A small hand-rolled spinner: the glyph shown in front of the words of a wait the TUI states,
//! turning one step of the braille cycle every hundred milliseconds. It is information rather
//! than an effect — it runs whether or not the animation layer does, at every colour depth and
//! in monochrome — so nothing here reaches into the theme or the effects layer at all.

use std::time::Duration;

/// The cycle the spinner turns through, one glyph a step.
const FRAMES: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

/// How long one glyph is shown before the next, and how often a wait's own clock is asked for a
/// fresh one.
pub const STEP: Duration = Duration::from_millis(100);

/// The glyph the spinner shows `elapsed` after whatever clock it is turning from began: the
/// first of the cycle at zero, the next a step later, and back to the first a whole cycle after
/// that — ten steps of a hundred milliseconds is a thousand, so a second in is exactly where it
/// started.
pub fn glyph(elapsed: Duration) -> char {
    let step = (elapsed.as_millis() / STEP.as_millis()) as usize % FRAMES.len();
    FRAMES[step]
}

/// `words`, with `glyph` in front of them: what a wait the TUI states shows while it lasts.
pub fn before(glyph: char, words: &str) -> String {
    format!("{glyph} {words}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The glyph at the moment a wait begins, a step after, and a full cycle later, which is
    /// the same glyph it began on.
    #[test]
    fn the_glyph_steps_every_hundred_milliseconds_and_cycles_in_a_second() {
        assert_eq!(glyph(Duration::ZERO), '⠋');
        assert_eq!(glyph(Duration::from_millis(100)), '⠙');
        assert_eq!(
            glyph(Duration::from_millis(150)),
            '⠙',
            "not yet the next step"
        );
        assert_eq!(
            glyph(Duration::from_secs(1)),
            '⠋',
            "a whole cycle back to the first"
        );
    }

    /// The glyph goes in front of the words, a space between them.
    #[test]
    fn the_glyph_goes_before_the_words() {
        assert_eq!(before('⠙', "listening…"), "⠙ listening…");
    }
}
