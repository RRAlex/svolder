// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Live BlueZ transport test. Ignored by default; `make e2e` runs it with a peripheral
//! advertising on a second controller. Requires two Bluetooth controllers.
use btleplug::api::{Central, Manager as _, Peripheral as _};
use btleplug::platform::{Manager, Peripheral};
use futures::StreamExt;
use std::time::Duration;
use svolder_ble::bluez::advertisement_from_manufacturer_data;
use svolder_ble::{BluezTransport, Transport};
use svolder_proto::{
    FunctionId, Message, Passphrase, UnitId, Value, decode_notification, encode_command,
};

/// How often a live test re-reads a state BlueZ needs a moment to reach.
const POLL: Duration = Duration::from_millis(250);

/// How long one scan listens: svolder-sim advertises every 100 ms, so a window this long holds
/// about fifty of its advertisements. `scripts/e2e.sh` keeps a copy of this constant and of the
/// two below for its own scans; a change to one copy must be made to the other.
const SCAN_WINDOW: Duration = Duration::from_secs(5);
/// How many of those windows a test gives the radio before it calls the peripheral absent.
/// `SCAN_TRIES` in `scripts/e2e.sh` must follow it.
const SCAN_TRIES: usize = 3;
/// How long a test waits between two of those windows. `SCAN_BREATH` in `scripts/e2e.sh` must
/// follow it.
const SCAN_BREATH: Duration = Duration::from_secs(1);

/// The controller `make e2e` drives the central side with, and the unit the peripheral
/// advertises.
fn central_and_unit() -> (Option<String>, UnitId) {
    let adapter = std::env::var("SVOLDER_E2E_SCAN_ADAPTER").ok();
    let unit: UnitId = std::env::var("SVOLDER_E2E_UNIT")
        .unwrap_or_else(|_| "0102030405060708".into())
        .parse()
        .unwrap();
    (adapter, unit)
}

/// How long a test gives BlueZ to reach a connection state: enough for the transport's own
/// find window and connect timeout, the open steps that follow them, and the release that
/// follows those.
const LINK_BUDGET: Duration = Duration::from_secs(20);

/// BlueZ's own handle on the device advertising `unit`, reached through a second btleplug
/// session so a test can ask whether that device is connected without going through the
/// transport. It starts no discovery: the device is one BlueZ already holds, from a sighting
/// the transport took.
async fn held_peripheral_of(adapter: Option<&str>, unit: UnitId) -> Peripheral {
    let manager = Manager::new().await.expect("btleplug manager");
    let mut central = None;
    for a in manager.adapters().await.expect("adapters") {
        let named = match adapter {
            None => true,
            Some(name) => a
                .adapter_info()
                .await
                .is_ok_and(|info| info.split_whitespace().next() == Some(name)),
        };
        if named {
            central = Some(a);
            break;
        }
    }
    let central = central.expect("the central controller the e2e run names");
    for p in central.peripherals().await.expect("peripherals") {
        if let Ok(Some(props)) = p.properties().await
            && let Some(a) =
                advertisement_from_manufacturer_data(&props.manufacturer_data, props.rssi)
            && a.unit_id() == unit
        {
            return p;
        }
    }
    panic!("BlueZ holds no device advertising {unit}; the transport's sight should have left one")
}

/// Wait for BlueZ to report `peripheral` as `connected`, polling until it does or `window`
/// runs out; true when it got there. A device BlueZ will not answer for counts as not
/// connected, so a peer that has gone altogether reads as down.
async fn connection_becomes(peripheral: &Peripheral, connected: bool, window: Duration) -> bool {
    let deadline = tokio::time::Instant::now() + window;
    loop {
        if matches!(peripheral.is_connected().await, Ok(true)) == connected {
            return true;
        }
        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(POLL).await;
    }
}

#[tokio::test]
#[ignore = "needs a real/virtual radio and a running svolder-sim peripheral; run via make e2e"]
async fn scan_finds_the_peripheral() {
    let (adapter, unit) = central_and_unit();
    let t = BluezTransport::new(adapter.as_deref())
        .await
        .expect("open adapter");
    let mut seen = false;
    for attempt in 0..SCAN_TRIES {
        if attempt > 0 {
            tokio::time::sleep(SCAN_BREATH).await;
        }
        let mut stream = t.scan(SCAN_WINDOW).await.expect("scan");
        while let Some(a) = stream.next().await {
            seen |= a.unit_id() == unit;
        }
        if seen {
            break;
        }
    }
    assert!(
        seen,
        "peripheral {unit} not listed by {SCAN_TRIES} scans of {SCAN_WINDOW:?}"
    );
}

