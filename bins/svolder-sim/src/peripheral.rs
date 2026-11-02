// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The BlueZ side of the peripheral: the Forkbeard GATT service, the advertisement in either
//! layout, and the run loop that answers writes until a stop signal arrives. `main` builds the
//! fake device from the arguments and hands it here.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bluer::adv::Advertisement;
use bluer::gatt::local::{
    Application, Characteristic, CharacteristicNotifier, CharacteristicNotify,
    CharacteristicNotifyMethod, CharacteristicWrite, CharacteristicWriteMethod, Service,
};
use futures::FutureExt;
use svolder_fake::{FakeDevice, SIGNAL_PERIOD, Signal};
use svolder_proto::ADVERT_COMPANY_ID;

use crate::{Args, Layout, legacy_adv};

/// The Forkbeard GATT service this peripheral serves.
const SERVICE_UUID: bluer::Uuid = bluer::Uuid::from_u128(svolder_proto::FORKBEARD_SERVICE_UUID);
/// The characteristic a central writes command frames to.
const TX_UUID: bluer::Uuid = bluer::Uuid::from_u128(svolder_proto::FORKBEARD_TX_UUID);
/// The characteristic this peripheral notifies status frames on.
const RX_UUID: bluer::Uuid = bluer::Uuid::from_u128(svolder_proto::FORKBEARD_RX_UUID);
/// How long one link is given to come down while the peripheral is shutting down, so a
/// central that will not let go cannot hold the exit up.
const DISCONNECT_TIMEOUT: Duration = Duration::from_secs(3);
/// How often the extended layout's advertisement goes out. One that names no interval goes out
/// every 1.28 s, the kernel's default, which leaves a 5-second scan window four advertisements
/// to hear, and a central can miss all four; at this rate a window holds about fifty.
const ADVERTISING_INTERVAL: Duration = Duration::from_millis(100);
/// How often the number of links the adapter holds is looked at, to notice one closing.
const LINK_POLL: Duration = Duration::from_millis(250);

/// Serve `device` on the adapter `args` names, advertising `manufacturer_bytes`, until a stop
/// signal arrives.
pub(super) async fn run(
    args: &Args,
    device: Arc<Mutex<FakeDevice>>,
    manufacturer_bytes: Vec<u8>,
) -> Result<(), Box<dyn std::error::Error>> {
    let session = bluer::Session::new().await?;
    let _agent_handle = register_agent(&session, args.accept_pairing).await?;
    let adapter = session.adapter(&args.adapter)?;
    adapter.set_powered(true).await?;
    eprintln!(
        "svolder-sim: advertising on {} ({})",
        adapter.name(),
        adapter.address().await?
    );

    let mut air = Air::default();
    air.put_on(&adapter, args, manufacturer_bytes.clone())
        .await?;

    // The notifier is stashed by the RX notify callback and used by the TX write callback.
    let notifier: Arc<tokio::sync::Mutex<Option<CharacteristicNotifier>>> =
        Arc::new(tokio::sync::Mutex::new(None));
    let app = gatt_application(device.clone(), notifier.clone(), args.trace);
    let _app_handle = adapter.serve_gatt_application(app).await?;

    let signal_task = start_signal(args.signal, device, notifier);

    let stops = Stops::register()?;
    eprintln!("svolder-sim: ready; Ctrl-C to stop");
    let stop = stops.wait();
    tokio::pin!(stop);
    let mut links = LinkWatch::default();
    let mut retry = Retry::default();
    let mut ticks = tokio::time::interval(LINK_POLL);
    loop {
        tokio::select! {
            _ = &mut stop => break,
            _ = ticks.tick() => {}
        }
        // A unit advertises again once its link is closed, and a central that scans for it
        // finds it only from a fresh advertisement. A look that fails says nothing of the links.
        let closed = match connected_links(&adapter).await {
            Some(held) => links.closed(held),
            None => false,
        };
        if closed {
            eprintln!("svolder-sim: a link closed; advertising again");
            air.take_off().await;
        }
        // A registration that failed is tried again, further apart each time.
        if !air.on && retry.due() {
            match air.put_on(&adapter, args, manufacturer_bytes.clone()).await {
                Ok(()) => retry = Retry::default(),
                Err(e) => eprintln!(
                    "svolder-sim: advertising again failed: {e}; next try in {:?}",
                    retry.failed()
                ),
            }
        }
    }

    if let Some(task) = signal_task {
        task.abort();
    }
    // A device being switched off drops the links it holds. One bluetoothd serves both
    // sides of the e2e rig, so giving up this process's GATT application and advertisement
    // is not by itself the end of a central's link: take the links down here, while the
    // application is still registered, so the central sees the device go.
    drop_links(&adapter).await;
    air.take_off().await;
    Ok(())
}

