// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Advertise the way a Forkbeard device does on the air: the flags and the 128-bit service
//! UUID in the 31-byte advertisement, the 27-byte manufacturer payload in the 31-byte scan
//! response, so a scanner that only listens for legacy advertising PDUs — a phone running the
//! official app — sees the device and, on its active scan, the payload too.
//!
//! bluer's `Advertisement` has no scan-response fields, so this registers its own
//! `org.bluez.LEAdvertisement1` object over a second system-bus connection and hands it to the
//! adapter's `LEAdvertisingManager1`. bluetoothd reads the `ScanResponse*` properties only when
//! started with `--experimental`; without it the payload is left out and a Forkbeard scanner
//! does not recognise the device.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use dbus::Path;
use dbus::arg::Variant;
use dbus::channel::MatchingReceiver;
use dbus::message::MatchRule;
use dbus::nonblock::{Proxy, SyncConnection};
use dbus_crossroads::{Crossroads, IfaceBuilder};
use std::time::Duration;

/// The advertisement's object path on this process's connection.
const ADVERTISEMENT_PATH: &str = "/org/svolder/sim/advertisement0";
/// bluetoothd's bus name and the interfaces this talks to.
const BLUEZ: &str = "org.bluez";
const ADVERTISEMENT_INTERFACE: &str = "org.bluez.LEAdvertisement1";
const MANAGER_INTERFACE: &str = "org.bluez.LEAdvertisingManager1";
/// How long a D-Bus call to bluetoothd may take.
const CALL_TIMEOUT: Duration = Duration::from_secs(10);
/// What one legacy advertising PDU or scan response carries.
const LEGACY_PDU_BYTES: usize = 31;

/// The legacy-layout advertisement and the bus connection that serves it. The advertisement is
/// registered with [`LegacyAdvertiser::register`] and taken off the air with
/// [`LegacyAdvertiser::unregister`], as often as needed on the one connection; it is taken off
/// best effort when this is dropped while registered.
pub(super) struct LegacyAdvertiser {
    connection: Arc<SyncConnection>,
    adapter_path: Path<'static>,
    registered: bool,
}

/// What the advertisement object tells bluetoothd.
struct Advertisement {
    service_uuid: String,
    manufacturer: (u16, Vec<u8>),
}

/// The sizes, in bytes, of the advertisement and the scan response this layout produces, or
/// why one of them does not fit a legacy PDU. Flags take 3 bytes, a 128-bit UUID list 2 + 16
/// per UUID, a manufacturer-data element 2 + 2 + the payload.
fn legacy_sizes(uuid_count: usize, payload_len: usize) -> Result<(usize, usize), String> {
    let advertisement = 3 + 2 + 16 * uuid_count;
    let scan_response = 2 + 2 + payload_len;
    if advertisement > LEGACY_PDU_BYTES {
        return Err(format!(
            "the advertisement would be {advertisement} bytes; a legacy PDU carries {LEGACY_PDU_BYTES}"
        ));
    }
    if scan_response > LEGACY_PDU_BYTES {
        return Err(format!(
            "the scan response would be {scan_response} bytes; a legacy PDU carries {LEGACY_PDU_BYTES}"
        ));
    }
    Ok((advertisement, scan_response))
}

