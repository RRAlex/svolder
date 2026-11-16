// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The download of one catalogue set: the listed URL only, over HTTPS, streamed to a temporary
//! file under the store's directory, checked against the entry's SHA-256, converted and stored;
//! the download is deleted whatever happens.
//!
//! The daemon's speakers base address (its environment variable is named where the daemon reads
//! it) replaces the scheme and host of the listed URLs (the path stays), so a
//! test serves the files from a loopback server. The address nothing listens on
//! (`127.0.0.1:9`) turns fetching off, as the fake daemon is given it.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use ureq::Agent;
use ureq::tls::{Certificate, RootCerts, TlsConfig, TlsProvider};

use crate::catalogue::{Container, Entry, find};
use crate::convert::{MAX_SOURCE_BYTES, convert};
use crate::error::ConvertError;
use crate::import::{Identity, Saved, store_converted};
use crate::sha::Sha;
use crate::store::{Store, StoreError};

/// The words of fetching being off; the same start whatever the transport.
pub const OFF_WORDS: &str = "fetching sets is off: the sets' base address is the discard port";

/// Where a fake transport's client points: nothing listens there.
const FAKE_BASE: &str = "http://127.0.0.1:9/";

const MAX_REDIRECTS: u32 = 5;

/// The time and byte limits of one fetch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// How long a connection may take to open.
    pub connect: Duration,
    /// How long the whole download may take.
    pub total: Duration,
    /// The most bytes a download may be (a larger entry is refused unasked).
    pub download_bytes: u64,
    /// The most bytes a zip member may inflate to.
    pub member_bytes: u64,
}

impl Default for Limits {
    /// Connect in 10 s, the download in 120 s, 64 MiB a download and a member.
    fn default() -> Self {
        Limits {
            connect: Duration::from_secs(10),
            total: Duration::from_secs(120),
            download_bytes: MAX_SOURCE_BYTES as u64,
            member_bytes: MAX_SOURCE_BYTES as u64,
        }
    }
}

/// Why a fetch failed.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum FetchError {
    /// Fetching is off.
    #[error("{OFF_WORDS}")]
    Off,
    /// The id is not in the catalogue, or the source answered 404.
    #[error("{0} is not listed at its source any more")]
    NotListed(String),
    /// The source could not be reached or the transfer failed. The words never name the file's
    /// address, whose name may carry a product's.
    #[error("could not fetch the set's source: {reason}")]
    Unreachable {
        /// What was asked for.
        url: String,
        /// What went wrong.
        reason: String,
    },
    /// The download is not the file the catalogue pins.
    #[error("the source changed; svolder needs an update ({0})")]
    Checksum(String),
    /// The download or a member is larger than allowed.
    #[error("the download is larger than allowed ({0} bytes)")]
    TooLarge(u64),
    /// The fetch was cancelled.
    #[error("the fetch was cancelled")]
    Cancelled,
    /// The zip archive does not hold the set.
    #[error("the archive cannot be read: {0}")]
    Archive(String),
    /// The download could not be converted.
    #[error("{0}")]
    Convert(#[from] ConvertError),
    /// The set could not be stored.
    #[error("{0}")]
    Store(#[from] StoreError),
}

impl FetchError {
    /// The wire word: `off`, `not_listed`, `unreachable`, `checksum`, `too_large`,
    /// `cancelled`, `archive`, `unreadable` (the converter refused it) or `store`.
    pub fn kind(&self) -> &'static str {
        match self {
            FetchError::Off => "off",
            FetchError::NotListed(_) => "not_listed",
            FetchError::Unreachable { .. } => "unreachable",
            FetchError::Checksum(_) => "checksum",
            FetchError::TooLarge(_) => "too_large",
            FetchError::Cancelled => "cancelled",
            FetchError::Archive(_) => "archive",
            FetchError::Convert(_) => "unreadable",
            FetchError::Store(_) => "store",
        }
    }
}

/// A flag a running fetch watches; setting it stops the download.
#[derive(Debug, Clone, Default)]
pub struct Cancel(Arc<AtomicBool>);

impl Cancel {
    /// A flag that is not set.
    pub fn new() -> Cancel {
        Cancel::default()
    }

    /// Stop the fetch that holds this flag.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    /// Whether it was set.
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// A blocking fetch client.
#[derive(Debug, Clone)]
pub struct Client {
    /// The base replacing the listed URLs' scheme and host, or `None` for the listed URLs.
    base: Option<String>,
    limits: Limits,
}

impl Client {
    /// A client for the listed URLs, or for `base` (an `http` or `https` origin) when given.
    pub fn new(base: Option<String>, limits: Limits) -> Client {
        let base = base
            .map(|base| base.trim().trim_end_matches('/').to_string())
            .filter(|base| !base.is_empty());
        Client { base, limits }
    }

