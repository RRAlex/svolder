// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The byte tap on the pty's output: it keeps every byte the program writes, feeds the screen
//! parser, recognises the queries a program may write to a terminal and answers them the way the
//! chosen [`Responder`] says.

use std::io::Write;
use std::sync::{Condvar, Mutex};

/// How a terminal answers the queries a program writes to it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Responder {
    /// Silence, as some tmux and SSH set-ups are.
    None,
    /// DA1 without the sixel attribute and the status report (`CSI 5n`), nothing graphical.
    Plain,
    /// DA1 with attribute 4, and the cell and window size in pixels (`CSI 16t`, `CSI 14t`).
    Sixel,
    /// The kitty graphics queries answered `OK`, the cell and window size, beside the plain
    /// answers.
    Kitty,
}

impl Responder {
    /// The terminal's size in pixels this responder's terminal reports through the pty's window
    /// size too, `(0, 0)` where it has no graphics: what the picker of ratatui-image reads when
    /// no answer to its cell-size query comes.
    pub fn pixels(self, cols: u16, rows: u16) -> (u16, u16) {
        match self {
            Responder::None | Responder::Plain => (0, 0),
            Responder::Sixel | Responder::Kitty => (cols * CELL_PX.0, rows * CELL_PX.1),
        }
    }
}

/// A query a program wrote to the terminal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Query {
    /// Primary device attributes, `CSI c`.
    Da1,
    /// Secondary device attributes, `CSI > c`.
    Da2,
    /// Device status report, `CSI 5 n`.
    Status,
    /// Cursor position report, `CSI 6 n`.
    CursorPosition,
    /// Cell size in pixels, `CSI 16 t`.
    CellSize,
    /// Window size in pixels, `CSI 14 t`.
    WindowPixels,
    /// Text area size in cells, `CSI 18 t`.
    TextArea,
    /// The terminal's name and version, `CSI > q`.
    Version,
    /// The kitty keyboard protocol's flags, `CSI ? u`.
    KeyboardFlags,
    /// A kitty graphics query (an APC `G` with `a=q`), with its image id.
    KittyGraphics(String),
    /// The terminal's background colour, `OSC 11 ; ?`.
    Background,
}

/// The pixel size of one cell the sixel responder reports.
pub const CELL_PX: (u16, u16) = (8, 16);

/// What the tap shares with the harness, behind one lock.
pub struct Tap {
    /// Every byte the program wrote, in order.
    pub raw: Vec<u8>,
    pub parser: vt100::Parser,
    pub responder: Responder,
    pub asked: Vec<Query>,
    /// Every answer written back, in order.
    pub answered: Vec<Vec<u8>>,
    /// The program closed its end: the reader saw end of file.
    pub closed: bool,
    /// The terminal's size in cells, `(cols, rows)`.
    pub size: (u16, u16),
    /// Where the scan for sequences has got to in `raw`.
    scanned: usize,
    /// Where the parser has got to in `raw`.
    fed: usize,
    /// Where an open synchronized update (`CSI ? 2026 h`) began: a terminal draws nothing of it
    /// until the end (`CSI ? 2026 l`), and the parser, which does not know the mode, is held
    /// back the same way so a wait never sees half a frame.
    hold: Option<usize>,
}

/// The tap and the condition variable that wakes a waiter on every change.
pub struct Shared {
    pub tap: Mutex<Tap>,
    pub changed: Condvar,
}

impl Shared {
    /// The tap, even after a thread panicked holding it: a failed wait is the case's own
    /// failure and must not turn every later read into a second one.
    pub fn lock(&self) -> std::sync::MutexGuard<'_, Tap> {
        self.tap
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn new(cols: u16, rows: u16, responder: Responder) -> Shared {
        Shared {
            tap: Mutex::new(Tap {
                raw: Vec::new(),
                parser: vt100::Parser::new(
                    rows.try_into().expect("rows are not zero"),
                    cols.try_into().expect("columns are not zero"),
                    0,
                ),
                responder,
                asked: Vec::new(),
                answered: Vec::new(),
                closed: false,
                size: (cols, rows),
                scanned: 0,
                fed: 0,
                hold: None,
            }),
            changed: Condvar::new(),
        }
    }

