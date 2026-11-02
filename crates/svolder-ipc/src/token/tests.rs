// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The token store against temporary state homes. Tokens are compared with `assert!(a == b)`
//! and never formatted, so a failing test cannot print one.

use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use super::*;
use crate::test_support::TempPath;

/// A state home of this test's own, removed with everything in it when the test ends.
fn temp_home(tag: &str) -> TempPath {
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("a clock after 1970")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("svolder-token-{tag}-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    TempPath(dir)
}

fn mode_of(path: &Path) -> u32 {
    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
}

fn write_secret(path: &Path, text: &str, mode: u32) {
    std::fs::write(path, text).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
}

#[test]
fn base64url_encodes_without_padding() {
    assert_eq!(base64url(b""), "");
    assert_eq!(base64url(b"f"), "Zg");
    assert_eq!(base64url(b"fo"), "Zm8");
    assert_eq!(base64url(b"foo"), "Zm9v");
    assert_eq!(base64url(&[0xfb, 0xff, 0xfe]), "-__-");
    assert_eq!(base64url(&[0u8; 32]).len(), 43);
}

#[test]
fn a_generated_token_is_43_url_safe_characters_and_differs_each_time() {
    let a = generate().unwrap();
    let b = generate().unwrap();
    assert_eq!(a.len(), 43);
    assert!(
        a.bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
    );
    assert!(a != b);
}

#[test]
fn the_generated_file_is_created_0600_and_stable_across_loads() {
    let home = temp_home("gen");
    let first = resolve_with(Store::MCP, None, None, &home).unwrap();
    let path = generated_path_in(Store::MCP, &home);
    assert!(matches!(&first.source, TokenSource::Generated(p) if *p == path));
    assert_eq!(mode_of(&path), 0o600);
    assert_eq!(first.value().len(), 43);
    let second = resolve_with(Store::MCP, None, None, &home).unwrap();
    assert!(first.value() == second.value());
}

#[test]
fn rotate_replaces_the_generated_token_and_keeps_0600() {
    let home = temp_home("rot");
    let before = resolve_with(Store::MCP, None, None, &home).unwrap();
    let after = rotate_with(Store::MCP, None, None, &home).unwrap();
    assert!(before.value() != after.value());
    assert_eq!(mode_of(&generated_path_in(Store::MCP, &home)), 0o600);
    let again = resolve_with(Store::MCP, None, None, &home).unwrap();
    assert!(after.value() == again.value());
}

#[test]
fn the_environment_outranks_the_token_file_and_generates_nothing() {
    let home = temp_home("env");
    let file = home.join("tok");
    write_secret(&file, "from-the-file\n", 0o600);
    let t = resolve_with(Store::MCP, Some("from-the-env".into()), Some(&file), &home).unwrap();
    assert!(t.value() == "from-the-env");
    assert!(matches!(t.source, TokenSource::Env("SVOLDER_MCP_TOKEN")));
    assert!(!generated_path_in(Store::MCP, &home).exists());
}

#[test]
fn an_empty_environment_value_counts_as_unset() {
    let home = temp_home("envempty");
    let t = resolve_with(Store::MCP, Some("  ".into()), None, &home).unwrap();
    assert!(matches!(t.source, TokenSource::Generated(_)));
}

#[test]
fn a_token_file_is_read_and_trimmed_and_generates_nothing() {
    let home = temp_home("file");
    let file = home.join("tok");
    write_secret(&file, "from-the-file\n", 0o600);
    let t = resolve_with(Store::MCP, None, Some(&file), &home).unwrap();
    assert!(t.value() == "from-the-file");
    assert!(matches!(&t.source, TokenSource::File(p) if *p == file));
    assert!(!generated_path_in(Store::MCP, &home).exists());
}

#[test]
fn a_token_file_readable_by_others_is_refused_naming_its_mode() {
    let home = temp_home("mode");
    let file = home.join("tok");
    write_secret(&file, "from-the-file\n", 0o644);
    let err = resolve_with(Store::MCP, None, Some(&file), &home)
        .unwrap_err()
        .to_string();
    assert!(err.contains("0644"), "{err}");
    assert!(err.contains(&file.display().to_string()), "{err}");
    assert!(!err.contains("from-the-file"));
    // Group-readable is refused too.
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o640)).unwrap();
    assert!(resolve_with(Store::MCP, None, Some(&file), &home).is_err());
}

