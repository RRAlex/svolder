// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Finding a device on the air, and how well each adapter hears it.
//!
//! A scan is one listening window, and an adapter that misses half of them makes a single
//! window a poor answer to "is this device there?". So a search for a particular device runs
//! several windows and stops at the first that hears it, and an adapter can be graded by how
//! many of a run of windows it caught the device in.
//!
//! Both run over [`ScanSource`], which hands back one window's worth of sightings: the
//! production sources listen through a backend or through one named adapter, and a test hands
//! over a script of windows instead, so nothing here needs a radio to be exercised.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use svolder_ble::{Advertisement, Transport};
use svolder_ipc::ScanEntry;
use svolder_profiles::Registry;
use tokio::sync::Mutex;

use crate::backend::{Backend, Failure, Globals, Wire, open_transport};
use crate::commands::device::{collect_unique, opened};
use crate::recovery::Kind;

/// How long one listening window lasts.
pub const WINDOW: Duration = Duration::from_secs(5);

/// How many windows a search for a named device runs before it gives up.
pub const WINDOWS: u8 = 3;

/// How many windows each adapter is graded over.
pub const GRADE_WINDOWS: u8 = 5;

/// How long one grading window lasts.
pub const GRADE_WINDOW: Duration = Duration::from_secs(1);

/// Where the windows a search runs come from.
#[async_trait(?Send)]
pub trait ScanSource {
    /// Listen for one window and answer with one entry per device heard.
    async fn window(
        &self,
        adapter: Option<&str>,
        want: Option<&str>,
    ) -> Result<Vec<ScanEntry>, Failure>;
}

/// How many of the windows it was graded over one adapter heard the device in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grade {
    /// The adapter, as BlueZ names it.
    pub adapter: String,
    /// How many windows it heard the device in.
    pub seen: u8,
    /// How many windows it was given.
    pub of: u8,
    /// Why it heard nothing, for an adapter that would not open at all.
    pub note: Option<String>,
}

/// One advertisement as a scan row, with the signal strength and the product id the
/// advertisement itself carries, and the product named as `registry` names it.
pub fn entry(advert: &Advertisement, registry: &Registry) -> ScanEntry {
    ScanEntry {
        unit: advert.unit_id().to_string(),
        product_id: advert.manufacturer.product_id,
        product: product_of(advert, registry).map(str::to_string),
        firmware: advert.firmware_build(),
        passphrase: advert.manufacturer.passphrase().is_well_formed(),
        rssi: advert.rssi,
    }
}

/// What the device behind an advertisement is called: [`Registry::product_name`] for the
/// product id it carries, so a product the codec does not know is named by its profile.
pub fn product_of<'r>(advert: &Advertisement, registry: &'r Registry) -> Option<&'r str> {
    registry.product_name(advert.manufacturer.product_id)
}

/// Whether `want` names this device: the start of its unit id, or its product name.
pub fn names(entry: &ScanEntry, want: &str) -> bool {
    names_device(&entry.unit, entry.product.as_deref(), want)
}

/// Whether `want` names the device with this unit id and product name — the one rule every
/// device selector is read by, so what a search stops at is what a caller then picks.
pub fn names_device(unit: &str, product: Option<&str>, want: &str) -> bool {
    unit.to_ascii_lowercase()
        .starts_with(&want.to_ascii_lowercase())
        || product.is_some_and(|name| name.eq_ignore_ascii_case(want))
}

/// Every device one window hears, or the one `want` names within up to [`WINDOWS`] of them.
///
/// The windows come from whichever backend the verb would run on: the daemon's discovery when
/// one is running, a direct session on the adapter `--adapter` or the configuration names
/// otherwise.
pub async fn find(g: &Globals<'_>, want: Option<&str>) -> Result<Vec<ScanEntry>, Failure> {
    find_with(&BackendScans::new(g), want).await
}

/// [`find`] over any source of windows.
///
/// A search for a particular device runs windows until one hears it, because an adapter that
/// misses half the windows a device advertises in makes a single quiet window no answer at
/// all. A search for whatever is out there is the one window it always was: nothing it could
/// hear would make it stop early.
pub async fn find_with(
    source: &dyn ScanSource,
    want: Option<&str>,
) -> Result<Vec<ScanEntry>, Failure> {
    let Some(want) = want else {
        return source.window(None, None).await;
    };
    for _ in 0..WINDOWS {
        let heard = source.window(None, Some(want)).await?;
        if heard.iter().any(|entry| names(entry, want)) {
            return Ok(heard);
        }
    }
    Err(Failure::of(
        Kind::NothingInRange,
        format!("no device {want} seen in {WINDOWS} windows"),
    ))
}

