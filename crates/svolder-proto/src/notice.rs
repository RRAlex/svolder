// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The copyright and licence notice svolder's binaries show.

/// Who holds svolder's copyright, as every notice states it.
pub const COPYRIGHT: &str = "Copyright (C) 2026 the svolder authors";

/// The licence and the warranty disclaimer in one line, for a screen with room for little.
pub const LICENCE: &str = "Free software under the GNU GPL, version 3 or later, with NO WARRANTY.";

/// Where the licence's full text is published.
pub const LICENCE_URL: &str = "https://gnu.org/licenses/gpl.html";

/// The calling crate's version with the copyright, licence and warranty lines under it, in
/// the layout the GNU coding standards give `--version`. A macro, so that the version is the
/// crate's that calls it and the result is a `&'static str` clap takes as `long_version`.
#[macro_export]
macro_rules! long_version {
    () => {
        concat!(
            env!("CARGO_PKG_VERSION"),
            "\nCopyright (C) 2026 the svolder authors",
            "\nLicense GPLv3+: GNU GPL version 3 or later <https://gnu.org/licenses/gpl.html>.",
            "\nThis is free software: you are free to change and redistribute it.",
            "\nThere is NO WARRANTY, to the extent permitted by law."
        )
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_second_line_of_the_version_block_is_the_copyright() {
        assert_eq!(crate::long_version!().lines().nth(1), Some(COPYRIGHT));
    }

    #[test]
    fn the_version_block_names_where_the_licence_is_published() {
        assert!(crate::long_version!().contains(LICENCE_URL));
    }

    #[test]
    fn the_version_block_starts_with_the_callers_version() {
        assert_eq!(
            crate::long_version!().lines().next(),
            Some(env!("CARGO_PKG_VERSION"))
        );
    }
}