/// Serve a legacy-layout advertisement for `adapter` (e.g. `hci1`) on a bus connection of its
/// own. Nothing is on the air until [`LegacyAdvertiser::register`].
pub async fn advertiser(
    adapter: &str,
    service_uuid: bluer::Uuid,
    company_id: u16,
    payload: Vec<u8>,
) -> Result<LegacyAdvertiser, Box<dyn std::error::Error>> {
    legacy_sizes(1, payload.len())?;

    let (resource, connection) =
        tokio::task::spawn_blocking(dbus_tokio::connection::new_system_sync).await??;
    tokio::spawn(async move {
        let err = resource.await;
        eprintln!("svolder-sim: the advertisement's D-Bus connection ended: {err}");
    });

    let mut crossroads = Crossroads::new();
    crossroads.set_async_support(Some((
        connection.clone(),
        Box::new(|x| {
            tokio::spawn(x);
        }),
    )));
    let token = crossroads.register(
        ADVERTISEMENT_INTERFACE,
        |ib: &mut IfaceBuilder<Advertisement>| {
            ib.property("Type").get(|_, _| Ok("peripheral".to_string()));
            ib.property("ServiceUUIDs")
                .get(|_, a| Ok(vec![a.service_uuid.clone()]));
            ib.property("Discoverable").get(|_, _| Ok(true));
            ib.property("ScanResponseManufacturerData").get(|_, a| {
                let mut data = HashMap::new();
                data.insert(a.manufacturer.0, Variant(a.manufacturer.1.clone()));
                Ok(data)
            });
            ib.method("Release", (), (), |_, _, _: ()| Ok(()));
        },
    );
    let path = Path::new(ADVERTISEMENT_PATH)?;
    crossroads.insert(
        path.clone(),
        &[token],
        Advertisement {
            service_uuid: service_uuid.to_string(),
            manufacturer: (company_id, payload),
        },
    );

    // Method calls bluetoothd makes on the object (property reads, Release) are answered by
    // this receiver for as long as the connection lives; every handler is synchronous, so the
    // lock is never held across an await.
    let crossroads = Arc::new(Mutex::new(crossroads));
    connection.start_receive(
        MatchRule::new_method_call(),
        Box::new(move |message, connection| {
            let mut crossroads = crossroads.lock().expect("crossroads mutex");
            let _ = crossroads.handle_message(message, connection);
            true
        }),
    );

    let adapter_path = Path::new(format!("/org/bluez/{adapter}"))?.into_static();
    Ok(LegacyAdvertiser {
        connection,
        adapter_path,
        registered: false,
    })
}

impl LegacyAdvertiser {
    /// Ask bluetoothd to put the advertisement on the air.
    pub async fn register(&mut self) -> Result<(), dbus::Error> {
        let path = Path::new(ADVERTISEMENT_PATH).expect("a valid object path");
        let options: HashMap<String, Variant<Box<dyn dbus::arg::RefArg>>> = HashMap::new();
        let () = self
            .manager()
            .method_call(MANAGER_INTERFACE, "RegisterAdvertisement", (path, options))
            .await?;
        self.registered = true;
        Ok(())
    }

    /// Ask bluetoothd to take the advertisement off the air and wait for its answer, so a
    /// shutdown leaves nothing advertising. It can be registered again afterwards.
    pub async fn unregister(&mut self) {
        self.registered = false;
        if let Ok(path) = Path::new(ADVERTISEMENT_PATH) {
            let _: Result<(), dbus::Error> = self
                .manager()
                .method_call(MANAGER_INTERFACE, "UnregisterAdvertisement", (path,))
                .await;
        }
    }

    fn manager(&self) -> Proxy<'static, Arc<SyncConnection>> {
        Proxy::new(
            BLUEZ,
            self.adapter_path.clone(),
            CALL_TIMEOUT,
            self.connection.clone(),
        )
    }
}

/// Dropping an advertiser that is still registered asks bluetoothd to stop the advertisement,
/// best effort: the request is spawned on the runtime the drop happens on and is lost if that
/// runtime is shutting down.
impl Drop for LegacyAdvertiser {
    fn drop(&mut self) {
        if !self.registered {
            return;
        }
        let manager = self.manager();
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                if let Ok(path) = Path::new(ADVERTISEMENT_PATH) {
                    let _: Result<(), dbus::Error> = manager
                        .method_call(MANAGER_INTERFACE, "UnregisterAdvertisement", (path,))
                        .await;
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_forkbeard_layout_fits_two_legacy_pdus() {
        assert_eq!(legacy_sizes(1, 27), Ok((21, 31)));
    }

    #[test]
    fn a_longer_payload_or_a_second_uuid_does_not_fit() {
        assert!(legacy_sizes(1, 28).unwrap_err().contains("scan response"));
        assert!(legacy_sizes(2, 27).unwrap_err().contains("advertisement"));
    }
}
