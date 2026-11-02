// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::net::Ipv4Addr;

use svolder_ipc::config::{WebAuth, WebGuest};
use svolder_ipc::{WEB_OFF, WebStatus};

use super::*;
use crate::test_support::stub_daemon::{STUB_LINK_CODE, STUB_PAIR_CODE, stub_daemon};

fn addr(text: &str) -> SocketAddr {
    text.parse().unwrap()
}

fn listening(listen: &str) -> WebStatus {
    WebStatus::Listening {
        listen: addr(listen),
        auth: WebAuth::Token,
        writes: true,
        guest: WebGuest::None,
    }
}

/// This machine's browser opens the loopback address on a loopback or wildcard listen, and the
/// listen address otherwise.
#[test]
fn the_local_base_is_loopback_on_a_wildcard_listen() {
    assert_eq!(local_base(addr("127.0.0.1:8738")), "http://127.0.0.1:8738/");
    assert_eq!(local_base(addr("0.0.0.0:9000")), "http://127.0.0.1:9000/");
    assert_eq!(local_base(addr("[::]:8738")), "http://[::1]:8738/");
    assert_eq!(
        local_base(addr("192.168.1.20:8738")),
        "http://192.168.1.20:8738/"
    );
}

/// A phone's base is `[web] url`, else the listen address, else on a wildcard listen the LAN
/// address; on a loopback listen it is refused with the way out.
#[test]
fn a_phones_base_follows_the_url_then_the_listen_address_then_the_lan() {
    let lan = Some(Ipv4Addr::new(192, 168, 1, 20));
    assert_eq!(
        phone_base(
            Some("https://svolder.home.lan/sv"),
            addr("127.0.0.1:8738"),
            lan
        )
        .unwrap(),
        "https://svolder.home.lan/sv/"
    );
    assert_eq!(
        phone_base(None, addr("10.0.0.5:8738"), lan).unwrap(),
        "http://10.0.0.5:8738/"
    );
    assert_eq!(
        phone_base(None, addr("0.0.0.0:8738"), lan).unwrap(),
        "http://192.168.1.20:8738/"
    );
    assert_eq!(
        phone_base(None, addr("127.0.0.1:8738"), lan).unwrap_err(),
        "a phone cannot reach 127.0.0.1; choose your network in the TUI's Web box, or set [web] \
         listen"
    );
    assert!(
        phone_base(None, addr("0.0.0.0:8738"), None)
            .unwrap_err()
            .contains("[web] url")
    );
}

/// The base is rebuilt from the url's scheme, host, port and path: a sub-path that ends in `/`
/// is kept as it is, and an authority with user info or a port that is not a number is refused,
/// never spliced into a link.
#[test]
fn a_url_becomes_a_base_from_its_parts_and_never_carries_user_info() {
    let listen = addr("127.0.0.1:8738");
    let base = |url| phone_base(Some(url), listen, None);
    assert_eq!(
        base("http://Media.local:9000/sv/ui/").unwrap(),
        "http://media.local:9000/sv/ui/"
    );
    assert_eq!(
        base("https://svolder.home.lan").unwrap(),
        "https://svolder.home.lan/"
    );
    assert_eq!(
        base("http://[fd00::5]:8738/").unwrap(),
        "http://[fd00::5]:8738/"
    );
    for refused in [
        "http://user:secret@svolder.home.lan/",
        "http://svolder.home.lan@evil.example/",
        "http://svolder.home.lan:http/",
        "http://svolder.home.lan:/sv/",
        "ftp://svolder.home.lan/",
    ] {
        let why = base(refused).unwrap_err();
        assert!(why.contains("[web] url"), "{refused}: {why}");
        assert!(!why.contains("secret"), "{refused}: {why}");
    }
}

/// The LAN address is the first up, non-loopback IPv4 address of an interface that is not a
/// container's or a virtual bridge's.
#[test]
fn the_lan_address_skips_loopback_down_and_bridge_interfaces() {
    let ip = |d| Ipv4Addr::new(10, 0, 0, d);
    let interfaces = [
        ("lo".to_string(), Ipv4Addr::LOCALHOST, true, true),
        ("docker0".to_string(), ip(1), true, false),
        ("br-5f2a".to_string(), ip(2), true, false),
        ("veth12".to_string(), ip(3), true, false),
        ("virbr0".to_string(), ip(4), true, false),
        ("wlan0".to_string(), ip(5), false, false),
        ("eth0".to_string(), ip(6), true, false),
        ("eth1".to_string(), ip(7), true, false),
    ];
    assert_eq!(first_lan(&interfaces), Some(ip(6)));
    assert_eq!(first_lan(&interfaces[..6]), None);
}

/// A web UI that is off, failed or unknown to an older daemon is refused in words that name
/// the fix.
#[test]
fn an_off_failed_or_missing_web_ui_names_the_fix() {
    assert_eq!(
        listening_at(Some(&listening("0.0.0.0:8738"))),
        Ok(addr("0.0.0.0:8738"))
    );
    assert_eq!(listening_at(Some(&WebStatus::Off)).unwrap_err(), WEB_OFF);
    let failed = WebStatus::Failed {
        listen: addr("0.0.0.0:8738"),
        error: "address in use".into(),
    };
    assert!(
        listening_at(Some(&failed))
            .unwrap_err()
            .contains("address in use")
    );
    assert!(
        listening_at(None)
            .unwrap_err()
            .contains("svolder daemon restart")
    );
}

