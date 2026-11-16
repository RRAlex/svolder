// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! SHA-256 as lower-case hex, for the checksums of sources and converted files.

use ring::digest::{Context, SHA256};

/// An incremental SHA-256.
pub(crate) struct Sha(Context);

impl Sha {
    pub(crate) fn new() -> Sha {
        Sha(Context::new(&SHA256))
    }

    pub(crate) fn update(&mut self, bytes: &[u8]) {
        self.0.update(bytes);
    }

    pub(crate) fn finish(self) -> String {
        hex(self.0.finish().as_ref())
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// The SHA-256 of `bytes` as 64 lower-case hex digits.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut sha = Sha::new();
    sha.update(bytes);
    sha.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_known_answers() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn pieces_give_the_whole_answer() {
        let mut sha = Sha::new();
        sha.update(b"a");
        sha.update(b"bc");
        assert_eq!(sha.finish(), sha256_hex(b"abc"));
    }
}
