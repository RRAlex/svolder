// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The bearer tokens svolderd's HTTP servers ask for, the MCP server's and the web UI's, and
//! where each comes from.
//!
//! Each [`Store`] has three sources, in order: its environment variable (when not empty), the
//! config's `token_file`, and a file svolder generates in its state directory,
//! `$XDG_STATE_HOME/svolder/<file name>`. A file read from disk, generated or named, must not be
//! readable by group or others. Nothing here ever puts a token in an error or a `Debug` print.

use std::io::{Read, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

/// Where one server's token lives: the environment variable that outranks every file, and the
/// name of the file svolder generates in its state directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Store {
    /// The environment variable that carries the token.
    pub env_var: &'static str,
    /// The generated file's name under `$XDG_STATE_HOME/svolder/`.
    pub file_name: &'static str,
}

impl Store {
    /// The MCP server's token: `SVOLDER_MCP_TOKEN`, `mcp-token`.
    pub const MCP: Store = Store {
        env_var: "SVOLDER_MCP_TOKEN",
        file_name: "mcp-token",
    };

    /// The web UI's token: `SVOLDER_WEB_TOKEN`, `web-token`.
    pub const WEB: Store = Store {
        env_var: "SVOLDER_WEB_TOKEN",
        file_name: "web-token",
    };
}

/// The longest token svolder accepts, in characters. A request's `Authorization` header is
/// `Bearer ` and the token, and the server refuses a header over 512 bytes before comparing, so
/// a longer token could never authenticate; refusing it at load says so.
pub const MAX_TOKEN_LEN: usize = 505;

/// The most of a token file that is read: a file of any other kind of size is no token.
const MAX_FILE_BYTES: u64 = 4096;

/// Random bytes in a generated token; they encode to 43 base64url characters.
const TOKEN_BYTES: usize = 32;

/// Why a token could not be had. The text names a source or a path, never a token.
#[derive(Debug, PartialEq, Eq)]
pub struct TokenError(pub String);

impl std::fmt::Display for TokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for TokenError {}

/// Where a token came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenSource {
    /// The environment variable named here.
    Env(&'static str),
    /// The file the config's `token_file` names.
    File(PathBuf),
    /// The file svolder generated in its state directory.
    Generated(PathBuf),
}

impl TokenSource {
    /// The source in words, for a message: the variable's name or the file's path.
    pub fn describe(&self) -> String {
        match self {
            TokenSource::Env(var) => format!("the {var} environment variable"),
            TokenSource::File(path) => format!("the token file {}", path.display()),
            TokenSource::Generated(path) => format!("the generated token file {}", path.display()),
        }
    }
}

/// A token and where it came from. It prints as its source, never as itself.
#[derive(Clone)]
pub struct Token {
    value: String,
    /// Where the token was read from.
    pub source: TokenSource,
}

impl Token {
    /// The token itself, for the one caller that prints it and the one that compares it.
    pub fn value(&self) -> &str {
        &self.value
    }
}

impl std::fmt::Debug for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Token")
            .field("value", &"***")
            .field("source", &self.source)
            .finish()
    }
}

/// The last three characters of `token`, or none for a token under 16 characters: what a mask
/// may show of it, and what a daemon tells a front end so the two can tell they read the same
/// token. A short token shows nothing.
pub fn tail(token: &str) -> String {
    let count = token.chars().count();
    match count >= 16 {
        true => token.chars().skip(count - 3).collect(),
        false => String::new(),
    }
}

impl Token {
    /// A token of `value` from `source`, for a case that needs one without a store.
    #[cfg(test)]
    pub(crate) fn for_test(value: &str, source: TokenSource) -> Token {
        Token {
            value: value.to_string(),
            source,
        }
    }

    /// What a daemon may say of this token: its source and its tail, never its value.
    pub fn info(&self) -> crate::socket::protocol::TokenInfo {
        use crate::socket::protocol::{TokenInfo, TokenKind};
        let (kind, path) = match &self.source {
            TokenSource::Env(_) => (TokenKind::Environment, None),
            TokenSource::File(path) => (TokenKind::File, Some(path.display().to_string())),
            TokenSource::Generated(path) => (TokenKind::State, Some(path.display().to_string())),
        };
        TokenInfo {
            kind,
            path,
            tail: tail(&self.value),
        }
    }
}

