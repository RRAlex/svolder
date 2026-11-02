// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The terminal's title as a small VU meter, one bar per channel.
//!
//! While a device streams its meters, the title carries one bar per scaled meter, filled to how
//! loud it reads. While the level instead comes from the host sink capture, one bar draws per
//! channel: a single channel draws one wide bar, `(██████░░░░) svolder · Tyr`; two keep the
//! facing pair, left channel filling from the left edge toward the centre and the right
//! mirroring it from the right edge, `(██░░░░░███) svolder · BF3`; three to five each
//! draw their own bar, a space between each pair, `(████░ ░░░░░ █████) svolder · Kraken`.
//! Every cell is a full or a light-shade block, so the title keeps its width in the
//! proportional font a tab bar sets it in.
//!
//! The title is decoration over a reading, never a reading itself: nothing here delays, smooths
//! or holds back what a level said, it only redraws less often than the screen does. A terminal
//! that knows nothing about titles simply shows none of it.
//!
//! [`TitleSink`] is what a title is written through, so a test can record the writes rather
//! than send escape sequences to whatever terminal happens to be running it.

use std::io::Write;
use std::time::{Duration, Instant};

/// Somewhere a terminal title can be written.
pub trait TitleSink {
    /// Show `text` as the terminal's title.
    fn set_title(&mut self, text: &str) -> std::io::Result<()>;
}

/// The terminal this process is running on.
pub struct TerminalTitle;

impl TitleSink for TerminalTitle {
    /// Write the title escape sequence to stdout.
    fn set_title(&mut self, text: &str) -> std::io::Result<()> {
        crossterm::execute!(std::io::stdout(), crossterm::terminal::SetTitle(text))
    }
}

/// The least time between two title writes.
///
/// Meters stream several times a second and a good few terminals redraw their whole tab bar
/// on every title they are given, so the bars move at a readable pace of their own rather than
/// at the rate the device reports.
pub const TITLE_PERIOD: Duration = Duration::from_millis(300);

/// How many cells one channel's bar is drawn in.
const BAR_CELLS: usize = 5;

/// What a title reads while there is no level to draw, which is what the terminal guard puts
/// up on its way out too.
pub const PLAIN: &str = "svolder";

/// A cell the bar reaches.
const FULL: char = '█';
/// A cell the bar does not reach: a light shade, the same family of block as a full cell, so
/// it takes the same width in the proportional fonts tab titles are set in, where a space
/// would not.
const EMPTY: char = '░';

/// One bar per channel level in `levels`, in the profile's own order, inside one pair of
/// brackets.
///
/// One channel draws a single bar of `BAR_CELLS * 2` cells filling from the left edge. Two keep
/// the facing pair: the first filling from the left edge toward the centre, the second
/// mirroring it from the right edge, the two touching with no gap between them.
/// Three to five each draw their own bar of [`BAR_CELLS`] cells filling from the left,
/// a single space between each pair of bars. Whatever the count, every bar's level is rounded to
/// the nearest whole cell, so the text after the title never moves: its width is the
/// same whatever the readings are. A level outside `0.0..=1.0` is read as the end it is past,
/// and a level that is not a number at all reads as silence.
///
/// Called with no levels at all, this draws an empty pair of brackets — [`TitleMeter::update`]
/// never does, since it draws no bars at all once a device reports no channel to read.
pub fn bars(levels: &[f64]) -> String {
    let mut out = String::new();
    out.push('(');
    match levels {
        [one] => {
            let full = cells(*one, BAR_CELLS * 2);
            out.extend(std::iter::repeat_n(FULL, full));
            out.extend(std::iter::repeat_n(EMPTY, BAR_CELLS * 2 - full));
        }
        [left, right] => {
            let full = cells(*left, BAR_CELLS);
            out.extend(std::iter::repeat_n(FULL, full));
            out.extend(std::iter::repeat_n(EMPTY, BAR_CELLS - full));
            let full = cells(*right, BAR_CELLS);
            out.extend(std::iter::repeat_n(EMPTY, BAR_CELLS - full));
            out.extend(std::iter::repeat_n(FULL, full));
        }
        many => {
            for (index, level) in many.iter().enumerate() {
                if index > 0 {
                    out.push(' ');
                }
                let full = cells(*level, BAR_CELLS);
                out.extend(std::iter::repeat_n(FULL, full));
                out.extend(std::iter::repeat_n(EMPTY, BAR_CELLS - full));
            }
        }
    }
    out.push(')');
    out
}

