// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The probe against terminals that answer in the ways the pty harness's responders do, and
//! against one that never answers.

use super::*;

/// A terminal with a clock of its own: the answers it gives, each at the time it gives it.
struct Scripted {
    now: Duration,
    /// `(when, bytes)`, in order.
    replies: Vec<(Duration, Vec<u8>)>,
    written: Vec<u8>,
}

impl Scripted {
    fn answering(replies: &[(u64, &str)]) -> Scripted {
        Scripted {
            now: Duration::ZERO,
            replies: replies
                .iter()
                .map(|(ms, bytes)| (Duration::from_millis(*ms), bytes.as_bytes().to_vec()))
                .collect(),
            written: Vec::new(),
        }
    }
}

impl Line for Scripted {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        self.written.extend_from_slice(bytes);
        Ok(())
    }

    fn read(&mut self, buf: &mut [u8], within: Duration) -> std::io::Result<usize> {
        let until = self.now + within;
        match self.replies.first() {
            Some((when, _)) if *when <= until => {
                let (when, mut bytes) = self.replies.remove(0);
                self.now = self.now.max(when);
                let take = bytes.len().min(buf.len());
                buf[..take].copy_from_slice(&bytes[..take]);
                if take < bytes.len() {
                    self.replies.insert(0, (when, bytes.split_off(take)));
                }
                Ok(take)
            }
            _ => {
                self.now = until;
                Ok(0)
            }
        }
    }

    fn elapsed(&mut self) -> Duration {
        self.now
    }
}

const KITTY: &str = "\x1b_Gi=31;OK\x1b\\";
const DA1_PLAIN: &str = "\x1b[?62;22c";
const DA1_SIXEL: &str = "\x1b[?62;4;22c";
const CELL: &str = "\x1b[6;16;8t";
const STATUS: &str = "\x1b[0n";

fn probed(replies: &[(u64, &str)], window: Option<(u16, u16)>) -> (Drawn, Duration, Vec<u8>) {
    let mut line = Scripted::answering(replies);
    let query = Parser::query(false, QueryStdioOptions::default());
    let answers = ask(&mut line, &query, WAIT);
    let spent = line.elapsed();
    (interpret(&answers, window), spent, line.written)
}

#[test]
fn a_kitty_terminal_is_drawn_with_the_kitty_protocol() {
    let (drawn, spent, written) =
        probed(&[(1, KITTY), (1, DA1_PLAIN), (1, CELL), (1, STATUS)], None);
    assert_eq!(drawn.protocol, ProtocolType::Kitty);
    assert_eq!(drawn.cell, Some((8, 16)));
    assert!(spent < WAIT, "the status report ends the wait: {spent:?}");
    let sent = String::from_utf8(written).unwrap();
    assert!(
        sent.contains("_Gi=31"),
        "the kitty query went out: {sent:?}"
    );
    assert!(sent.ends_with("\x1b[5n"), "the status report is asked last");
}

#[test]
fn a_sixel_terminal_is_drawn_with_sixel() {
    let (drawn, _, _) = probed(&[(1, DA1_SIXEL), (1, CELL), (1, STATUS)], None);
    assert_eq!(drawn.protocol, ProtocolType::Sixel);
    assert_eq!(drawn.cell, Some((8, 16)));
}

#[test]
fn a_plain_terminal_is_drawn_with_half_blocks() {
    let (drawn, _, _) = probed(&[(1, DA1_PLAIN), (1, STATUS)], None);
    assert_eq!(drawn.protocol, ProtocolType::Halfblocks);
    assert_eq!(drawn.cell, None);
}

#[test]
fn kitty_beats_sixel_when_a_terminal_answers_both() {
    let (drawn, _, _) = probed(&[(1, DA1_SIXEL), (1, KITTY), (1, CELL), (1, STATUS)], None);
    assert_eq!(drawn.protocol, ProtocolType::Kitty);
}

#[test]
fn no_cell_size_means_half_blocks_whatever_else_was_said() {
    let (drawn, _, _) = probed(&[(1, KITTY), (1, DA1_SIXEL), (1, STATUS)], None);
    assert_eq!(drawn.protocol, ProtocolType::Halfblocks);
    let (drawn, _, _) = probed(&[(1, KITTY), (1, STATUS)], Some((9, 18)));
    assert_eq!(
        drawn.protocol,
        ProtocolType::Kitty,
        "the window's size serves"
    );
    assert_eq!(drawn.cell, Some((9, 18)));
}

#[test]
fn a_silent_terminal_costs_the_wait_and_no_more_and_gets_half_blocks() {
    let (drawn, spent, _) = probed(&[], None);
    assert_eq!(drawn.protocol, ProtocolType::Halfblocks);
    assert_eq!(spent, WAIT);
}

#[test]
fn an_answer_that_comes_after_the_wait_is_not_waited_for() {
    let (drawn, spent, _) = probed(&[(500, KITTY), (500, CELL), (500, STATUS)], None);
    assert_eq!(drawn.protocol, ProtocolType::Halfblocks);
    assert_eq!(spent, WAIT);
}

#[test]
fn an_answer_split_across_reads_is_put_together() {
    let (drawn, _, _) = probed(
        &[
            (1, "\x1b_Gi=3"),
            (2, "1;OK\x1b"),
            (3, "\\\x1b[6;1"),
            (4, "6;8t\x1b[0n"),
        ],
        None,
    );
    assert_eq!(drawn.protocol, ProtocolType::Kitty);
    assert_eq!(drawn.cell, Some((8, 16)));
}

#[test]
fn the_picker_takes_the_protocol_and_the_cell_size() {
    let picker = picker_of(Drawn {
        protocol: ProtocolType::Kitty,
        cell: Some((8, 16)),
    });
    assert_eq!(picker.protocol_type(), ProtocolType::Kitty);
    assert_eq!(
        (picker.font_size().width, picker.font_size().height),
        (8, 16)
    );
    let picker = picker_of(Drawn {
        protocol: ProtocolType::Halfblocks,
        cell: None,
    });
    assert_eq!(picker.protocol_type(), ProtocolType::Halfblocks);
}