    /// Take in what the program wrote and answer what it asked, writing answers to `master`.
    pub fn take(&self, chunk: &[u8], master: &mut impl Write) {
        let mut tap = self.lock();
        tap.raw.extend_from_slice(chunk);
        tap.scan(master);
        drop(tap);
        self.changed.notify_all();
    }

    /// The program's end closed: nothing more comes.
    pub fn close(&self) {
        self.lock().closed = true;
        self.changed.notify_all();
    }
}

/// One control string found in the byte stream.
enum Found {
    /// A complete one, ending at this offset (exclusive), and what it asks, if it is a query.
    Complete(usize, Option<Query>),
    /// Cut short by the end of what has arrived.
    Partial,
}

impl Tap {
    fn scan(&mut self, master: &mut impl Write) {
        loop {
            let Some(rel) = self.raw[self.scanned..].iter().position(|b| *b == 0x1b) else {
                self.scanned = self.raw.len();
                break;
            };
            let start = self.scanned + rel;
            match sequence_at(&self.raw[start..]) {
                Found::Partial => {
                    self.scanned = start;
                    break;
                }
                Found::Complete(len, query) => {
                    let end = start + len;
                    self.scanned = end;
                    match &self.raw[start..end] {
                        b"\x1b[?2026h" => self.hold = Some(start),
                        b"\x1b[?2026l" => self.hold = None,
                        _ => {}
                    }
                    if let Some(query) = query {
                        // The cursor a position report answers is where the program put it
                        // by the time it asked, so the parser is brought up to the query first.
                        self.feed_to(end);
                        self.answer(query, master);
                    }
                }
            }
        }
        self.feed_to(self.raw.len());
    }

    fn feed_to(&mut self, end: usize) {
        let end = end.min(self.hold.unwrap_or(usize::MAX));
        if end > self.fed {
            self.parser.process(&self.raw[self.fed..end]);
            self.fed = end;
        }
    }

    fn answer(&mut self, query: Query, master: &mut impl Write) {
        self.asked.push(query.clone());
        let (cols, rows) = self.size;
        let sixel = matches!(self.responder, Responder::Sixel);
        let sized = matches!(self.responder, Responder::Sixel | Responder::Kitty);
        let reply: Option<Vec<u8>> = match (self.responder, &query) {
            (Responder::None, _) => None,
            (_, Query::Da1) => Some(if sixel {
                b"\x1b[?62;4;22c".to_vec()
            } else {
                b"\x1b[?62;c".to_vec()
            }),
            (_, Query::Status) => Some(b"\x1b[0n".to_vec()),
            (_, Query::CursorPosition) => {
                let (row, col) = self.parser.screen().cursor_position();
                Some(format!("\x1b[{};{}R", row + 1, col + 1).into_bytes())
            }
            (_, Query::CellSize) if sized => {
                Some(format!("\x1b[6;{};{}t", CELL_PX.1, CELL_PX.0).into_bytes())
            }
            (_, Query::WindowPixels) if sized => Some(
                format!(
                    "\x1b[4;{};{}t",
                    u32::from(rows) * u32::from(CELL_PX.1),
                    u32::from(cols) * u32::from(CELL_PX.0)
                )
                .into_bytes(),
            ),
            (_, Query::TextArea) if sized => Some(format!("\x1b[8;{rows};{cols}t").into_bytes()),
            (_, Query::Background) => Some(b"\x1b]11;rgb:0000/0000/0000\x07".to_vec()),
            (Responder::Kitty, Query::KittyGraphics(id)) => {
                Some(format!("\x1b_Gi={id};OK\x1b\\").into_bytes())
            }
            _ => None,
        };
        if let Some(reply) = reply {
            let _ = master.write_all(&reply);
            let _ = master.flush();
            self.answered.push(reply);
        }
    }
}

