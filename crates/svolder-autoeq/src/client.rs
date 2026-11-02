// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The HTTP side: fetching the index into a cache directory and downloading one correction,
//! within byte and time limits.

use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ureq::Agent;
use ureq::tls::{Certificate, RootCerts, TlsConfig, TlsProvider};

use crate::index::{Entry, parse_index};

/// Where the index is fetched from unless `SVOLDER_AUTOEQ_URL` names another base.
const DEFAULT_BASE: &str = "https://raw.githubusercontent.com/jaakkopasanen/AutoEq/master/results/";

/// The name the index is cached under.
const CACHE_FILE: &str = "INDEX.md";

/// How many redirects a request follows.
const MAX_REDIRECTS: u32 = 5;

const WEEK: Duration = Duration::from_secs(7 * 24 * 3600);

/// When [`Client::index`] fetches the index again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refresh {
    /// Only when there is no cached copy.
    Never,
    /// When the cached copy is older than this many weeks.
    AfterWeeks(u8),
    /// Now, whatever the cached copy's age.
    Now,
}

/// The byte and time limits of a transfer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// How long a connection may take to open.
    pub connect: Duration,
    /// How long the whole transfer of the index may take.
    pub index_total: Duration,
    /// How long the whole transfer of one file may take.
    pub file_total: Duration,
    /// The most bytes the index may be.
    pub index_bytes: u64,
    /// The most bytes one file may be.
    pub file_bytes: u64,
}

impl Default for Limits {
    /// Connect in 10 s; the index in 60 s and 8 MiB; a file in 20 s and 64 KiB.
    fn default() -> Self {
        Limits {
            connect: Duration::from_secs(10),
            index_total: Duration::from_secs(60),
            file_total: Duration::from_secs(20),
            index_bytes: 8 * 1024 * 1024,
            file_bytes: 64 * 1024,
        }
    }
}

/// The index as [`Client::index`] returns it.
#[derive(Debug, Clone)]
pub struct Index {
    /// Every entry, in AutoEq's order.
    pub entries: Vec<Entry>,
    /// When this copy was fetched (the cache file's modification time).
    pub fetched: SystemTime,
    /// What the caller should say about where the copy came from: set when a refresh failed
    /// and the cached copy was used instead.
    pub note: Option<String>,
}

/// Why the index or a file could not be had.
#[derive(Debug, thiserror::Error)]
pub enum AutoEqError {
    /// The server could not be reached, or the transfer failed or ran out of time.
    #[error("could not fetch {url}: {reason}")]
    Fetch {
        /// What was asked for.
        url: String,
        /// What went wrong.
        reason: String,
    },
    /// The server answered with an error status; 404 for a file the index names.
    #[error("{url} answered {status}")]
    Status {
        /// What was asked for.
        url: String,
        /// The HTTP status.
        status: u16,
    },
    /// The body is larger than the limit.
    #[error("{url} is larger than {limit} bytes")]
    TooLarge {
        /// What was asked for.
        url: String,
        /// The limit it passed.
        limit: u64,
    },
    /// The body is not text.
    #[error("{url} is not text")]
    NotText {
        /// What was asked for.
        url: String,
    },
    /// The text fetched holds no entry.
    #[error("the AutoEq index at {url} lists no headphone")]
    EmptyIndex {
        /// Where it was fetched.
        url: String,
    },
    /// No cached copy, and the fetch failed.
    #[error("there is no cached AutoEq index and fetching one failed ({reason})")]
    NoIndex {
        /// Why the fetch failed.
        reason: String,
    },
    /// The cache directory or file could not be written.
    #[error("could not write the AutoEq cache {}: {reason}", path.display())]
    Cache {
        /// The path.
        path: PathBuf,
        /// What went wrong.
        reason: String,
    },
}

/// A blocking AutoEq client: a base URL, a cache directory and limits.
#[derive(Debug, Clone)]
pub struct Client {
    base: String,
    cache_dir: PathBuf,
    limits: Limits,
}

impl Client {
    /// A client for the `results/` directory at `base`, caching the index in `cache_dir`.
    pub fn new(base: String, cache_dir: PathBuf, limits: Limits) -> Client {
        let mut base = base;
        if !base.ends_with('/') {
            base.push('/');
        }
        Client {
            base,
            cache_dir,
            limits,
        }
    }

    /// A client for `SVOLDER_AUTOEQ_URL` or AutoEq's own repository, with the default limits.
    pub fn from_env(cache_dir: PathBuf) -> Client {
        Client::from_var("SVOLDER_AUTOEQ_URL", cache_dir)
    }

    /// A client for the base URL in the environment variable `var` (unset or blank means AutoEq's
    /// own repository), with the default limits.
    pub fn from_var(var: &str, cache_dir: PathBuf) -> Client {
        let base = std::env::var(var)
            .ok()
            .filter(|url| !url.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_BASE.to_string());
        Client::new(base, cache_dir, Limits::default())
    }

    /// The base URL entries' files are under.
    pub fn base(&self) -> &str {
        &self.base
    }

    /// The index, from the cache or the network as `refresh` and the cached copy's age say.
    ///
    /// A fetched index replaces the cached one atomically, and only when it holds at least one
    /// entry. A failed fetch with a cached copy in hand returns that copy with a note saying so;
    /// a failed fetch with none is [`AutoEqError::NoIndex`].
    pub fn index(&self, refresh: Refresh) -> Result<Index, AutoEqError> {
        let cached = self.read_cache();
        let stale = match (&cached, refresh) {
            (None, _) | (_, Refresh::Now) => true,
            (Some(_), Refresh::Never) => false,
            (Some(copy), Refresh::AfterWeeks(weeks)) => SystemTime::now()
                .duration_since(copy.fetched)
                .is_ok_and(|age| age > WEEK * u32::from(weeks)),
        };
        if !stale {
            return Ok(cached.expect("a fresh copy is a cached one"));
        }
        match self.fetch_index() {
            Ok(index) => Ok(index),
            Err(error) => match cached {
                Some(mut copy) => {
                    copy.note = Some(format!(
                        "using the index from {} (refresh failed: {error})",
                        date(copy.fetched)
                    ));
                    Ok(copy)
                }
                None => Err(AutoEqError::NoIndex {
                    reason: error.to_string(),
                }),
            },
        }
    }

