// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Finding the device a backend opens: the scan windows a selector is searched over, the
//! device it picks from what they heard, and the profile that device is read through.

use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use svolder_ble::{Advertisement, Advertisements, BleError, Transport};
use svolder_ipc::ScanEntry;
use svolder_profiles::{Profile, Registry};
use svolder_proto::UnitId;

use crate::backend::Failure;
use crate::recovery::Kind;
use crate::scanning;

/// Scan for `window` and keep the first advertisement seen for each unit id.
pub(crate) async fn collect_unique(
    transport: &dyn Transport,
    window: Duration,
) -> Result<Vec<Advertisement>, BleError> {
    Ok(unique(transport.scan(window).await?).await)
}

/// One advertisement per unit id, in the order they arrive.
async fn unique(mut stream: Advertisements) -> Vec<Advertisement> {
    let mut found: Vec<Advertisement> = Vec::new();
    while let Some(a) = stream.next().await {
        if !found.iter().any(|x| x.unit_id() == a.unit_id()) {
            found.push(a);
        }
    }
    found
}

/// Scan and pick the device matching `selector` (unit-id prefix or product name), falling back
/// to the devices the adapter holds when the window names none.
///
/// With a selector, the search runs windows until one hears it; with none, one window is
/// enough, since anything at all answers it.
///
/// A selector that is a full unit id names exactly one device, so each window ends at that
/// device's first advertisement instead of being waited out, and the transport keeps what it
/// heard for the connect that follows. A prefix or a product name waits out every window: it
/// must see every device it could name to know it names only one.
///
/// With no selector and several devices in range, the failure names the candidates, so whoever
/// reports it shows them and the user can name one with `-d`. Nothing is printed here: this
/// runs inside a backend, which a front end may be opening with its own screen up.
///
/// A product name is read as `registry` names the product, so a device the codec's table does
/// not know is selected by the name of the profile made for it.
///
/// The pick says whether a window heard its advertisement ([`Selected::heard`]) or the adapter
/// held it from before, whose signal strength is of unknown age.
pub async fn select_device(
    transport: &Arc<dyn Transport>,
    registry: &Registry,
    selector: Option<&str>,
    window: Duration,
) -> Result<Selected, Failure> {
    let source = TransportScans {
        transport,
        registry,
        window,
        seen: std::sync::Mutex::new(Vec::new()),
        held: std::sync::Mutex::new(Vec::new()),
    };
    scanning::find_with(&source, selector).await?;
    let heard = source.seen.lock().expect("the sightings").clone();
    let advert = pick(heard, registry, selector)?;
    let held = source.held.lock().expect("the sightings");
    let heard = !held.contains(&advert.unit_id());
    Ok(Selected { advert, heard })
}

/// The device [`select_device`] picked.
#[derive(Debug, Clone)]
pub struct Selected {
    /// Its advertisement, which the connect that follows takes.
    pub advert: Advertisement,
    /// Whether a window heard that advertisement, as against the adapter holding it from before.
    pub heard: bool,
}

/// The windows [`select_device`] searches over: a listen on the transport it was handed,
/// answered from the devices the adapter holds when the window itself heard nothing that
/// matched.
///
/// The advertisements themselves are kept as the windows go by, because connecting to the
/// device that is picked takes the advertisement rather than the row a scan prints.
struct TransportScans<'t> {
    /// The session this process already has open.
    transport: &'t Arc<dyn Transport>,
    /// What names each product heard.
    registry: &'t Registry,
    /// How long each window listens.
    window: Duration,
    /// What the last window heard, in the order it heard it.
    seen: std::sync::Mutex<Vec<Advertisement>>,
    /// The devices of `seen` that came from the adapter's held list, not from the window.
    held: std::sync::Mutex<Vec<UnitId>>,
}