/// The control string that starts at `bytes[0]` (an ESC).
fn sequence_at(bytes: &[u8]) -> Found {
    let Some(kind) = bytes.get(1) else {
        return Found::Partial;
    };
    match kind {
        b'[' => {
            let mut at = 2;
            while let Some(b) = bytes.get(at) {
                if (0x40..=0x7e).contains(b) {
                    let body = &bytes[2..at];
                    return Found::Complete(at + 1, csi_query(body, *b));
                }
                at += 1;
            }
            Found::Partial
        }
        b'_' | b'P' | b']' | b'^' | b'X' => {
            let mut at = 2;
            while let Some(b) = bytes.get(at) {
                let (ends, len) = match b {
                    0x07 => (true, at + 1),
                    0x1b if bytes.get(at + 1) == Some(&b'\\') => (true, at + 2),
                    0x1b if at + 1 >= bytes.len() => return Found::Partial,
                    _ => (false, 0),
                };
                if ends {
                    let body = &bytes[2..at];
                    let query = match kind {
                        b'_' => apc_query(body),
                        b']' => (body == b"11;?").then_some(Query::Background),
                        _ => None,
                    };
                    return Found::Complete(len, query);
                }
                at += 1;
            }
            Found::Partial
        }
        _ => Found::Complete(2, None),
    }
}

/// The query a CSI with this parameter text and final byte is, if it is one.
fn csi_query(body: &[u8], fin: u8) -> Option<Query> {
    let text = std::str::from_utf8(body).ok()?;
    match (text, fin) {
        ("" | "0", b'c') => Some(Query::Da1),
        (">" | ">0", b'c') => Some(Query::Da2),
        ("5", b'n') => Some(Query::Status),
        ("6", b'n') => Some(Query::CursorPosition),
        ("16", b't') => Some(Query::CellSize),
        ("14", b't') => Some(Query::WindowPixels),
        ("18", b't') => Some(Query::TextArea),
        (">" | ">0", b'q') => Some(Query::Version),
        ("?", b'u') => Some(Query::KeyboardFlags),
        _ => None,
    }
}