    /// The text of `entry`'s `ParametricEQ.txt`.
    pub fn download(&self, entry: &Entry) -> Result<String, AutoEqError> {
        let url = entry.peq_url(&self.base);
        self.get_text(&url, self.limits.file_total, self.limits.file_bytes)
    }

    /// The cached copy, when there is one with an entry in it.
    fn read_cache(&self) -> Option<Index> {
        let path = self.cache_dir.join(CACHE_FILE);
        let text = std::fs::read_to_string(&path).ok()?;
        let fetched = std::fs::metadata(&path).ok()?.modified().ok()?;
        let entries = parse_index(&text);
        (!entries.is_empty()).then_some(Index {
            entries,
            fetched,
            note: None,
        })
    }

    fn fetch_index(&self) -> Result<Index, AutoEqError> {
        let url = format!("{}{CACHE_FILE}", self.base);
        let text = self.get_text(&url, self.limits.index_total, self.limits.index_bytes)?;
        let entries = parse_index(&text);
        if entries.is_empty() {
            return Err(AutoEqError::EmptyIndex { url });
        }
        self.write_cache(&text)?;
        Ok(Index {
            entries,
            fetched: SystemTime::now(),
            note: None,
        })
    }

    fn get_text(&self, url: &str, total: Duration, cap: u64) -> Result<String, AutoEqError> {
        let fetch = |reason: String| AutoEqError::Fetch {
            url: url.to_string(),
            reason,
        };
        let mut response = self
            .agent(total)
            .get(url)
            .call()
            .map_err(|error| match error {
                ureq::Error::StatusCode(status) => AutoEqError::Status {
                    url: url.to_string(),
                    status,
                },
                other => fetch(other.to_string()),
            })?;
        let bytes = response
            .body_mut()
            .with_config()
            .limit(cap)
            .read_to_vec()
            .map_err(|error| match error {
                ureq::Error::BodyExceedsLimit(limit) => AutoEqError::TooLarge {
                    url: url.to_string(),
                    limit,
                },
                other => fetch(other.to_string()),
            })?;
        String::from_utf8(bytes).map_err(|_| AutoEqError::NotText {
            url: url.to_string(),
        })
    }

    fn agent(&self, total: Duration) -> Agent {
        // Only an HTTPS base needs the system's roots read.
        let roots = match self.base.starts_with("https://") {
            true => system_roots(),
            false => Arc::new(Vec::new()),
        };
        let tls = TlsConfig::builder()
            .provider(TlsProvider::Rustls)
            .root_certs(RootCerts::Specific(roots))
            .unversioned_rustls_crypto_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .build();
        let mut config = Agent::config_builder()
            .tls_config(tls)
            .max_redirects(MAX_REDIRECTS)
            .user_agent(concat!("svolder/", env!("CARGO_PKG_VERSION")))
            .timeout_connect(Some(self.limits.connect))
            .timeout_global(Some(total));
        if self.base.starts_with("https://") {
            // A redirect from an https base to plain http is refused.
            config = config.https_only(true);
        }
        if is_loopback(&self.base) {
            config = config.proxy(None);
        }
        config.build().new_agent()
    }

    /// Put `text` in the cache through a temporary file beside it, renamed into place.
    fn write_cache(&self, text: &str) -> Result<(), AutoEqError> {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let path = self.cache_dir.join(CACHE_FILE);
        let fail = |error: std::io::Error| AutoEqError::Cache {
            path: path.clone(),
            reason: error.to_string(),
        };
        std::fs::create_dir_all(&self.cache_dir).map_err(fail)?;
        let tmp = self.cache_dir.join(format!(
            "{CACHE_FILE}.{}.{}.tmp",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let written = std::fs::File::create_new(&tmp).and_then(|mut file| {
            file.write_all(text.as_bytes())?;
            file.sync_all()
        });
        match written.and_then(|()| std::fs::rename(&tmp, &path)) {
            Ok(()) => Ok(()),
            Err(error) => {
                let _ = std::fs::remove_file(&tmp);
                Err(fail(error))
            }
        }
    }
}

/// The operating system's trusted root certificates, read once. A store that cannot be read
/// leaves the list empty, and every HTTPS request then fails verification.
fn system_roots() -> Arc<Vec<Certificate<'static>>> {
    static ROOTS: OnceLock<Arc<Vec<Certificate<'static>>>> = OnceLock::new();
    ROOTS
        .get_or_init(|| {
            let loaded = rustls_native_certs::load_native_certs();
            let certs = loaded
                .certs
                .iter()
                .map(|der| Certificate::from_der(der.as_ref()).to_owned())
                .collect();
            Arc::new(certs)
        })
        .clone()
}

/// Whether `url` names this machine, which a proxy is never asked to reach.
fn is_loopback(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("http://") else {
        return false;
    };
    let host = rest.split(['/', ':']).next().unwrap_or_default();
    host == "localhost" || host.starts_with("127.") || rest.starts_with("[::1]")
}

/// `time` as `YYYY-MM-DD` in UTC.
fn date(time: SystemTime) -> String {
    let days = time
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs() / 86400) as i64;
    // Days since 1970-01-01 as a proleptic Gregorian date.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

#[cfg(test)]
mod tests;