#[test]
fn a_missing_or_empty_token_file_is_an_error_naming_the_file() {
    let home = temp_home("missing");
    let file = home.join("tok");
    let err = resolve_with(Store::MCP, None, Some(&file), &home)
        .unwrap_err()
        .to_string();
    assert!(err.contains(&file.display().to_string()), "{err}");
    write_secret(&file, "\n", 0o600);
    let err = resolve_with(Store::MCP, None, Some(&file), &home)
        .unwrap_err()
        .to_string();
    assert!(err.contains("empty"), "{err}");
}

#[test]
fn a_generated_file_readable_by_others_is_refused_naming_its_mode() {
    let home = temp_home("genmode");
    resolve_with(Store::MCP, None, None, &home).unwrap();
    let path = generated_path_in(Store::MCP, &home);
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o604)).unwrap();
    let err = resolve_with(Store::MCP, None, None, &home)
        .unwrap_err()
        .to_string();
    assert!(err.contains("0604"), "{err}");
}

#[test]
fn rotate_refuses_an_outside_source_naming_it() {
    let home = temp_home("rotref");
    let file = home.join("tok");
    write_secret(&file, "from-the-file\n", 0o600);
    let err = rotate_with(Store::MCP, None, Some(&file), &home)
        .unwrap_err()
        .to_string();
    assert!(err.contains(&file.display().to_string()), "{err}");
    let err = rotate_with(Store::MCP, Some("from-the-env".into()), None, &home)
        .unwrap_err()
        .to_string();
    assert!(err.contains("SVOLDER_MCP_TOKEN"), "{err}");
    assert!(!err.contains("from-the-env"));
    // The file is untouched.
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "from-the-file\n");
}

#[test]
fn the_token_never_prints_in_debug() {
    let home = temp_home("dbg");
    let t = resolve_with(Store::MCP, Some("super-secret-value".into()), None, &home).unwrap();
    assert!(!format!("{t:?}").contains("super-secret-value"));
}

#[test]
fn token_matches_compares_the_whole_value() {
    assert!(token_matches("abc", "abc"));
    assert!(!token_matches("abc", "abd"));
    assert!(!token_matches("abc", "ab"));
    assert!(!token_matches("abc", "abcd"));
    assert!(!token_matches("abc", ""));
    assert!(!token_matches("", "x"));
}

#[test]
fn a_token_with_whitespace_or_several_lines_is_refused_from_any_source() {
    let home = temp_home("ws");
    let file = home.join("tok");
    write_secret(&file, "two\nlines\n", 0o600);
    let err = resolve_with(Store::MCP, None, Some(&file), &home)
        .unwrap_err()
        .to_string();
    assert!(err.contains(&file.display().to_string()), "{err}");
    assert!(!err.contains("two"));
    write_secret(&file, "has space\n", 0o600);
    assert!(resolve_with(Store::MCP, None, Some(&file), &home).is_err());
    let err = resolve_with(Store::MCP, Some("a b".into()), None, &home)
        .unwrap_err()
        .to_string();
    assert!(err.contains("SVOLDER_MCP_TOKEN"), "{err}");
}

#[test]
fn a_non_utf8_environment_token_is_an_error_naming_the_variable() {
    use std::os::unix::ffi::OsStringExt;
    // The one test that sets the variable; no other test in this crate reads it.
    unsafe {
        std::env::set_var(
            Store::MCP.env_var,
            std::ffi::OsString::from_vec(vec![0xff, 0xfe]),
        )
    };
    let err = env_token(Store::MCP).unwrap_err().to_string();
    unsafe { std::env::remove_var(Store::MCP.env_var) };
    assert!(err.contains(Store::MCP.env_var), "{err}");
    assert!(matches!(env_token(Store::MCP), Ok(None)));
}