#[async_trait::async_trait(?Send)]
impl scanning::ScanSource for TransportScans<'_> {
    /// Which adapter this listens on is settled: the transport is open on it already, so a
    /// window here is asked for rather than aimed.
    async fn window(
        &self,
        _adapter: Option<&str>,
        want: Option<&str>,
    ) -> Result<Vec<ScanEntry>, Failure> {
        let listened = match want.and_then(full_unit_id) {
            Some(unit) => self
                .transport
                .sight(unit, self.window)
                .await
                .map(|advert| advert.into_iter().collect()),
            None => collect_unique(self.transport.as_ref(), self.window).await,
        };
        let mut heard = listened.map_err(|e| Failure::from_ble(&e))?;
        let mut from_held = Vec::new();
        if !heard
            .iter()
            .any(|advert| answers(advert, self.registry, want))
        {
            // The window was quiet for what the caller is after. A device the adapter holds
            // carries the address and the identity that picking one needs, and the connection
            // that follows is what settles whether that device is still there, so it joins
            // what the window heard on the same terms.
            let held = unique(
                self.transport
                    .held()
                    .await
                    .map_err(|e| Failure::from_ble(&e))?,
            )
            .await;
            for advert in held {
                if !heard.iter().any(|a| a.unit_id() == advert.unit_id()) {
                    from_held.push(advert.unit_id());
                    heard.push(advert);
                }
            }
        }
        *self.held.lock().expect("the sightings") = from_held;
        let entries = heard
            .iter()
            .map(|advert| scanning::entry(advert, self.registry))
            .collect();
        *self.seen.lock().expect("the sightings") = heard;
        Ok(entries)
    }
}

/// The unit id `want` is, when it is a whole one — sixteen hex digits — rather than a prefix of
/// one or a product name.
fn full_unit_id(want: &str) -> Option<UnitId> {
    want.parse().ok()
}

/// Whether this advertisement is one a search for `want` is after; a search for nothing in
/// particular is after any device at all.
fn answers(advert: &Advertisement, registry: &Registry, want: Option<&str>) -> bool {
    match want {
        Some(want) => scanning::names_device(
            &advert.unit_id().to_string(),
            scanning::product_of(advert, registry),
            want,
        ),
        None => true,
    }
}

/// Pick the device `selector` names from the ones `found`, on the terms
/// [`select_device`] describes.
fn pick(
    mut found: Vec<Advertisement>,
    registry: &Registry,
    selector: Option<&str>,
) -> Result<Advertisement, Failure> {
    match selector {
        None if found.len() == 1 => Ok(found.remove(0)),
        None => Err(ambiguous(&found, registry)),
        Some(sel) => {
            let mut matches: Vec<Advertisement> = found
                .into_iter()
                .filter(|a| answers(a, registry, Some(sel)))
                .collect();
            match matches.len() {
                1 => Ok(matches.remove(0)),
                _ => Err(nothing_in_range()),
            }
        }
    }
}

/// What a selector that names nothing in particular earns: the devices in range, so the user
/// can pick one, or the plain not-found when there are none to pick from.
///
/// The candidates are named on the one line the message is: a front end shows a failure on a
/// single key line, and a line break there would run through the frame around it.
pub(super) fn ambiguous(found: &[Advertisement], registry: &Registry) -> Failure {
    if found.is_empty() {
        return nothing_in_range();
    }
    let candidates: Vec<String> = found
        .iter()
        .map(|a| {
            let product = scanning::product_of(a, registry).unwrap_or("unknown");
            format!("{}  {product}", a.unit_id())
        })
        .collect();
    Failure::of(
        Kind::AmbiguousDevice,
        format!(
            "{} devices in range: {}",
            found.len(),
            candidates.join("; ")
        ),
    )
}

/// What a scan that heard nothing earns, in the words a session reports it with.
fn nothing_in_range() -> Failure {
    Failure::of(Kind::NothingInRange, BleError::NotFound.to_string())
}

/// Choose the profile for an advertisement: registry select by product id + firmware,
/// else a generic profile.
pub fn profile_for(reg: &Registry, advert: &Advertisement) -> Profile {
    reg.select(advert.manufacturer.product_id, advert.firmware_build())
        .cloned()
        .unwrap_or_else(|| Registry::generic(advert.manufacturer.product_id))
}