/// The query an APC string is, if it is a kitty graphics query: `G`, then `key=value` pairs with
/// `a=q`, then optionally `;payload`.
fn apc_query(body: &[u8]) -> Option<Query> {
    let text = std::str::from_utf8(body).ok()?.strip_prefix('G')?;
    let keys = text.split(';').next()?;
    let mut id = None;
    let mut query = false;
    for pair in keys.split(',') {
        match pair.split_once('=') {
            Some(("i", v)) => id = Some(v.to_string()),
            Some(("a", "q")) => query = true,
            _ => {}
        }
    }
    query.then(|| Query::KittyGraphics(id.unwrap_or_else(|| "0".to_string())))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What the terminal wrote back for `chunks` of program output, with this responder.
    fn replies(responder: Responder, chunks: &[&[u8]]) -> (Vec<Query>, Vec<Vec<u8>>, Shared) {
        let shared = Shared::new(80, 24, responder);
        let mut back = Vec::new();
        for chunk in chunks {
            shared.take(chunk, &mut back);
        }
        let tap = shared.tap.lock().unwrap();
        let (asked, answered) = (tap.asked.clone(), tap.answered.clone());
        drop(tap);
        (asked, answered, shared)
    }

    const PROBE: &[u8] = b"\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\\x1b[c\x1b[5n\x1b[16t\x1b[14t";

    #[test]
    fn a_silent_terminal_records_the_queries_and_answers_none() {
        let (asked, answered, _) = replies(Responder::None, &[PROBE]);
        assert_eq!(
            asked,
            [
                Query::KittyGraphics("31".into()),
                Query::Da1,
                Query::Status,
                Query::CellSize,
                Query::WindowPixels
            ]
        );
        assert!(answered.is_empty());
    }

    #[test]
    fn the_plain_terminal_answers_da1_without_sixel_and_the_status_report() {
        let (_, answered, _) = replies(Responder::Plain, &[PROBE]);
        assert_eq!(answered, [b"\x1b[?62;c".to_vec(), b"\x1b[0n".to_vec()]);
    }

    #[test]
    fn the_sixel_terminal_says_four_and_gives_its_cell_size() {
        let (_, answered, _) = replies(Responder::Sixel, &[PROBE]);
        assert_eq!(
            answered,
            [
                b"\x1b[?62;4;22c".to_vec(),
                b"\x1b[0n".to_vec(),
                b"\x1b[6;16;8t".to_vec(),
                b"\x1b[4;384;640t".to_vec()
            ]
        );
    }

    #[test]
    fn the_kitty_terminal_answers_the_graphics_query_with_its_id_and_gives_its_sizes() {
        let (_, answered, _) = replies(Responder::Kitty, &[PROBE]);
        assert_eq!(
            answered,
            [
                b"\x1b_Gi=31;OK\x1b\\".to_vec(),
                b"\x1b[?62;c".to_vec(),
                b"\x1b[0n".to_vec(),
                b"\x1b[6;16;8t".to_vec(),
                b"\x1b[4;384;640t".to_vec()
            ]
        );
    }

    /// The query string ratatui-image 11.1.0's picker writes (kitty probes with and without
    /// compression, DA1, cell size, background colour, the text-sizing cursor probes, status
    /// report), in its order; the sizes it needs to pick kitty are answered.
    const PICKER: &[u8] = b"\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\\
        \x1b_Gi=32,s=1,v=1,a=q,t=d,f=24,o=z;eJxjYGAAAAADAAE=\x1b\\\
        \x1b[c\x1b[16t\x1b]11;?\x07\
        \x1b[6n\x1b]66;w=2; \x07\x1b[6n\x1b]66;s=2; \x07\x1b[6n\x1b[5n";

    #[test]
    fn every_query_of_the_pickers_string_is_recognised_and_answered_in_order() {
        let (asked, answered, _) = replies(Responder::Kitty, &[PICKER]);
        assert_eq!(
            asked,
            [
                Query::KittyGraphics("31".into()),
                Query::KittyGraphics("32".into()),
                Query::Da1,
                Query::CellSize,
                Query::Background,
                Query::CursorPosition,
                Query::CursorPosition,
                Query::CursorPosition,
                Query::Status
            ]
        );
        assert_eq!(answered.len(), asked.len());
        assert_eq!(answered[0], b"\x1b_Gi=31;OK\x1b\\".to_vec());
        assert_eq!(answered[3], b"\x1b[6;16;8t".to_vec());
        assert_eq!(answered.last().unwrap(), &b"\x1b[0n".to_vec());
        // Silence answers none of it, and the plain terminal gives no cell size.
        assert!(replies(Responder::None, &[PICKER]).1.is_empty());
        let (_, plain, _) = replies(Responder::Plain, &[PICKER]);
        assert!(!plain.iter().any(|a| a.ends_with(b"t")));
    }

    #[test]
    fn the_pty_size_in_pixels_matches_the_cell_size_the_responder_gives() {
        assert_eq!(Responder::Kitty.pixels(80, 24), (640, 384));
        assert_eq!(Responder::Sixel.pixels(120, 40), (960, 640));
        assert_eq!(Responder::None.pixels(80, 24), (0, 0));
        assert_eq!(Responder::Plain.pixels(80, 24), (0, 0));
    }

    #[test]
    fn a_query_cut_in_two_reads_is_answered_once() {
        let (asked, answered, _) = replies(
            Responder::Plain,
            &[b"hello\x1b_Gi=7,a=", b"q;AA\x1b", b"\\\x1b[", b"c"],
        );
        assert_eq!(asked, [Query::KittyGraphics("7".into()), Query::Da1]);
        assert_eq!(answered.len(), 1);
    }

    #[test]
    fn a_graphics_payload_that_is_not_a_query_and_the_title_stack_are_not_asked() {
        let (asked, _, _) = replies(
            Responder::Plain,
            &[b"\x1b_Ga=T,f=24,i=3;AAAA\x1b\\\x1b[22;2t\x1b[23;2t\x1bP0;0;0q\"1;1\x1b\\"],
        );
        assert!(asked.is_empty(), "{asked:?}");
    }

    #[test]
    fn the_position_report_names_where_the_cursor_was_when_it_was_asked() {
        let (_, answered, _) = replies(Responder::Plain, &[b"\x1b[3;5Hab\x1b[6n\x1b[10;10H"]);
        assert_eq!(answered, [b"\x1b[3;7R".to_vec()]);
    }

    #[test]
    fn a_synchronized_update_reaches_the_screen_whole() {
        let shared = Shared::new(80, 24, Responder::None);
        let mut back = Vec::new();
        shared.take(b"\x1b[Hold", &mut back);
        shared.take(b"\x1b[?2026h\x1b[Hnew", &mut back);
        let seen = |shared: &Shared| shared.tap.lock().unwrap().parser.screen().contents();
        assert!(seen(&shared).starts_with("old"), "half a frame was drawn");
        shared.take(b"!!\x1b[?2026l", &mut back);
        assert!(seen(&shared).starts_with("new!!"));
        assert!(shared.tap.lock().unwrap().raw.starts_with(b"\x1b[Hold"));
    }
}