    /// A client for a daemon whose transport is `transport`, told `configured` (the value of
    /// the daemon's speakers base address): the listed URLs, or `configured`, or the address nothing listens
    /// on under the fake transport.
    pub fn for_transport(transport: &str, configured: Option<String>) -> Client {
        let base = configured
            .filter(|url| !url.trim().is_empty())
            .or_else(|| (transport.trim() == "fake").then(|| FAKE_BASE.to_string()));
        Client::new(base, Limits::default())
    }

    /// Whether fetching is off: the base is the address nothing listens on.
    pub fn is_off(&self) -> bool {
        let Some(base) = &self.base else { return false };
        let rest = base
            .split_once("://")
            .map_or(base.as_str(), |(_, rest)| rest);
        matches!(
            rest.split('/').next().unwrap_or_default(),
            "127.0.0.1:9" | "localhost:9" | "[::1]:9"
        )
    }

    /// The URL `entry` is fetched from.
    pub fn url_for(&self, entry: &Entry) -> String {
        match &self.base {
            None => entry.url.to_string(),
            Some(base) => {
                let after = entry
                    .url
                    .split_once("://")
                    .map_or(entry.url, |(_, rest)| rest);
                let path = after.find('/').map_or("/", |at| &after[at..]);
                format!("{base}{path}")
            }
        }
    }

    /// Fetch the catalogue set `id` into `store`: download, check, convert, store. `progress`
    /// is told the bytes received and the bytes expected after every chunk; `cancel` stops it,
    /// keeping nothing. `chosen` is the set in use, which the store's limit never evicts.
    pub fn fetch(
        &self,
        store: &Store,
        id: &str,
        now: u64,
        chosen: Option<&str>,
        cancel: &Cancel,
        progress: &mut dyn FnMut(u64, u64),
    ) -> Result<Saved, FetchError> {
        let entry = find(id).ok_or_else(|| FetchError::NotListed(id.to_string()))?;
        self.fetch_entry(store, entry, now, chosen, cancel, progress)
    }

    pub(crate) fn fetch_entry(
        &self,
        store: &Store,
        entry: &Entry,
        now: u64,
        chosen: Option<&str>,
        cancel: &Cancel,
        progress: &mut dyn FnMut(u64, u64),
    ) -> Result<Saved, FetchError> {
        if self.is_off() {
            return Err(FetchError::Off);
        }
        if entry.bytes > self.limits.download_bytes {
            return Err(FetchError::TooLarge(entry.bytes));
        }
        let part = Part::new(store, entry.id)?;
        self.download(entry, &part.0, cancel, progress)?;
        let bytes = match entry.container {
            Container::Sofa => {
                std::fs::read(&part.0).map_err(|e| archive(format!("reading the download: {e}")))?
            }
            Container::Zip { member } => self.read_member(&part.0, member)?,
        };
        let converted = convert(&bytes, &entry.recipe())?;
        let identity = Identity {
            id: entry.id,
            room: entry.room,
            label: entry.label,
            licence: entry.licence,
            credit: entry.credit,
            source_url: entry.source_page,
            source_sha256: entry.sha256,
            own: false,
        };
        Ok(store_converted(store, &converted, &identity, now, chosen)?)
    }

    /// Stream the entry's URL to `path`, checking its size and SHA-256.
    fn download(
        &self,
        entry: &Entry,
        path: &Path,
        cancel: &Cancel,
        progress: &mut dyn FnMut(u64, u64),
    ) -> Result<(), FetchError> {
        use std::io::Write;
        let url = self.url_for(entry);
        let unreachable = |reason: String| FetchError::Unreachable {
            url: url.clone(),
            reason,
        };
        let agent = self.agent();
        let mut target = url.clone();
        let mut hops = 0;
        let mut response = loop {
            let response = agent.get(&target).call().map_err(|error| match error {
                ureq::Error::StatusCode(404) => FetchError::NotListed(entry.id.to_string()),
                ureq::Error::StatusCode(status) => unreachable(format!("answered {status}")),
                other => unreachable(other.to_string().replace(&target, "the address")),
            })?;
            if !matches!(response.status().as_u16(), 301 | 302 | 303 | 307 | 308) {
                break response;
            }
            hops += 1;
            if hops > MAX_REDIRECTS {
                return Err(unreachable("too many redirects".into()));
            }
            let location = response
                .headers()
                .get("location")
                .and_then(|value| value.to_str().ok())
                .ok_or_else(|| unreachable("a redirect with no location".into()))?;
            target = match location.starts_with('/') {
                true => format!("{}{location}", origin(&target)),
                false => location.to_string(),
            };
            if !self.host_allowed(entry, &target) {
                return Err(unreachable(format!(
                    "redirected to a host the catalogue does not list ({})",
                    authority(&target)
                )));
            }
        };
        // One byte more than the pinned size: a longer body is seen as longer, not as cut.
        let mut body = response
            .body_mut()
            .with_config()
            .limit(entry.bytes.saturating_add(1))
            .reader();
        let mut file = std::fs::File::create(path)
            .map_err(|e| archive(format!("creating the download file: {e}")))?;
        let (mut sha, mut received) = (Sha::new(), 0u64);
        let mut chunk = vec![0u8; 64 * 1024];
        loop {
            if cancel.is_cancelled() {
                return Err(FetchError::Cancelled);
            }
            let read = match body.read(&mut chunk) {
                Ok(0) => break,
                Ok(read) => read,
                Err(error) => return Err(unreachable(error.to_string())),
            };
            received += read as u64;
            if received > entry.bytes {
                return Err(FetchError::TooLarge(received));
            }
            sha.update(&chunk[..read]);
            file.write_all(&chunk[..read])
                .map_err(|e| archive(format!("writing the download: {e}")))?;
            progress(received, entry.bytes);
        }
        if cancel.is_cancelled() {
            return Err(FetchError::Cancelled);
        }
        let got = sha.finish();
        if received != entry.bytes || got != entry.sha256 {
            return Err(FetchError::Checksum(format!(
                "{received} bytes, sha256 {got}"
            )));
        }
        Ok(())
    }