/// What a search on a front end's behalf heard, and what opening the backend had to say.
#[derive(Debug, Default, PartialEq)]
pub struct Scanned {
    /// One entry per device heard.
    pub entries: Vec<ScanEntry>,
    /// What opening kept for whoever opened it — a daemon this process tried to start and
    /// could not — for a caller that shows its own words rather than printing them.
    pub notices: Vec<String>,
}

/// Every device in range, over up to [`WINDOWS`] windows, stopping at the first that hears
/// anything, with what opening had to say kept rather than printed.
///
/// This is what the setup wizard listens with. A verb that prints what is out there takes one
/// window and shows whatever it held, since whoever typed it is watching and can type it again;
/// a guided run asks a question on the user's behalf and answers it as well as a search for a
/// named device does, since an adapter that misses half the windows a device advertises in
/// makes a single quiet one no answer at all.
pub async fn find_any(g: &Globals<'_>) -> Result<Scanned, Failure> {
    let source = BackendScans::quiet(g);
    let heard = find_any_with(&source).await;
    scanned(heard, source.kept().await)
}

/// What a search heard and what opening had to say, as one answer.
///
/// A search that failed keeps the notices too, folded into the failure's own words: what
/// opening said is usually the half of the story that explains the other, and dropping it with
/// the source would leave the reader the failure alone.
fn scanned(
    heard: Result<Vec<ScanEntry>, Failure>,
    notices: Vec<String>,
) -> Result<Scanned, Failure> {
    match heard {
        Ok(entries) => Ok(Scanned { entries, notices }),
        Err(failure) if notices.is_empty() => Err(failure),
        Err(mut failure) => {
            failure.what = format!("{}; {}", notices.join("; "), failure.what);
            Err(failure)
        }
    }
}

/// [`find_any`] over any source of windows.
pub async fn find_any_with(source: &dyn ScanSource) -> Result<Vec<ScanEntry>, Failure> {
    let mut heard = Vec::new();
    for _ in 0..WINDOWS {
        heard = source.window(None, None).await?;
        if !heard.is_empty() {
            break;
        }
    }
    Ok(heard)
}

/// Grade each adapter in turn by how many of [`GRADE_WINDOWS`] windows it hears `udid` in.
///
/// The adapters are graded one after another, each on a session of this process's own, so
/// that only one of them is listening at a time and the count is what that adapter caught
/// rather than what it caught while sharing the air with its neighbours.
pub async fn grade_adapters(g: &Globals<'_>, udid: &str, adapters: &[String]) -> Vec<Grade> {
    // A graded adapter is a real one, except under `--transport fake`, where the fake device
    // answers every window whichever adapter is named.
    let wire = match g.transport {
        "fake" => Wire::Fake,
        _ => Wire::Ble,
    };
    grade_with(&AdapterScans::on(wire), udid, adapters).await
}

/// [`grade_adapters`] over any source of windows, in the order the adapters are given.
///
/// An adapter that will not open at all hears nothing, keeping the reason it gave: a grade of
/// none out of five is what a caller compares adapters by, and the note tells a user this one
/// was not merely deaf. A window that fails once the adapter is open is a window lost, not a
/// verdict on the adapter, so it counts as a miss and grading goes on, the reason kept in the
/// same note.
pub async fn grade_with(source: &dyn ScanSource, udid: &str, adapters: &[String]) -> Vec<Grade> {
    let mut grades = Vec::with_capacity(adapters.len());
    for adapter in adapters {
        let mut seen = 0;
        let mut note = None;
        for window in 0..GRADE_WINDOWS {
            match source.window(Some(adapter), Some(udid)).await {
                Ok(heard) if heard.iter().any(|entry| names(entry, udid)) => seen += 1,
                Ok(_) => {}
                Err(failure) => {
                    note.get_or_insert(failure.what);
                    // The first window is the one that opens the adapter, so a failure there
                    // is the adapter itself and there is nothing left to grade.
                    if window == 0 {
                        break;
                    }
                }
            }
        }
        grades.push(Grade {
            adapter: adapter.clone(),
            seen,
            of: GRADE_WINDOWS,
            note,
        });
    }
    grades
}