/// Register the pairing agent when `accept_pairing` is set.
///
/// A registered agent answers bluetoothd's pairing questions for as long as its handle lives;
/// every answer here is yes, which is what a Just Works pairing amounts to.
async fn register_agent(
    session: &bluer::Session,
    accept_pairing: bool,
) -> Result<Option<bluer::agent::AgentHandle>, Box<dyn std::error::Error>> {
    Ok(if accept_pairing {
        let agent = bluer::agent::Agent {
            request_default: true,
            request_confirmation: Some(Box::new(|_| async { Ok(()) }.boxed())),
            request_authorization: Some(Box::new(|_| async { Ok(()) }.boxed())),
            authorize_service: Some(Box::new(|_| async { Ok(()) }.boxed())),
            ..Default::default()
        };
        let handle = session.register_agent(agent).await?;
        eprintln!("svolder-sim: accepting pairing requests (Just Works)");
        Some(handle)
    } else {
        None
    })
}

/// The advertisement and what keeps it on the air: the extended layout's handle, or the legacy
/// layout's advertiser, which keeps its one bus connection across every registration.
#[derive(Default)]
struct Air {
    extended: Option<bluer::adv::AdvertisementHandle>,
    legacy: Option<legacy_adv::LegacyAdvertiser>,
    /// Whether the advertisement is on the air.
    on: bool,
}

impl Air {
    /// Put the advertisement on the air in the layout `args` names.
    async fn put_on(
        &mut self,
        adapter: &bluer::Adapter,
        args: &Args,
        manufacturer_bytes: Vec<u8>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match args.layout {
            Layout::Extended => {
                self.extended = Some(
                    adapter
                        .advertise(extended_advertisement(manufacturer_bytes))
                        .await?,
                );
            }
            Layout::Legacy => {
                if self.legacy.is_none() {
                    self.legacy = Some(
                        legacy_adv::advertiser(
                            &args.adapter,
                            SERVICE_UUID,
                            ADVERT_COMPANY_ID,
                            manufacturer_bytes,
                        )
                        .await?,
                    );
                    eprintln!(
                        "svolder-sim: legacy layout: service UUID in the advertisement, payload in the scan response"
                    );
                }
                if let Some(legacy) = self.legacy.as_mut() {
                    legacy.register().await?;
                }
            }
        }
        self.on = true;
        Ok(())
    }

    /// Take the advertisement off the air. The extended handle is dropped, which has bluer ask
    /// bluetoothd to unregister it in the background, so it may still be on the air for a moment
    /// when the next one is registered; the legacy advertisement is unregistered before this
    /// returns.
    async fn take_off(&mut self) {
        self.on = false;
        self.extended = None;
        if let Some(legacy) = self.legacy.as_mut() {
            legacy.unregister().await;
        }
    }
}

/// When a failed registration is tried again: a second after the first failure, twice as long
/// after each one that follows, up to half a minute.
#[derive(Default)]
struct Retry {
    wait: Duration,
    next: Option<tokio::time::Instant>,
}

impl Retry {
    /// Whether it is time to try.
    fn due(&self) -> bool {
        self.next.is_none_or(|at| tokio::time::Instant::now() >= at)
    }

    /// Note a failure and say how long until the next try.
    fn failed(&mut self) -> Duration {
        self.wait = (self.wait * 2).clamp(Duration::from_secs(1), Duration::from_secs(30));
        self.next = Some(tokio::time::Instant::now() + self.wait);
        self.wait
    }
}