/// How a token read here stands against the one a daemon describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Agreement {
    /// The same source and the same tail: the daemon uses this token.
    Same,
    /// The same file but another tail: the file changed since the daemon read it, and a reload
    /// settles it.
    NotReadYet,
    /// Another source, or an environment's other value: the daemon uses another token.
    Other,
}

/// How a token read from `source` whose tail is `tail` stands against the one a daemon's
/// `theirs` describes: the same kind of source, the same file (as written or once resolved) and
/// the same tail. The one comparison the TUI and the CLI make against a daemon's answer.
pub fn compare(
    source: &TokenSource,
    tail: &str,
    theirs: &crate::socket::protocol::TokenInfo,
) -> Agreement {
    use crate::socket::protocol::TokenKind;
    let path_is = |path: &Path| {
        theirs
            .path
            .as_deref()
            .is_some_and(|theirs| same_file(Path::new(theirs), path))
    };
    let same_source = match (source, theirs.kind) {
        (TokenSource::Env(_), TokenKind::Environment) => true,
        (TokenSource::File(path), TokenKind::File)
        | (TokenSource::Generated(path), TokenKind::State) => path_is(path),
        _ => false,
    };
    match (same_source, tail == theirs.tail) {
        (true, true) => Agreement::Same,
        (true, false) if !matches!(source, TokenSource::Env(_)) => Agreement::NotReadYet,
        _ => Agreement::Other,
    }
}

/// Where the token a daemon describes lives, in words: its environment variable or its file.
pub fn lives(store: Store, theirs: &crate::socket::protocol::TokenInfo) -> String {
    match (theirs.kind, theirs.path.as_deref()) {
        (crate::socket::protocol::TokenKind::Environment, _) => {
            format!("in the daemon's environment ({})", store.env_var)
        }
        (_, Some(path)) => format!("at {path}"),
        (_, None) => "elsewhere".to_string(),
    }
}

/// Whether two paths name the same file: equal as written, or equal once resolved.
pub fn same_file(a: &Path, b: &Path) -> bool {
    a == b
        || matches!(
            (std::fs::canonicalize(a), std::fs::canonicalize(b)),
            (Ok(a), Ok(b)) if a == b
        )
}

/// The generated token file, `$XDG_STATE_HOME/svolder/<file name>`.
pub fn generated_path(store: Store) -> PathBuf {
    generated_path_in(store, &crate::config::state_home())
}

/// [`generated_path`] under the state home `base`.
pub fn generated_path_in(store: Store, base: &Path) -> PathBuf {
    base.join("svolder").join(store.file_name)
}

/// The token to use now: the environment's, else `token_file`'s, else the generated file's,
/// created with a fresh token when it does not exist yet.
pub fn resolve(store: Store, token_file: Option<&Path>) -> Result<Token, TokenError> {
    resolve_with(
        store,
        env_token(store)?,
        token_file,
        &crate::config::state_home(),
    )
}

/// Replace the generated token with a fresh one. Refuses when the token comes from the
/// environment or a `token_file`, naming that source: svolder does not own it.
pub fn rotate(store: Store, token_file: Option<&Path>) -> Result<Token, TokenError> {
    rotate_with(
        store,
        env_token(store)?,
        token_file,
        &crate::config::state_home(),
    )
}

/// The token as [`resolve`] would find it, without creating one: `None` when only the generated
/// file could hold it and it does not exist yet. What a front end shows of the token before the
/// daemon has made one, so looking never makes one.
pub fn peek(store: Store, token_file: Option<&Path>) -> Result<Option<Token>, TokenError> {
    peek_with(
        store,
        env_token(store)?,
        token_file,
        &crate::config::state_home(),
    )
}

/// Whether `given` is `expected`, in time that depends on the lengths and not on where the two
/// first differ.
pub fn token_matches(expected: &str, given: &str) -> bool {
    let (a, b) = (expected.as_bytes(), given.as_bytes());
    let mut diff = a.len() ^ b.len();
    for i in 0..a.len().max(b.len()) {
        let x = a.get(i).copied().unwrap_or(0);
        let y = b.get(i).copied().unwrap_or(0);
        diff |= usize::from(x ^ y);
    }
    std::hint::black_box(diff) == 0
}

/// The store's variable as set; a value that is not UTF-8 is an error naming the variable, not
/// an unset one, so a daemon never falls back to a generated token the operator did not mean.
pub fn env_token(store: Store) -> Result<Option<String>, TokenError> {
    match std::env::var(store.env_var) {
        Ok(value) => Ok(Some(value)),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => {
            Err(TokenError(format!("{} is not valid UTF-8", store.env_var)))
        }
    }
}