/// The windows [`find`] runs: a scan through whichever backend the verb itself would open.
///
/// The backend is opened once, on the first window a search runs, and every window after that
/// listens over the same one: a search for a named device can run up to [`WINDOWS`] of them, and
/// opening again for each would walk the launcher chain (and could autospawn a daemon) that many
/// times over for what is really one search. What opening had to say is said before the first
/// window runs by a source [`BackendScans::new`] made, and kept for the caller to show by one
/// [`BackendScans::quiet`] made.
struct BackendScans<'a, 'g> {
    /// What the verb was given, which is what says where a scan goes and over which adapter.
    g: &'a Globals<'g>,
    /// The backend this search's windows run over, opened on the first one asked for.
    backend: Mutex<Option<Backend>>,
    /// Whether opening says what it kept where a verb's own messages go, rather than keeping
    /// it here for the caller.
    prints: bool,
    /// What opening kept, for a caller that was not printed to.
    kept: Mutex<Vec<String>>,
}

impl<'a, 'g> BackendScans<'a, 'g> {
    /// A source that opens `g`'s backend the first time a window is asked for, saying what
    /// opening had to say where a verb's own messages go.
    fn new(g: &'a Globals<'g>) -> BackendScans<'a, 'g> {
        BackendScans::over(g, true)
    }

    /// The same source for a caller that draws its own screen: what opening had to say is kept
    /// for [`BackendScans::kept`] rather than printed.
    fn quiet(g: &'a Globals<'g>) -> BackendScans<'a, 'g> {
        BackendScans::over(g, false)
    }

    /// A source over `g`, printing what opening kept or keeping it.
    fn over(g: &'a Globals<'g>, prints: bool) -> BackendScans<'a, 'g> {
        BackendScans {
            g,
            backend: Mutex::new(None),
            prints,
            kept: Mutex::new(Vec::new()),
        }
    }

    /// What opening had to say, once, for a source that kept it.
    async fn kept(&self) -> Vec<String> {
        std::mem::take(&mut *self.kept.lock().await)
    }
}

#[async_trait(?Send)]
impl ScanSource for BackendScans<'_, '_> {
    /// Which adapter to listen on is the backend's own choice here — a running daemon has its
    /// own, and a direct session takes `--adapter` or the configured one — so a window of this
    /// source is asked for rather than aimed.
    async fn window(
        &self,
        _adapter: Option<&str>,
        _want: Option<&str>,
    ) -> Result<Vec<ScanEntry>, Failure> {
        let mut slot = self.backend.lock().await;
        if slot.is_none() {
            *slot = Some(match self.prints {
                true => opened(self.g, false).await?,
                false => {
                    let mut backend = Backend::open(self.g, false).await?;
                    self.kept.lock().await.extend(backend.take_warnings());
                    backend
                }
            });
        }
        slot.as_mut().expect("opened just above").scan(WINDOW).await
    }
}

/// The windows [`grade_adapters`] runs: one short listen on the adapter it is given, through a
/// session this process opens itself.
///
/// One adapter is open at a time. The transport is kept between the windows of the adapter
/// being graded, so an adapter is opened once rather than five times, and it is dropped the
/// moment the next adapter's first window asks for one.
struct AdapterScans {
    /// Which transport a window opens.
    wire: Wire,
    /// The adapter being graded and the transport open on it.
    open: Mutex<Option<(String, Arc<dyn Transport>)>>,
}

impl AdapterScans {
    /// Grading sessions over `wire`, with nothing open yet.
    fn on(wire: Wire) -> AdapterScans {
        AdapterScans {
            wire,
            open: Mutex::new(None),
        }
    }
}

#[async_trait(?Send)]
impl ScanSource for AdapterScans {
    /// What `adapter` hears in [`GRADE_WINDOW`]; the device a caller is after is filtered for
    /// by the grading, so every device heard is reported.
    async fn window(
        &self,
        adapter: Option<&str>,
        _want: Option<&str>,
    ) -> Result<Vec<ScanEntry>, Failure> {
        let mut open = self.open.lock().await;
        let transport = match open.as_ref() {
            Some((name, transport)) if Some(name.as_str()) == adapter => Arc::clone(transport),
            _ => {
                // Whatever was open is let go before the next adapter opens, so two adapters
                // are never listening at once.
                *open = None;
                let transport = open_transport(self.wire, adapter).await?;
                *open = Some((
                    adapter.unwrap_or_default().to_string(),
                    Arc::clone(&transport),
                ));
                transport
            }
        };
        let heard = collect_unique(transport.as_ref(), GRADE_WINDOW)
            .await
            .map_err(|e| Failure::from_ble(&e))?;
        // A grade counts sightings by unit id, so the rows carry no product name of a profile's:
        // the codec's own table names what it knows.
        let registry = Registry::default();
        Ok(heard
            .iter()
            .map(|advert| entry(advert, &registry))
            .collect())
    }
}

#[cfg(test)]
mod tests;