#[tokio::test]
#[ignore = "needs a real/virtual radio and a running svolder-sim peripheral; run via make e2e"]
async fn connect_writes_a_command_and_receives_a_status() {
    let (adapter, unit) = central_and_unit();
    let t = BluezTransport::new(adapter.as_deref())
        .await
        .expect("open adapter");
    let link = t.connect(unit).await.expect("connect");
    let mut notifications = link.notifications();

    // Function 1 asks for a full refresh, which svolder-sim answers with every control's
    // status. The digits are the ones `scripts/e2e.sh` starts the peripheral with, in its
    // `--passphrase` argument; a frame encoded with any others is answered with a refusal.
    let passphrase = Passphrase::from_digits([1, 2, 3, 4]).expect("passphrase");
    let frame = encode_command(
        &Message::command(FunctionId::one(1), Value::Data(vec![])),
        passphrase,
    )
    .expect("encode refresh");
    link.write(&frame).await.expect("write refresh");

    let bytes = tokio::time::timeout(Duration::from_secs(5), notifications.next())
        .await
        .expect("a status notification within 5s")
        .expect("the notification stream ended");
    let notification = decode_notification(&bytes).expect("decode the status notification");
    assert!(
        !notification.messages.is_empty(),
        "the refresh reply carried no messages"
    );
    link.disconnect().await.expect("disconnect");
}

#[tokio::test]
#[ignore = "needs a real/virtual radio and a running svolder-sim peripheral; run via make e2e"]
async fn a_cut_connect_leaves_the_peripheral_disconnected() {
    let (adapter, unit) = central_and_unit();
    let t = BluezTransport::new(adapter.as_deref())
        .await
        .expect("open adapter");
    // The transport's one scan: it stops when the unit is heard and keeps the sighting for
    // the connect below, which then scans nothing.
    t.sight(unit, LINK_BUDGET)
        .await
        .expect("sight")
        .unwrap_or_else(|| panic!("{unit} was not heard within {LINK_BUDGET:?}"));
    let peripheral = held_peripheral_of(adapter.as_deref(), unit).await;

    // The cut waits on BlueZ, not on a clock: what gets dropped is a connect whose link is
    // up and whose remaining open steps are still running, which is the only state in which
    // there is anything to release. A connect that answers first is the test failing to set
    // that state up, not the transport passing.
    let up = tokio::select! {
        attempt = t.connect(unit) => match attempt {
            Ok(_) => panic!("connect handed over a link before BlueZ reported one, so nothing was dropped mid-connection"),
            Err(e) => panic!("connect failed before BlueZ reported a link: {e}"),
        },
        up = connection_becomes(&peripheral, true, LINK_BUDGET) => up,
    };
    assert!(
        up,
        "BlueZ never reported the link up within {LINK_BUDGET:?}, so the connect was never cut mid-setup"
    );

    // The release runs after the open steps it interrupted finish, so the wait covers those
    // as well as the disconnect itself.
    assert!(
        connection_becomes(&peripheral, false, LINK_BUDGET).await,
        "the peripheral is still connected {LINK_BUDGET:?} after the connect was dropped"
    );
}

#[tokio::test]
#[ignore = "needs a real/virtual radio and a running svolder-sim peripheral; run via make e2e"]
async fn the_transport_reports_the_adapter_it_opened() {
    let (adapter, _unit) = central_and_unit();
    let named = adapter.expect("SVOLDER_E2E_SCAN_ADAPTER names the central controller");
    let t = BluezTransport::new(Some(&named))
        .await
        .expect("open the named adapter");
    assert_eq!(t.adapter(), Some(named.as_str()));
}

#[tokio::test]
#[ignore = "needs a real/virtual radio and a running svolder-sim peripheral; run via make e2e"]
async fn the_link_ends_when_the_peripheral_goes_away() {
    let (adapter, unit) = central_and_unit();
    let sim_pid = std::env::var("SVOLDER_E2E_SIM_PID")
        .expect("SVOLDER_E2E_SIM_PID names the svolder-sim process this test stops");
    let t = BluezTransport::new(adapter.as_deref())
        .await
        .expect("open adapter");
    let link = t.connect(unit).await.expect("connect");
    let mut notifications = link.notifications();

    let killed = std::process::Command::new("kill")
        .arg(&sim_pid)
        .status()
        .expect("run kill");
    assert!(killed.success(), "kill {sim_pid} exited {killed}");

    let drained = tokio::time::timeout(Duration::from_secs(5), async {
        while notifications.next().await.is_some() {}
    })
    .await;
    assert!(
        drained.is_ok(),
        "the notification stream did not end within 5s of the peripheral going away"
    );
}