/// Whether a link has closed since the last look, from how many links the adapter holds each
/// time it is looked at: fewer than before.
#[derive(Default)]
struct LinkWatch {
    held: usize,
}

impl LinkWatch {
    /// Note that the adapter holds `now` links, and say whether that is fewer than at the last look.
    fn closed(&mut self, now: usize) -> bool {
        let closed = now < self.held;
        self.held = now;
        closed
    }
}

/// How many devices the adapter holds a link to, or `None` when BlueZ could not be asked.
async fn connected_links(adapter: &bluer::Adapter) -> Option<usize> {
    let addresses = adapter.device_addresses().await.ok()?;
    let mut held = 0;
    for address in addresses {
        if let Ok(device) = adapter.device(address)
            && matches!(device.is_connected().await, Ok(true))
        {
            held += 1;
        }
    }
    Some(held)
}

/// The GATT application: the Forkbeard service with its TX characteristic, which answers writes
/// through `device`, and its RX characteristic, which stashes the subscriber in `notifier`.
fn gatt_application(
    device: Arc<Mutex<FakeDevice>>,
    notifier: Arc<tokio::sync::Mutex<Option<CharacteristicNotifier>>>,
    trace: bool,
) -> Application {
    let notifier_for_notify = notifier.clone();
    let device_for_write = device.clone();
    let notifier_for_write = notifier.clone();

    Application {
        services: vec![Service {
            uuid: SERVICE_UUID,
            primary: true,
            characteristics: vec![
                Characteristic {
                    uuid: TX_UUID,
                    // Both ATT write opcodes are accepted: a central that writes with a
                    // response reaches the fake device exactly as one that does not.
                    write: Some(CharacteristicWrite {
                        write: true,
                        write_without_response: true,
                        method: CharacteristicWriteMethod::Fun(Box::new(move |value, req| {
                            let device = device_for_write.clone();
                            let notifier = notifier_for_write.clone();
                            async move {
                                if trace {
                                    eprintln!(
                                        "svolder-sim: {} from {} (mtu {})",
                                        describe_tx(&value),
                                        req.device_address,
                                        req.mtu
                                    );
                                }
                                let frames = {
                                    let mut d = device.lock().expect("device mutex");
                                    d.handle(&value)
                                };
                                let mut guard = notifier.lock().await;
                                match guard.as_mut() {
                                    Some(n) => {
                                        for f in frames {
                                            if trace {
                                                eprintln!("svolder-sim: {}", describe_rx(&f));
                                            }
                                            let _ = n.notify(f).await;
                                        }
                                    }
                                    None if trace && !frames.is_empty() => eprintln!(
                                        "svolder-sim: {} answer(s) not sent: RX is not subscribed",
                                        frames.len()
                                    ),
                                    None => {}
                                }
                                Ok(())
                            }
                            .boxed()
                        })),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                Characteristic {
                    uuid: RX_UUID,
                    // A central may enable notifications or indications; bluetoothd delivers
                    // each frame the way that central asked for.
                    notify: Some(CharacteristicNotify {
                        notify: true,
                        indicate: true,
                        method: CharacteristicNotifyMethod::Fun(Box::new(move |n| {
                            let notifier = notifier_for_notify.clone();
                            async move {
                                if trace {
                                    eprintln!("svolder-sim: RX subscribed");
                                }
                                *notifier.lock().await = Some(n);
                            }
                            .boxed()
                        })),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            ],
            ..Default::default()
        }],
        ..Default::default()
    }
}

/// Start the signal task when `playing` is set, and return it so the stop can abort it.
///
/// The signal is the device's own doing, so it is pushed on RX whenever a central is
/// subscribed, without waiting for anything to be written. The frames are left out of the
/// trace: at ten a second they would bury everything else it has to say.
fn start_signal(
    playing: bool,
    device: Arc<Mutex<FakeDevice>>,
    notifier: Arc<tokio::sync::Mutex<Option<CharacteristicNotifier>>>,
) -> Option<tokio::task::JoinHandle<()>> {
    playing.then(|| {
        eprintln!("svolder-sim: playing a signal on the meters");
        let device = device.clone();
        let notifier = notifier.clone();
        tokio::spawn(async move {
            let signal = Signal::new(1);
            let started = std::time::Instant::now();
            let mut ticks = tokio::time::interval(SIGNAL_PERIOD);
            loop {
                ticks.tick().await;
                let elapsed = started.elapsed();
                let frame = {
                    let mut d = device.lock().expect("device mutex");
                    d.signal_frame(&|channel| signal.at(elapsed, channel))
                };
                // A device with nothing to play never had a signal to begin with, which the
                // refusal above has already ruled out.
                let Some(frame) = frame else { return };
                if let Some(n) = notifier.lock().await.as_mut() {
                    let _ = n.notify(frame).await;
                }
            }
        })
    })
}

/// The extended layout's advertisement: the Forkbeard service and `manufacturer_bytes` under
/// the Forkbeard company id in one extended advertising PDU, discoverable, under the
/// simulator's name.
fn extended_advertisement(manufacturer_bytes: Vec<u8>) -> Advertisement {
    let mut manufacturer_data = BTreeMap::new();
    manufacturer_data.insert(ADVERT_COMPANY_ID, manufacturer_bytes);
    Advertisement {
        advertisement_type: bluer::adv::Type::Peripheral,
        service_uuids: [SERVICE_UUID].into_iter().collect(),
        manufacturer_data,
        discoverable: Some(true),
        local_name: Some("svolder-sim".to_string()),
        min_interval: Some(ADVERTISING_INTERVAL),
        max_interval: Some(ADVERTISING_INTERVAL),
        ..Default::default()
    }
}

/// One line for a frame a central wrote to TX: the bytes, the passphrase field, and each message
/// it carries, or the codec's reason for rejecting it.
fn describe_tx(frame: &[u8]) -> String {
    match svolder_proto::decode_command(frame) {
        Ok(command) => format!(
            "TX {}  passphrase field {:#010x}  {}",
            hex(frame),
            command.passphrase_field,
            describe_body(&command.body)
        ),
        Err(e) => format!("TX {}  undecodable: {e}", hex(frame)),
    }
}

/// One line for a notification sent on RX: the bytes and each message it carries.
fn describe_rx(frame: &[u8]) -> String {
    match svolder_proto::decode_notification(frame) {
        Ok(notification) => format!("RX {}  {}", hex(frame), describe_body(&notification)),
        Err(e) => format!("RX {}  undecodable: {e}", hex(frame)),
    }
}

/// The messages of one frame as `<direction> <function> <value>`, separated by ` | `, with the
/// codec's note for each message it dropped.
fn describe_body(notification: &svolder_proto::Notification) -> String {
    let mut parts: Vec<String> = notification
        .messages
        .iter()
        .map(|m| format!("{:?} {} {}", m.direction, m.id, m.value))
        .collect();
    parts.extend(notification.dropped.iter().map(|e| format!("dropped: {e}")));
    if parts.is_empty() {
        "(no messages)".to_string()
    } else {
        parts.join(" | ")
    }
}

/// Lower-case hex, two digits per byte, no separators.
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The signals that stop the simulator: Ctrl-C (SIGINT) and SIGTERM. Registered before the
/// ready line is printed, so a stop sent the moment it appears is caught, and the links are
/// still dropped on the way out.
struct Stops {
    interrupt: tokio::signal::unix::Signal,
    terminate: tokio::signal::unix::Signal,
}

impl Stops {
    /// Register both handlers.
    fn register() -> std::io::Result<Self> {
        use tokio::signal::unix::{SignalKind, signal};
        Ok(Self {
            interrupt: signal(SignalKind::interrupt())?,
            terminate: signal(SignalKind::terminate())?,
        })
    }

    /// Wait for either signal.
    async fn wait(mut self) {
        tokio::select! {
            _ = self.interrupt.recv() => {}
            _ = self.terminate.recv() => {}
        }
    }
}

/// Disconnect every device the adapter is holding a link to, reporting each on stderr so a
/// run's log shows what went down.
async fn drop_links(adapter: &bluer::Adapter) {
    let addresses = match adapter.device_addresses().await {
        Ok(addresses) => addresses,
        Err(e) => {
            eprintln!("svolder-sim: could not list the adapter's devices: {e}");
            return;
        }
    };
    for address in addresses {
        let Ok(device) = adapter.device(address) else {
            continue;
        };
        if !matches!(device.is_connected().await, Ok(true)) {
            continue;
        }
        match tokio::time::timeout(DISCONNECT_TIMEOUT, device.disconnect()).await {
            Ok(Ok(())) => eprintln!("svolder-sim: disconnected {address}"),
            Ok(Err(e)) => eprintln!("svolder-sim: disconnecting {address} failed: {e}"),
            Err(_) => eprintln!("svolder-sim: disconnecting {address} timed out"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use svolder_proto::{
        Direction, FunctionId, Message, Passphrase, Value, encode_command, encode_notification,
    };

    /// A SIGTERM sent once the stop signals are registered, before anything waits on them, is
    /// caught by the wait that follows: the ready line sits in that window.
    #[tokio::test]
    async fn a_stop_sent_before_the_wait_is_caught() {
        let stops = Stops::register().expect("the stop signals register");
        let sent = std::process::Command::new("kill")
            .args(["-TERM", &std::process::id().to_string()])
            .status()
            .expect("kill runs");
        assert!(sent.success());
        tokio::time::timeout(Duration::from_secs(5), stops.wait())
            .await
            .expect("the TERM sent before the wait is caught");
    }

    /// A link closing is noticed once, when the count falls, and a link opening is not one.
    #[test]
    fn a_closed_link_is_noticed_when_the_count_falls() {
        let mut links = LinkWatch::default();
        let seen: Vec<bool> = [0, 1, 1, 0, 0, 1, 2, 1, 0]
            .into_iter()
            .map(|now| links.closed(now))
            .collect();
        assert_eq!(
            seen,
            [false, false, false, true, false, false, false, true, true]
        );
    }

    /// A failed registration is tried again a second later, then twice as far apart, up to 30 s.
    #[test]
    fn a_failed_registration_backs_off() {
        let mut retry = Retry::default();
        assert!(retry.due());
        let waits: Vec<u64> = (0..7).map(|_| retry.failed().as_secs()).collect();
        assert_eq!(waits, [1, 2, 4, 8, 16, 30, 30]);
        assert!(!retry.due());
    }

    fn message(direction: Direction, id: &[u8], value: Value) -> Message {
        Message {
            direction,
            id: FunctionId::new(id).expect("a valid function id"),
            value,
        }
    }

    /// The extended advertisement names its interval rather than leaving BlueZ the kernel's
    /// default of 1.28 s, which gives a 5-second scan window only four advertisements to hear.
    #[test]
    fn the_extended_advertisement_goes_out_every_100_ms() {
        let adv = extended_advertisement(vec![0; 27]);
        assert_eq!(ADVERTISING_INTERVAL, Duration::from_millis(100));
        assert_eq!(adv.min_interval, Some(ADVERTISING_INTERVAL));
        assert_eq!(adv.max_interval, Some(ADVERTISING_INTERVAL));
    }

    #[test]
    fn a_command_frame_is_described_with_its_passphrase_and_message() {
        let passphrase = Passphrase::from_digits([1, 2, 3, 4]).expect("digits");
        let frame = encode_command(
            &message(Direction::Command, &[10], Value::U8(63)),
            passphrase,
        )
        .expect("encodes");
        let line = describe_tx(&frame);
        assert!(line.starts_with("TX "), "{line}");
        assert!(line.contains("passphrase field 0x"), "{line}");
        assert!(line.contains("Command 10 63"), "{line}");
    }

    #[test]
    fn a_notification_frame_is_described_message_by_message() {
        let frame = encode_notification(&[
            message(Direction::Status, &[10], Value::U8(63)),
            message(Direction::Status, &[5], Value::Bool(false)),
        ])
        .expect("encodes");
        let line = describe_rx(&frame);
        assert!(line.contains("Status 10 63 | Status 5 false"), "{line}");
    }

    #[test]
    fn a_frame_the_codec_rejects_says_why() {
        let line = describe_tx(&[0x01, 0x02]);
        assert!(line.contains("undecodable"), "{line}");
        assert_eq!(hex(&[0x00, 0xab, 0xff]), "00abff");
    }
}