/// How many of a `width`-cell bar `level` fills.
fn cells(level: f64, width: usize) -> usize {
    let level = match level.is_nan() {
        true => 0.0,
        false => level.clamp(0.0, 1.0),
    };
    (level * width as f64).round() as usize
}

/// The terminal's title, and what was last put in it.
///
/// What it holds is only there to write less: a title that would say what the title already
/// says is not written at all, and one that would come sooner than [`TITLE_PERIOD`] after the
/// last waits for the next frame after that — carrying whatever the levels read by then rather
/// than the reading it was holding. The plain title is exempt from that wait, so switching the
/// meter off or losing the signal cleans the title the moment it happens.
pub struct TitleMeter {
    /// Where the titles go.
    sink: Box<dyn TitleSink>,
    /// The title last written, which is what a fresh one is worth writing against.
    last: Option<String>,
    /// When that write happened, which is what [`TITLE_PERIOD`] is measured from.
    wrote_at: Option<Instant>,
}

impl TitleMeter {
    /// A meter that writes its titles to `sink` and has written none yet.
    pub fn new(sink: Box<dyn TitleSink>) -> TitleMeter {
        TitleMeter {
            sink,
            last: None,
            wrote_at: None,
        }
    }

    /// Say that `device` reads `levels` — one per channel, in profile order — as of `now`, with
    /// the title meter switched `enabled` or not.
    ///
    /// The title carries the bars while the meter is on and at least one level has been
    /// reported, and the device's name alone otherwise.
    pub fn update(&mut self, now: Instant, device: &str, levels: Option<Vec<f64>>, enabled: bool) {
        let metered = enabled.then_some(levels).flatten();
        // The title travels inside an escape sequence the terminal ends at the first control
        // character, so a name that carries one is shown without it rather than cut short or
        // let loose on the terminal.
        let device: String = device.chars().filter(|c| !c.is_control()).collect();
        let text = match &metered {
            Some(levels) if !levels.is_empty() => format!("{} {PLAIN} · {device}", bars(levels)),
            _ => format!("{PLAIN} · {device}"),
        };
        if self.last.as_deref() == Some(text.as_str()) {
            return;
        }
        if metered.is_some_and(|levels| !levels.is_empty())
            && let Some(wrote_at) = self.wrote_at
            && now.saturating_duration_since(wrote_at) < TITLE_PERIOD
        {
            return;
        }
        self.write(text, Some(now));
    }

    /// Put the plain word back, which is what the title says with nothing being watched.
    pub fn restore(&mut self) {
        self.write(PLAIN.to_string(), None);
    }

    /// Write `text` and remember it, dropping whatever the terminal answered: one that will not
    /// take a title will not take the next one either, and the meter has no way to say so and
    /// nothing to say it on.
    fn write(&mut self, text: String, at: Option<Instant>) {
        let _ = self.sink.set_title(&text);
        self.last = Some(text);
        if let Some(at) = at {
            self.wrote_at = Some(at);
        }
    }
}

/// The xterm sequence that pushes the terminal's own title onto its title stack, so whatever
/// was there before this run can be put back.
pub fn push_title() -> String {
    "\x1b[22;2t".to_string()
}

/// The xterm sequence that pops that title back off the stack. A terminal with no title stack
/// ignores both of these, which is why the plain title is written before this one is sent.
pub fn pop_title() -> String {
    "\x1b[23;2t".to_string()
}