#[test]
fn a_filesystem_without_hard_links_falls_back_to_a_direct_0600_create() {
    let home = temp_home("nolink");
    let path = generated_path_in(Store::MCP, &home);
    create_generated_with(&path, |_, _| {
        Err(std::io::Error::from_raw_os_error(libc::EPERM))
    })
    .unwrap();
    assert_eq!(mode_of(&path), 0o600);
    let t = resolve_with(Store::MCP, None, None, &home).unwrap();
    assert_eq!(t.value().len(), 43);
    // No temporary file is left beside it.
    let names: Vec<_> = std::fs::read_dir(path.parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(names.len(), 1, "{names:?}");
}

#[test]
fn creating_over_an_existing_generated_file_keeps_the_first_token() {
    let home = temp_home("exists");
    let first = resolve_with(Store::MCP, None, None, &home).unwrap();
    let path = generated_path_in(Store::MCP, &home);
    // The link finds the name taken, and so does the direct create after a failed link.
    create_generated(&path).unwrap();
    create_generated_with(&path, |_, _| {
        Err(std::io::Error::from_raw_os_error(libc::EPERM))
    })
    .unwrap();
    let again = resolve_with(Store::MCP, None, None, &home).unwrap();
    assert!(first.value() == again.value());
}

#[test]
fn the_created_state_directory_is_0700() {
    let home = temp_home("dirmode");
    resolve_with(Store::MCP, None, None, &home).unwrap();
    assert_eq!(mode_of(&home.join("svolder")), 0o700);
}

#[test]
fn a_named_pipe_or_directory_is_refused_without_blocking() {
    let home = temp_home("fifo");
    let fifo = home.join("pipe");
    let made = std::process::Command::new("mkfifo")
        .args(["-m", "600"])
        .arg(&fifo)
        .status()
        .expect("run mkfifo");
    assert!(made.success());
    let err = resolve_with(Store::MCP, None, Some(&fifo), &home).unwrap_err();
    assert!(err.0.contains("a named pipe, not a regular file"), "{err}");
    let err = resolve_with(Store::MCP, None, Some(&home), &home).unwrap_err();
    assert!(err.0.contains("a directory, not a regular file"), "{err}");
}

#[test]
fn a_file_over_the_read_cap_is_refused() {
    let home = temp_home("big");
    let file = home.join("big");
    write_secret(&file, &"a".repeat(5000), 0o600);
    let err = resolve_with(Store::MCP, None, Some(&file), &home).unwrap_err();
    assert!(err.0.contains("larger than 4096 bytes"), "{err}");
}

#[test]
fn a_token_the_header_cap_could_not_carry_is_refused_at_load() {
    let home = temp_home("long");
    let file = home.join("long");
    let longest = "a".repeat(MAX_TOKEN_LEN);
    write_secret(&file, &longest, 0o600);
    assert!(resolve_with(Store::MCP, None, Some(&file), &home).is_ok());
    write_secret(&file, &format!("{longest}a"), 0o600);
    let err = resolve_with(Store::MCP, None, Some(&file), &home).unwrap_err();
    assert!(err.0.contains("longer than 505 characters"), "{err}");
    let env =
        resolve_with(Store::MCP, Some("b".repeat(MAX_TOKEN_LEN + 1)), None, &home).unwrap_err();
    assert!(env.0.contains("longer than 505 characters"), "{env}");
}

#[test]
fn peeking_never_creates_the_generated_file() {
    let home = temp_home("peek");
    assert!(peek_with(Store::MCP, None, None, &home).unwrap().is_none());
    assert!(!generated_path_in(Store::MCP, &home).exists());
    let made = resolve_with(Store::MCP, None, None, &home).unwrap();
    let peeked = peek_with(Store::MCP, None, None, &home)
        .unwrap()
        .expect("the token exists");
    assert!(peeked.value() == made.value());
    assert!(matches!(peeked.source, TokenSource::Generated(_)));
}

#[test]
fn peeking_reads_the_environment_and_a_named_file_as_resolving_does() {
    let home = temp_home("peek-outside");
    let env = peek_with(Store::MCP, Some("fake-env-token".into()), None, &home)
        .unwrap()
        .expect("an environment token");
    assert!(env.value() == "fake-env-token" && env.source == TokenSource::Env("SVOLDER_MCP_TOKEN"));
    let file = home.join("secret");
    write_secret(&file, "fake-file-token\n", 0o600);
    let named = peek_with(Store::MCP, None, Some(&file), &home)
        .unwrap()
        .expect("a file token");
    assert!(named.value() == "fake-file-token");
    assert!(!generated_path_in(Store::MCP, &home).exists());
    write_secret(&file, "fake-file-token\n", 0o644);
    assert!(peek_with(Store::MCP, None, Some(&file), &home).is_err());
}

#[test]
fn a_token_is_compared_with_a_daemons_by_source_path_and_tail() {
    use crate::socket::protocol::{TokenInfo, TokenKind};
    let state = |path: &str, tail: &str| TokenInfo {
        kind: TokenKind::State,
        path: Some(path.into()),
        tail: tail.into(),
    };
    let ours = TokenSource::Generated("/home/user/state/svolder/mcp-token".into());
    let same = state("/home/user/state/svolder/mcp-token", "3fQ");
    assert_eq!(compare(&ours, "3fQ", &same), Agreement::Same);
    assert_eq!(compare(&ours, "zzz", &same), Agreement::NotReadYet);
    let elsewhere = state("/var/lib/svolder/mcp-token", "3fQ");
    assert_eq!(compare(&ours, "3fQ", &elsewhere), Agreement::Other);
    let env = TokenInfo {
        kind: TokenKind::Environment,
        path: None,
        tail: "3fQ".into(),
    };
    assert_eq!(
        compare(&TokenSource::Env("SVOLDER_MCP_TOKEN"), "3fQ", &env),
        Agreement::Same
    );
    assert_eq!(
        compare(&TokenSource::Env("SVOLDER_MCP_TOKEN"), "zzz", &env),
        Agreement::Other
    );
    assert_eq!(compare(&ours, "3fQ", &env), Agreement::Other);
}

/// The two stores keep their tokens apart: each has its own variable and its own generated
/// file, and one store's variable never stands in for the other's.
#[test]
fn the_web_store_has_its_own_variable_and_file() {
    let home = temp_home("stores");
    let web = resolve_with(Store::WEB, None, None, &home).expect("a web token");
    let mcp = resolve_with(Store::MCP, None, None, &home).expect("an MCP token");
    assert!(web.value() != mcp.value(), "the two stores share a token");
    assert_eq!(
        web.source,
        TokenSource::Generated(home.join("svolder").join("web-token"))
    );
    assert_eq!(
        generated_path_in(Store::MCP, &home),
        home.join("svolder").join("mcp-token")
    );
    let from_env = resolve_with(
        Store::WEB,
        Some("from-the-env-0123456789".into()),
        None,
        &home,
    )
    .expect("the variable's token");
    assert_eq!(from_env.source, TokenSource::Env("SVOLDER_WEB_TOKEN"));
    assert_eq!(
        from_env.source.describe(),
        "the SVOLDER_WEB_TOKEN environment variable"
    );
    let refused = rotate_with(
        Store::WEB,
        Some("from-the-env-0123456789".into()),
        None,
        &home,
    )
    .unwrap_err();
    assert!(refused.0.contains("SVOLDER_WEB_TOKEN"), "{refused}");
    assert_eq!(base64url(&[0xfb, 0xff]), "-_8");
}