/// A link that the opener takes is opened; one it refuses, or an opener that is not there, is
/// reported so the link can be printed.
#[test]
fn a_link_the_opener_refuses_is_reported() {
    assert!(open_link(Path::new("/bin/true"), "http://127.0.0.1:8738/#login=x").is_ok());
    assert!(open_link(Path::new("/bin/false"), "http://127.0.0.1:8738/#login=x").is_err());
    assert!(open_link(Path::new("/nonexistent/xdg-open"), "http://127.0.0.1:8738/").is_err());
}

/// An opener that stays in the foreground (xdg-open running a browser itself) does not hold the
/// command: it is left running in its own process group once it has had its moment, and one that
/// fails inside that moment is still reported.
#[test]
fn an_opener_that_stays_in_the_foreground_does_not_hold_the_command() {
    let began = std::time::Instant::now();
    // `sleep 3` stands in for a browser the opener runs and waits for; it ends on its own.
    let opened = open_link_within(
        Path::new("/bin/sleep"),
        "3",
        std::time::Duration::from_millis(300),
    );
    assert!(opened.is_ok(), "{opened:?}");
    assert!(began.elapsed() < std::time::Duration::from_secs(5));
    let failed = open_link_within(
        Path::new("/bin/false"),
        "x",
        std::time::Duration::from_secs(5),
    );
    assert!(failed.is_err());
}

/// An opener still running after its moment is waited for on a thread of its own, so it does not
/// stay a defunct process once it ends while the program (the TUI) goes on.
#[test]
fn a_slow_opener_is_reaped_when_it_ends() {
    let dir = std::env::temp_dir().join(format!("svolder-opener-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let script = dir.join("opener.sh");
    std::fs::write(&script, "echo $$ > \"$0.pid\"\nsleep 1\n").unwrap();
    let opened = open_link_within(
        Path::new("/bin/sh"),
        script.to_str().unwrap(),
        std::time::Duration::from_millis(200),
    );
    assert!(opened.is_ok(), "{opened:?}");
    let pid = std::fs::read_to_string(dir.join("opener.sh.pid")).unwrap();
    let alive = Path::new("/proc").join(pid.trim());
    let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while alive.exists() && std::time::Instant::now() < until {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let _ = std::fs::remove_dir_all(&dir);
    assert!(!alive.exists(), "the opener's process was not reaped");
}

/// The QR code is painted in colour for a terminal and plain glyphs for a pipe or a file.
#[test]
fn the_qr_code_is_painted_only_for_a_terminal() {
    let link = "http://192.0.2.5:8738/#login=abc";
    let piped = qr_lines(link, false).unwrap();
    assert!(piped.iter().all(|line| !line.contains('\x1b')), "{piped:?}");
    let shown = qr_lines(link, true).unwrap();
    assert!(shown.iter().all(|line| line.contains("\x1b[")));
    assert_eq!(shown.len(), piped.len());
}

/// The link carries the code in the fragment only, after the base.
#[test]
fn the_link_carries_the_code_in_the_fragment_only() {
    let link = login_link("https://svolder.home.lan/sv/", "abc_DEF-123");
    assert_eq!(link, "https://svolder.home.lan/sv/#login=abc_DEF-123");
    assert!(!link.contains('?'));
}

/// A pairing code that is spent or burned is not reissued by `pair`: the words of its printout
/// point to the link and `--qr`.
#[test]
fn the_pairing_note_offers_the_link_and_the_qr_code() {
    let note = pairing_note("5 minutes", "http://127.0.0.1:8738/");
    assert!(note.contains("within 5 minutes"), "{note}");
    assert!(
        note.contains("svolder web --qr") && note.contains("svolder web"),
        "{note}"
    );
}

/// The codes come from the daemon's web UI, and a daemon whose web UI is off says how to turn
/// it on.
#[tokio::test]
async fn the_codes_come_from_the_daemons_web_ui() {
    let daemon = stub_daemon().await;
    daemon.says_web(Some(WebStatus::Off), None);
    assert_eq!(served(&daemon.socket).await.unwrap_err(), WEB_OFF);
    assert_eq!(code(&daemon.socket, true).await.unwrap_err(), WEB_OFF);
    daemon.says_web(Some(listening("127.0.0.1:8738")), None);
    assert_eq!(served(&daemon.socket).await, Ok(addr("127.0.0.1:8738")));
    assert_eq!(
        code(&daemon.socket, true).await,
        Ok((STUB_LINK_CODE.to_string(), 120))
    );
    assert_eq!(
        code(&daemon.socket, false).await,
        Ok((STUB_PAIR_CODE.to_string(), 120))
    );
    let nobody = crate::test_support::stub_daemon::fresh_socket();
    assert!(
        served(&nobody)
            .await
            .unwrap_err()
            .contains("no daemon answers")
    );
}
