// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The `host_cover` request on the wire: the request, its chunked answer, the bound on a line
//! and the feature.

use svolder_host::cover::{Cover, MAX_COVER_BYTES};

use super::*;
use crate::socket::lines::MAX_LINE;

/// A picture of `len` bytes that is not valid anywhere but is a cover to the protocol.
fn picture(len: usize) -> Cover {
    Cover {
        bytes: (0..len).map(|i| (i * 7 + i / 251) as u8).collect(),
        mime: "image/png",
    }
}

/// The request names the hash and the offset; the offset may be left out and is then the start.
#[test]
fn the_host_cover_request_has_its_wire_shape() {
    let request = Request::HostCover {
        hash: "abc".into(),
        offset: 0,
    };
    let wire = r#"{"cmd":"host_cover","hash":"abc","offset":0}"#;
    assert_eq!(serde_json::to_string(&request).unwrap(), wire);
    assert_eq!(serde_json::from_str::<Request>(wire).unwrap(), request);
    assert_eq!(
        serde_json::from_str::<Request>(r#"{"cmd":"host_cover","hash":"abc"}"#).unwrap(),
        request
    );
    assert!(!request.is_stream());
    assert!(REQUEST_CMDS.contains(&"host_cover"));
    assert!(serde_json::from_str::<Request>(r#"{"cmd":"host_cover"}"#).is_err());
}

/// The feature is listed, so a front end asks before it sends the request.
#[test]
fn host_cover_is_a_listed_feature() {
    assert_eq!(FEATURE_HOST_COVER, "host_cover");
    assert!(FEATURES.contains(&FEATURE_HOST_COVER));
}

/// A chunk's answer carries the cover's hash, type, whole length, its own offset and its bytes
/// as unpadded base64url.
#[test]
fn a_chunk_answer_has_its_wire_shape() {
    let cover = Cover {
        bytes: vec![0xff, 0xd8, 0xff],
        mime: "image/jpeg",
    };
    let answer = cover_chunk("abc", &cover, 0);
    let wire = r#"{"result":"host_cover","hash":"abc","mime":"image/jpeg","total":3,"offset":0,"data":"_9j_"}"#;
    assert_eq!(serde_json::to_string(&answer).unwrap(), wire);
    assert_eq!(serde_json::from_str::<Response>(wire).unwrap(), answer);
}

/// Every chunk's line, with the id the socket adds, stays under the line bound even for the
/// largest cover a daemon reads, and the chunks put together are the picture.
#[test]
fn a_largest_cover_comes_in_chunks_that_fit_a_line() {
    let cover = picture(MAX_COVER_BYTES as usize);
    let mut reader = CoverReader::new("abc");
    let mut chunks = 0;
    let got = loop {
        let answer = cover_chunk("abc", &cover, reader.next_offset());
        let line = serde_json::to_string(&Reply::Response {
            id: u64::MAX,
            response: answer.clone(),
        })
        .unwrap();
        assert!(
            line.len() < MAX_LINE,
            "a chunk line is {} bytes",
            line.len()
        );
        chunks += 1;
        if let Some(done) = reader.push(answer).unwrap() {
            break done;
        }
    };
    assert_eq!(got, cover);
    assert_eq!(chunks, 8, "4 MiB in chunks of {COVER_CHUNK_BYTES}");
}

/// A cover past the bound is never sent, and an offset past its end is refused.
#[test]
fn a_chunk_is_refused_past_the_bound_or_the_end() {
    let refused = |answer: Response| matches!(answer, Response::Error { .. });
    assert!(refused(cover_chunk(
        "abc",
        &picture(MAX_COVER_BYTES as usize + 1),
        0
    )));
    let small = picture(10);
    assert!(refused(cover_chunk("abc", &small, 11)));
    assert!(
        refused(cover_chunk("abc", &small, 10)),
        "no empty tail chunk"
    );
}

/// The reader takes only what answers its own asks: another hash, an offset out of turn, a
/// total over the bound, a total that changes, a bad type or bad base64 end it.
#[test]
fn the_reader_refuses_a_chunk_that_is_not_the_next_of_its_cover() {
    let cover = picture(COVER_CHUNK_BYTES + 5);
    let first = cover_chunk("abc", &cover, 0);
    let second = cover_chunk("abc", &cover, COVER_CHUNK_BYTES as u64);

    let mut reader = CoverReader::new("abc");
    assert_eq!(reader.push(first.clone()), Ok(None));
    assert!(reader.push(first.clone()).is_err(), "the first again");
    assert_eq!(reader.push(second.clone()).unwrap(), Some(cover.clone()));

    let mut other = CoverReader::new("xyz");
    assert!(other.push(first.clone()).is_err(), "another hash");
    assert!(
        CoverReader::new("abc").push(second.clone()).is_err(),
        "out of turn"
    );

    // A second chunk that says the cover is another length, or another type, than the first did
    // is of another picture, whatever its offset.
    for edit in [
        &(|r: &mut Response| {
            if let Response::HostCover { total, .. } = r {
                *total += 1;
            }
        }) as &dyn Fn(&mut Response),
        &|r: &mut Response| {
            if let Response::HostCover { mime, .. } = r {
                *mime = "image/jpeg".into();
            }
        },
    ] {
        let mut reader = CoverReader::new("abc");
        assert_eq!(reader.push(first.clone()), Ok(None));
        let mut changed = second.clone();
        edit(&mut changed);
        let err = reader.push(changed).unwrap_err();
        assert!(err.contains("changed between chunks"), "{err}");
    }

    let Response::HostCover { .. } = first else {
        panic!("a chunk");
    };
    let tamper = |edit: &dyn Fn(&mut Response)| {
        let mut answer = first.clone();
        edit(&mut answer);
        CoverReader::new("abc").push(answer)
    };
    assert!(
        tamper(&|r| {
            if let Response::HostCover { total, .. } = r {
                *total = MAX_COVER_BYTES + 1;
            }
        })
        .is_err()
    );
    assert!(
        tamper(&|r| {
            if let Response::HostCover { mime, .. } = r {
                *mime = "text/html".into();
            }
        })
        .is_err()
    );
    assert!(
        tamper(&|r| {
            if let Response::HostCover { data, .. } = r {
                *data = "!!!!".into();
            }
        })
        .is_err()
    );
    assert!(
        CoverReader::new("abc").push(Response::Ack).is_err(),
        "not a chunk"
    );
}

/// An error answer ends the read with the daemon's words.
#[test]
fn the_reader_passes_a_refusal_on() {
    let refusal = Response::error(CODE_NOT_FOUND, "no cover with that hash".to_string());
    let err = CoverReader::new("abc").push(refusal).unwrap_err();
    assert!(err.contains("no cover with that hash"), "{err}");
}