/// Refuse a token a request header could never carry: one with whitespace inside it, which
/// includes a second line. `source` names where it came from.
fn check_header_safe(value: String, source: &str) -> Result<String, TokenError> {
    if value.len() > MAX_TOKEN_LEN {
        return Err(TokenError(format!(
            "the token in {source} is longer than {MAX_TOKEN_LEN} characters, which a request \
             header cannot carry"
        )));
    }
    if value.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(TokenError(format!(
            "the token in {source} has whitespace or more than one line; a token must be one \
             run of visible characters"
        )));
    }
    Ok(value)
}

/// [`resolve`] with the environment's value and the state home as inputs.
pub fn resolve_with(
    store: Store,
    env: Option<String>,
    token_file: Option<&Path>,
    state_home: &Path,
) -> Result<Token, TokenError> {
    if let Some(value) = env.map(|v| v.trim().to_string()).filter(|v| !v.is_empty()) {
        return Ok(Token {
            value: check_header_safe(value, &TokenSource::Env(store.env_var).describe())?,
            source: TokenSource::Env(store.env_var),
        });
    }
    if let Some(path) = token_file {
        return Ok(Token {
            value: read_secret(path)?,
            source: TokenSource::File(path.to_path_buf()),
        });
    }
    let path = generated_path_in(store, state_home);
    let value = match read_secret(&path) {
        Ok(value) => value,
        Err(_) if !path.exists() => {
            create_generated(&path)?;
            read_created(&path)?
        }
        Err(e) => return Err(e),
    };
    Ok(Token {
        value,
        source: TokenSource::Generated(path),
    })
}

/// [`peek`] with the environment's value and the state home as inputs.
pub fn peek_with(
    store: Store,
    env: Option<String>,
    token_file: Option<&Path>,
    state_home: &Path,
) -> Result<Option<Token>, TokenError> {
    if env.as_deref().is_some_and(|v| !v.trim().is_empty()) || token_file.is_some() {
        return resolve_with(store, env, token_file, state_home).map(Some);
    }
    let path = generated_path_in(store, state_home);
    if !path.exists() {
        return Ok(None);
    }
    resolve_with(store, env, token_file, state_home).map(Some)
}

/// [`rotate`] with the environment's value and the state home as inputs.
pub fn rotate_with(
    store: Store,
    env: Option<String>,
    token_file: Option<&Path>,
    state_home: &Path,
) -> Result<Token, TokenError> {
    if env.is_some_and(|v| !v.trim().is_empty()) {
        return Err(refuse_rotate(&TokenSource::Env(store.env_var)));
    }
    if let Some(path) = token_file {
        return Err(refuse_rotate(&TokenSource::File(path.to_path_buf())));
    }
    let path = generated_path_in(store, state_home);
    let value = generate()?;
    write_replacing(&path, &value)?;
    Ok(Token {
        value,
        source: TokenSource::Generated(path),
    })
}

fn refuse_rotate(source: &TokenSource) -> TokenError {
    TokenError(format!(
        "the token comes from {}, which svolder does not own: rotate it there",
        source.describe()
    ))
}

/// Read a token file: refuse one group or others can read, and one with no token in it.
fn read_secret(path: &Path) -> Result<String, TokenError> {
    let io = |e: std::io::Error| TokenError(format!("token file {}: {e}", path.display()));
    // Opened without blocking, so a named pipe or a hung mount cannot hold the caller: a pipe
    // with no writer opens at once and is then refused as not a regular file.
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)
        .map_err(io)?;
    let meta = file.metadata().map_err(io)?;
    if !meta.is_file() {
        return Err(TokenError(format!(
            "token file {} is {}, not a regular file",
            path.display(),
            kind_of(&meta)
        )));
    }
    if meta.len() > MAX_FILE_BYTES {
        return Err(TokenError(format!(
            "token file {} is larger than {MAX_FILE_BYTES} bytes",
            path.display()
        )));
    }
    let mode = meta.permissions().mode() & 0o777;
    if mode & 0o077 != 0 {
        return Err(TokenError(format!(
            "token file {} has mode {mode:04o}; it must not be readable by group or others \
             (chmod 600 it)",
            path.display()
        )));
    }
    let mut text = String::new();
    (&mut file)
        .take(MAX_FILE_BYTES)
        .read_to_string(&mut text)
        .map_err(io)?;
    let value = text.trim().to_string();
    if value.is_empty() {
        return Err(TokenError(format!(
            "token file {} is empty",
            path.display()
        )));
    }
    check_header_safe(value, &format!("token file {}", path.display()))
}

