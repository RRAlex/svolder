// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The followed player's cover as a request: `Request::HostCover { hash, offset }`.
//!
//! The host view names the cover the daemon holds by its hash ([`PlayingView::cover`]); the
//! bytes come from this request, so a front end on another machine shows the art too. The daemon
//! answers from the cover it holds now and only for the hash that is current: any other hash
//! (an earlier cover, a made-up one, or no art at all) is an error with
//! [`CODE_NOT_FOUND`], and the front end asks the host view again. A
//! daemon that has the request lists the feature
//! [`FEATURE_HOST_COVER`](super::FEATURE_HOST_COVER).
//!
//! A picture is up to [`MAX_COVER_BYTES`] (4 MiB) and a line of the socket is at most 1 MiB, so
//! the bytes come in chunks. Each answer is [`Response::HostCover`]: the hash, the media type
//! (`image/jpeg`, `image/png` or `image/webp`), the picture's whole length `total`, the chunk's
//! `offset`, and the chunk's bytes as unpadded base64url in `data`. A chunk is at most
//! [`COVER_CHUNK_BYTES`] (512 KiB) of picture; base64 costs a third more, so a line is about
//! 683 KiB and stays under the bound with its envelope, and the largest cover is eight round
//! trips. The front end asks `offset` 0 first, then `offset` plus the length it got, until
//! it holds `total` bytes; [`CoverReader`] does that bookkeeping and refuses an answer that is
//! not the next of its own cover. Every ask names the hash, so a cover that changes midway ends
//! the read with `not_found` rather than mixing two pictures.
//!
//! [`PlayingView::cover`]: crate::host::PlayingView::cover

use svolder_host::cover::{Cover, MAX_COVER_BYTES};

use super::{CODE_NOT_FOUND, CODE_USAGE, Response};
use crate::token::{base64url, base64url_decode};

/// The most picture bytes one [`Response::HostCover`] carries.
pub const COVER_CHUNK_BYTES: usize = 512 * 1024;

/// The answer to a `host_cover` request for `hash` at `offset`, `cover` being the cover the
/// daemon holds under it: the chunk that starts there, or an error answer. A cover past
/// [`MAX_COVER_BYTES`] is never sent, and an offset at or past the end is a usage error.
pub fn cover_chunk(hash: &str, cover: &Cover, offset: u64) -> Response {
    let total = cover.bytes.len();
    if total as u64 > MAX_COVER_BYTES {
        return Response::error(
            CODE_NOT_FOUND,
            format!("the cover is larger than {MAX_COVER_BYTES} bytes"),
        );
    }
    let Some(start) = usize::try_from(offset).ok().filter(|start| *start < total) else {
        return Response::error(
            CODE_USAGE,
            format!("offset {offset} is past the end of a cover of {total} bytes"),
        );
    };
    let end = (start + COVER_CHUNK_BYTES).min(total);
    Response::HostCover {
        hash: hash.to_string(),
        mime: cover.mime.to_string(),
        total: total as u64,
        offset,
        data: base64url(&cover.bytes[start..end]),
    }
}

/// A front end's read of one cover, chunk by chunk.
#[derive(Debug)]
pub struct CoverReader {
    /// The hash asked for.
    hash: String,
    /// What has arrived.
    bytes: Vec<u8>,
    /// The whole length and the type, from the first chunk.
    seen: Option<(u64, &'static str)>,
}

impl CoverReader {
    /// A read of the cover with `hash`.
    pub fn new(hash: &str) -> CoverReader {
        CoverReader {
            hash: hash.to_string(),
            bytes: Vec::new(),
            seen: None,
        }
    }

    /// The offset the next request asks for.
    pub fn next_offset(&self) -> u64 {
        self.bytes.len() as u64
    }

    /// Take the daemon's answer to the request at [`CoverReader::next_offset`]: the whole
    /// cover once the last chunk is in, `None` while more is due. An error is the daemon's
    /// refusal in its words, or why the answer is not the next chunk of this cover.
    pub fn push(&mut self, answer: Response) -> Result<Option<Cover>, String> {
        let Response::HostCover {
            hash,
            mime,
            total,
            offset,
            data,
        } = answer
        else {
            return Err(match answer {
                Response::Error { message, .. } => message,
                other => format!("not a cover chunk: {other:?}"),
            });
        };
        if hash != self.hash {
            return Err("the chunk is of another cover".to_string());
        }
        if offset != self.next_offset() {
            return Err(format!(
                "chunk at {offset}, expected {}",
                self.next_offset()
            ));
        }
        let mime = known_mime(&mime).ok_or_else(|| format!("unknown media type {mime}"))?;
        if total == 0 || total > MAX_COVER_BYTES {
            return Err(format!("a cover of {total} bytes is out of bounds"));
        }
        if self.seen.is_some_and(|seen| seen != (total, mime)) {
            return Err("the cover changed between chunks".to_string());
        }
        self.seen = Some((total, mime));
        let bytes = base64url_decode(&data).ok_or("the chunk is not base64")?;
        if bytes.is_empty() || self.bytes.len() as u64 + bytes.len() as u64 > total {
            return Err("the chunk does not fit the cover".to_string());
        }
        self.bytes.extend_from_slice(&bytes);
        Ok((self.bytes.len() as u64 == total).then(|| Cover {
            bytes: std::mem::take(&mut self.bytes),
            mime,
        }))
    }
}

/// The media type a daemon names, as the static word a [`Cover`] holds; `None` for any other.
fn known_mime(mime: &str) -> Option<&'static str> {
    ["image/jpeg", "image/png", "image/webp"]
        .into_iter()
        .find(|known| *known == mime)
}