    /// Whether `url` is on a host a redirect may go to: the host the entry is fetched from, or
    /// any host the catalogue lists.
    pub(crate) fn host_allowed(&self, entry: &Entry, url: &str) -> bool {
        let host = authority(url).to_ascii_lowercase();
        let listed = crate::entries()
            .iter()
            .any(|e| authority(e.url).eq_ignore_ascii_case(&host));
        listed || authority(&self.url_for(entry)).eq_ignore_ascii_case(&host)
    }

    /// The member `name` of the zip at `path`, never more than the member limit.
    fn read_member(&self, path: &Path, name: &str) -> Result<Vec<u8>, FetchError> {
        let file =
            std::fs::File::open(path).map_err(|e| archive(format!("opening the download: {e}")))?;
        let mut zip = zip::ZipArchive::new(file).map_err(|e| archive(e.to_string()))?;
        let member = zip
            .by_name(name)
            .map_err(|e| archive(format!("{name}: {e}")))?;
        let limit = self.limits.member_bytes;
        if member.size() > limit {
            return Err(FetchError::TooLarge(member.size()));
        }
        // The declared size is not trusted: reading stops one byte past the limit.
        let mut bytes = Vec::new();
        member
            .take(limit + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| archive(e.to_string()))?;
        if bytes.len() as u64 > limit {
            return Err(FetchError::TooLarge(bytes.len() as u64));
        }
        Ok(bytes)
    }

    fn agent(&self) -> Agent {
        let https = self
            .base
            .as_ref()
            .is_none_or(|base| base.starts_with("https://"));
        let roots = match https {
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
            // Redirects are followed by hand, each hop checked against the listed hosts.
            .max_redirects(0)
            .max_redirects_will_error(false)
            .user_agent(concat!("svolder/", env!("CARGO_PKG_VERSION")))
            .timeout_connect(Some(self.limits.connect))
            .timeout_global(Some(self.limits.total));
        if https {
            config = config.https_only(true);
        }
        if self.base.as_deref().is_some_and(is_loopback) {
            config = config.proxy(None);
        }
        config.build().new_agent()
    }
}

/// The `host[:port]` of `url`, without a user name.
fn authority(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    host.rsplit_once('@').map_or(host, |(_, host)| host)
}

/// `scheme://host[:port]` of `url`.
fn origin(url: &str) -> String {
    let scheme = url.split_once("://").map_or("https", |(scheme, _)| scheme);
    format!("{scheme}://{}", authority(url))
}

fn archive(reason: String) -> FetchError {
    FetchError::Archive(reason)
}

/// The temporary download file; removed when it goes out of scope, whatever happened.
struct Part(PathBuf);

impl Part {
    fn new(store: &Store, id: &str) -> Result<Part, FetchError> {
        std::fs::create_dir_all(store.dir())
            .map_err(|e| archive(format!("creating the speakers directory: {e}")))?;
        Ok(Part(store.dir().join(format!(".{id}.part"))))
    }
}

impl Drop for Part {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn system_roots() -> Arc<Vec<Certificate<'static>>> {
    static ROOTS: OnceLock<Arc<Vec<Certificate<'static>>>> = OnceLock::new();
    ROOTS
        .get_or_init(|| {
            let loaded = rustls_native_certs::load_native_certs();
            Arc::new(
                loaded
                    .certs
                    .iter()
                    .map(|der| Certificate::from_der(der.as_ref()).to_owned())
                    .collect(),
            )
        })
        .clone()
}

fn is_loopback(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("http://") else {
        return false;
    };
    let host = rest.split(['/', ':']).next().unwrap_or_default();
    host == "localhost" || host.starts_with("127.") || rest.starts_with("[::1]")
}

#[cfg(test)]
mod tests;