/// What a file that is not a regular one is, in words.
fn kind_of(meta: &std::fs::Metadata) -> &'static str {
    use std::os::unix::fs::FileTypeExt;
    let kind = meta.file_type();
    if kind.is_fifo() {
        "a named pipe"
    } else if kind.is_dir() {
        "a directory"
    } else if kind.is_socket() {
        "a socket"
    } else if kind.is_char_device() || kind.is_block_device() {
        "a device"
    } else {
        "something other than a file"
    }
}

/// Read a file another process may be writing: an empty one is read again for a moment, as
/// the creator of a file on a filesystem without hard links writes it after creating it.
fn read_created(path: &Path) -> Result<String, TokenError> {
    for _ in 0..20 {
        let empty = std::fs::metadata(path).is_ok_and(|m| m.len() == 0);
        if !empty {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    read_secret(path)
}

/// Create the generated token file with a fresh token, unless another process got there first.
fn create_generated(path: &Path) -> Result<(), TokenError> {
    create_generated_with(path, |from, to| std::fs::hard_link(from, to))
}

/// [`create_generated`] with the link step as an input.
///
/// The token is written to a temporary file and linked to the final name, so the name never
/// holds a half-written token. A link fails when the name exists, so a concurrent creator's
/// file is never replaced. Where the filesystem cannot link at all, the final name is created
/// directly, exclusively and with mode 0600, and written; a reader may then see it empty for a
/// moment ([`read_created`] waits it out).
fn create_generated_with(
    path: &Path,
    link: impl FnOnce(&Path, &Path) -> std::io::Result<()>,
) -> Result<(), TokenError> {
    let value = generate()?;
    let tmp = write_temp(path, &value)?;
    let linked = link(&tmp, path);
    let _ = std::fs::remove_file(&tmp);
    match linked {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(_) => {
            let io = |e: std::io::Error| TokenError(format!("token file {}: {e}", path.display()));
            let mut file = match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(path)
            {
                Ok(file) => file,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => return Ok(()),
                Err(e) => return Err(io(e)),
            };
            file.write_all(value.as_bytes()).map_err(io)
        }
    }
}

/// Atomically replace the file at `path` with `value`, mode 0600.
fn write_replacing(path: &Path, value: &str) -> Result<(), TokenError> {
    let tmp = write_temp(path, value)?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        TokenError(format!("token file {}: {e}", path.display()))
    })
}

/// Write `value` to a new 0600 file beside `path`, making its directory (0700) when needed.
fn write_temp(path: &Path, value: &str) -> Result<PathBuf, TokenError> {
    let io = |e: std::io::Error| TokenError(format!("token file {}: {e}", path.display()));
    let dir = path.parent().unwrap_or(Path::new("."));
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
        .map_err(io)?;
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let tmp = dir.join(format!(".{}.{}.tmp", name, std::process::id()));
    let _ = std::fs::remove_file(&tmp);
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&tmp)
        .map_err(io)?;
    let written = file
        .write_all(value.as_bytes())
        .and_then(|()| file.sync_all());
    if let Err(e) = written {
        let _ = std::fs::remove_file(&tmp);
        return Err(io(e));
    }
    Ok(tmp)
}

/// A fresh token: 32 bytes from the operating system's random source, base64url encoded.
fn generate() -> Result<String, TokenError> {
    let mut bytes = [0u8; TOKEN_BYTES];
    random_bytes(&mut bytes)
        .map_err(|e| TokenError(format!("cannot read random bytes from /dev/urandom: {e}")))?;
    Ok(base64url(&bytes))
}

/// Fill `buf` from the operating system's random source (`/dev/urandom`).
pub fn random_bytes(buf: &mut [u8]) -> std::io::Result<()> {
    std::fs::File::open("/dev/urandom")?.read_exact(buf)
}

/// `bytes` in the URL-safe base64 alphabet, without padding: the generated tokens, and the web
/// UI's cookie MAC and link codes.
pub fn base64url(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |acc, (i, b)| acc | u32::from(*b) << (16 - 8 * i));
        for i in 0..=chunk.len() {
            out.push(char::from(ALPHABET[(n >> (18 - 6 * i) & 63) as usize]));
        }
    }
    out
}

#[cfg(test)]
mod tests;