/// Write `sequence` straight to stdout, best-effort: a terminal that does not know it ignores
/// it, and one that will not take it is no reason not to run.
pub fn emit(sequence: &str) {
    let mut out = std::io::stdout();
    let _ = out.write_all(sequence.as_bytes());
    let _ = out.flush();
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    /// A terminal title that only remembers what was written to it.
    struct Fake(Arc<Mutex<Vec<String>>>);

    impl TitleSink for Fake {
        fn set_title(&mut self, text: &str) -> std::io::Result<()> {
            self.0.lock().expect("the titles mutex").push(text.into());
            Ok(())
        }
    }

    /// A meter writing into a list of titles this case can read back.
    fn meter() -> (TitleMeter, Arc<Mutex<Vec<String>>>) {
        let titles = Arc::new(Mutex::new(Vec::new()));
        (TitleMeter::new(Box::new(Fake(titles.clone()))), titles)
    }

    /// The titles written so far.
    fn written(titles: &Arc<Mutex<Vec<String>>>) -> Vec<String> {
        titles.lock().expect("the titles mutex").clone()
    }

    /// The moment the paused clock stands at.
    fn now() -> Instant {
        tokio::time::Instant::now().into_std()
    }

    /// Two channels are drawn from their own edge toward the centre, a whole cell at a time,
    /// and every cell of either is a block of the same family — full where the bar reaches,
    /// light shade where it does not — so the title is the same width in any font whatever it
    /// reads.
    #[test]
    fn two_channels_face_each_other_in_whole_cells_of_one_glyph_family() {
        for (left, right, wanted) in [
            (0.0, 0.0, "(░░░░░░░░░░)"),
            (1.0, 1.0, "(██████████)"),
            (0.4, 0.6, "(██░░░░░███)"),
            (0.5, 0.5, "(███░░░░███)"),
            (0.25, 0.95, "(█░░░░█████)"),
            (0.12, 0.05, "(█░░░░░░░░░)"),
        ] {
            assert_eq!(bars(&[left, right]), wanted, "bars(&[{left}, {right}])");
        }
    }

    /// A single channel draws one wide bar filling from the left edge, `BAR_CELLS * 2` cells,
    /// the same family of glyph as every other count draws.
    #[test]
    fn a_single_channel_draws_one_wide_bar_filling_from_the_left() {
        for (level, wanted) in [
            (0.0, "(░░░░░░░░░░)"),
            (1.0, "(██████████)"),
            (0.6, "(██████░░░░)"),
        ] {
            assert_eq!(bars(&[level]), wanted, "bars(&[{level}])");
        }
    }

    /// Three to five channels each draw their own [`BAR_CELLS`]-cell bar filling from the left,
    /// a single space between each pair — never inward, and never touching.
    #[test]
    fn three_to_five_channels_draw_one_bar_each_separated_by_a_space() {
        assert_eq!(bars(&[1.0, 0.0, 1.0]), "(█████ ░░░░░ █████)");
        assert_eq!(bars(&[1.0, 0.5, 0.0, 1.0]), "(█████ ███░░ ░░░░░ █████)");
        assert_eq!(
            bars(&[1.0, 0.75, 0.5, 0.25, 0.0]),
            "(█████ ████░ ███░░ █░░░░ ░░░░░)"
        );
    }

    /// Whatever the count or the levels, every bar holds the same number of blocks: nothing but
    /// full and light-shade blocks between the brackets, [`BAR_CELLS`] a side for two channels.
    #[test]
    fn the_bars_hold_the_same_number_of_blocks_at_every_level() {
        for step in 0..=100 {
            let level = f64::from(step) / 100.0;
            let drawn = bars(&[level, 1.0 - level]);
            let inner: Vec<char> = drawn
                .trim_start_matches('(')
                .trim_end_matches(')')
                .chars()
                .collect();
            assert_eq!(inner.len(), 2 * BAR_CELLS, "{drawn}");
            assert!(
                inner.iter().all(|c| matches!(c, '█' | '░')),
                "a glyph of another width in {drawn}"
            );
        }
    }

    /// A level past either end of the scale is drawn as that end, and one that is not a number
    /// at all is drawn as silence.
    #[test]
    fn a_level_off_the_scale_is_drawn_as_the_end_it_is_past() {
        assert_eq!(bars(&[-1.0, 4.0]), "(░░░░░█████)");
        assert_eq!(bars(&[f64::NAN, f64::NAN]), "(░░░░░░░░░░)");
    }

    /// A title that would say what the title already says is not written again, however many
    /// frames ask for it.
    #[tokio::test(start_paused = true)]
    async fn an_unchanged_title_is_never_written_again() {
        let (mut meter, titles) = meter();
        meter.update(now(), "BF3", Some(vec![1.0, 1.0]), true);
        assert_eq!(written(&titles), ["(██████████) svolder · BF3"]);

        for _ in 0..4 {
            tokio::time::advance(TITLE_PERIOD).await;
            meter.update(now(), "BF3", Some(vec![1.0, 1.0]), true);
        }
        assert_eq!(
            written(&titles).len(),
            1,
            "the same title was written more than once"
        );
    }

    /// A change that comes sooner than the period waits, and the write that follows it carries
    /// what the meters read by then rather than the reading that was held back.
    #[tokio::test(start_paused = true)]
    async fn a_change_inside_the_period_waits_and_the_next_write_carries_the_latest() {
        let (mut meter, titles) = meter();
        meter.update(now(), "BF3", Some(vec![1.0, 1.0]), true);

        tokio::time::advance(Duration::from_millis(100)).await;
        meter.update(now(), "BF3", Some(vec![0.5, 0.5]), true);
        tokio::time::advance(Duration::from_millis(100)).await;
        meter.update(now(), "BF3", Some(vec![0.25, 0.25]), true);
        assert_eq!(
            written(&titles).len(),
            1,
            "a change inside the period reached the terminal"
        );

        tokio::time::advance(Duration::from_millis(150)).await;
        meter.update(now(), "BF3", Some(vec![0.0, 0.0]), true);
        assert_eq!(
            written(&titles),
            ["(██████████) svolder · BF3", "(░░░░░░░░░░) svolder · BF3"],
            "the write that was allowed carried a reading that had already passed"
        );
    }

    /// Switching the meter off, and losing the signal, both clean the title straight away
    /// rather than at the end of a period the bars were keeping.
    #[tokio::test(start_paused = true)]
    async fn the_switch_off_and_a_lost_signal_clean_the_title_at_once() {
        let (mut meter, titles) = meter();
        meter.update(now(), "BF3", Some(vec![1.0, 1.0]), true);
        meter.update(now(), "BF3", Some(vec![1.0, 1.0]), false);
        assert_eq!(
            written(&titles),
            ["(██████████) svolder · BF3", "svolder · BF3"],
            "the switch waited for the period out"
        );

        meter.update(now(), "BF3", Some(vec![0.5, 0.5]), false);
        assert_eq!(
            written(&titles).len(),
            2,
            "the plain title was written twice"
        );

        tokio::time::advance(TITLE_PERIOD).await;
        meter.update(now(), "BF3", Some(vec![0.5, 0.5]), true);
        meter.update(now(), "BF3", None, true);
        assert_eq!(
            written(&titles).len(),
            4,
            "a device that stopped reporting left its last bars up: {:?}",
            written(&titles)
        );
    }

    /// Restoring says svolder's own name and nothing about a device.
    #[tokio::test(start_paused = true)]
    async fn restore_puts_the_plain_word_back() {
        let (mut meter, titles) = meter();
        meter.update(now(), "BF3", Some(vec![1.0, 1.0]), true);
        meter.restore();
        assert_eq!(written(&titles).last().map(String::as_str), Some("svolder"));
    }

    /// A single-channel device draws the one wide bar, and a five-channel one draws five, each
    /// carried in the same title format as the two-channel pair.
    #[tokio::test(start_paused = true)]
    async fn one_channel_and_five_channels_draw_their_own_bar_count() {
        let (mut one, one_titles) = meter();
        one.update(now(), "Tyr", Some(vec![0.6]), true);
        assert_eq!(written(&one_titles), ["(██████░░░░) svolder · Tyr"]);

        let (mut five, five_titles) = meter();
        five.update(now(), "Kraken", Some(vec![1.0, 0.75, 0.5, 0.25, 0.0]), true);
        assert_eq!(
            written(&five_titles),
            ["(█████ ████░ ███░░ █░░░░ ░░░░░) svolder · Kraken"]
        );
    }

    /// The title stack is pushed and popped with the sequences xterm names for it.
    #[test]
    fn the_title_stack_sequences_are_the_ones_xterm_names() {
        assert_eq!(push_title(), "\u{1b}[22;2t");
        assert_eq!(pop_title(), "\u{1b}[23;2t");
    }

    /// A device name carrying control characters reaches the title without them, so nothing a
    /// configuration file says can end the title's escape sequence early or start another.
    #[tokio::test(start_paused = true)]
    async fn control_characters_in_the_device_name_stay_out_of_the_title() {
        let (mut meter, titles) = meter();
        meter.update(now(), "BF\u{7}3\u{1b}]0;x", None, true);
        assert_eq!(written(&titles), ["svolder · BF3]0;x"]);
    }
}
