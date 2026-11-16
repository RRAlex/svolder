// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The VU needle's golden path ([`svolder_host::vu::golden`]) as the text of
//! `scripts/web-checks/fixtures/vu-ballistics.json`, printed to standard output: `make vu-golden`
//! writes the file, `make vu-golden-check` compares it with a fresh run. The web page's needle
//! has to follow the same path.

fn main() {
    print!("{}", svolder_host::vu::golden().json());
}
