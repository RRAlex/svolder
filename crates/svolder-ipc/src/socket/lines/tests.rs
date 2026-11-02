// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use tokio::io::BufReader;

fn reader(bytes: Vec<u8>) -> LineReader<BufReader<std::io::Cursor<Vec<u8>>>> {
    LineReader::new(BufReader::new(std::io::Cursor::new(bytes)))
}

#[tokio::test]
async fn lines_come_out_as_bytes_without_their_terminators() {
    let mut lines = reader(b"one\ntwo\r\n\n\xff\xfe\nlast".to_vec());
    assert_eq!(
        lines.next_line().await.unwrap(),
        Line::Bytes(b"one".to_vec())
    );
    assert_eq!(
        lines.next_line().await.unwrap(),
        Line::Bytes(b"two".to_vec())
    );
    assert_eq!(lines.next_line().await.unwrap(), Line::Bytes(Vec::new()));
    assert_eq!(
        lines.next_line().await.unwrap(),
        Line::Bytes(b"\xff\xfe".to_vec())
    );
    assert_eq!(
        lines.next_line().await.unwrap(),
        Line::Bytes(b"last".to_vec())
    );
    assert_eq!(lines.next_line().await.unwrap(), Line::Eof);
}

/// A line of exactly the cap passes; one byte more is too long.
#[tokio::test]
async fn the_cap_is_on_the_line_without_its_newline() {
    let mut ok = vec![b'a'; MAX_LINE];
    ok.push(b'\n');
    assert!(matches!(
        reader(ok).next_line().await.unwrap(),
        Line::Bytes(b) if b.len() == MAX_LINE
    ));
    let mut long = vec![b'a'; MAX_LINE + 1];
    long.push(b'\n');
    assert_eq!(reader(long).next_line().await.unwrap(), Line::TooLong);
}

/// An over-long line is dropped without ever being held past the cap, and the line after
/// it reads normally.
#[tokio::test]
async fn an_over_long_line_is_never_buffered_past_the_cap() {
    let mut bytes = vec![b'a'; 3 * MAX_LINE];
    bytes.extend_from_slice(b"\nnext\n");
    let mut lines = reader(bytes);
    assert_eq!(lines.next_line().await.unwrap(), Line::TooLong);
    // The buffer never held more than the cap, and the source was read no further than
    // the cap plus the read-ahead buffers (8 KiB each: the chunk in hand and the next).
    assert!(
        lines.high_water <= MAX_LINE,
        "buffered {} bytes",
        lines.high_water
    );
    let taken = lines.inner.get_ref().position();
    assert!(
        taken <= (MAX_LINE + 16 * 1024) as u64,
        "took {taken} bytes from the source"
    );
    assert!(lines.skip_line().await.unwrap());
    assert_eq!(
        lines.next_line().await.unwrap(),
        Line::Bytes(b"next".to_vec())
    );
}
