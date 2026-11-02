// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `docs/usage/environment.md`'s table cannot drift from `svolder help env
//! --markdown`: this runs the built binary and compares its output, between the file's
//! `<!-- env-table:begin -->`/`<!-- env-table:end -->` markers, to what is actually there,
//! failing naming `make docs-env` when the two differ.

use std::path::Path;
use std::process::Command;

const BEGIN: &str = "<!-- env-table:begin -->";
const END: &str = "<!-- env-table:end -->";

/// This workspace's root: `bins/svolder` sits two directories under it.
fn workspace_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("bins/svolder sits two levels under the workspace root")
        .to_path_buf()
}

/// The text strictly between the first `BEGIN` line and the following `END` line.
fn marked_block(doc: &str) -> &str {
    let after_begin = doc
        .find(BEGIN)
        .map(|i| i + BEGIN.len())
        .expect("docs/usage/environment.md has an env-table:begin marker");
    let end_at = doc[after_begin..]
        .find(END)
        .expect("docs/usage/environment.md has an env-table:end marker after env-table:begin");
    doc[after_begin..after_begin + end_at].trim_matches('\n')
}

#[test]
fn the_doc_table_matches_svolder_help_env_markdown() {
    let root = workspace_root();
    let doc = std::fs::read_to_string(root.join("docs/usage/environment.md"))
        .expect("docs/usage/environment.md reads");
    let block = marked_block(&doc);

    let output = Command::new(env!("CARGO_BIN_EXE_svolder"))
        .env("SVOLDER_HOST", "off")
        .args(["help", "env", "--markdown"])
        .output()
        .expect("svolder help env --markdown runs");
    assert!(output.status.success());
    let rendered = String::from_utf8_lossy(&output.stdout);
    let rendered = rendered.trim_matches('\n');

    assert_eq!(
        block, rendered,
        "docs/usage/environment.md's environment-variable table has drifted from `svolder help env \
         --markdown`; run `make docs-env` and commit the result"
    );
}
