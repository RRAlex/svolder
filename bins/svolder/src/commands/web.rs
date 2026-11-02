// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `svolder web`: open the daemon's web UI in this machine's browser, signed in by
//! a one-use link code; `--qr` prints that link as a QR code for a phone; `web pair` prints a
//! six-digit pairing code.
//!
//! A link carries a code the daemon mints, never the token: `xdg-open`'s arguments are readable
//! by every local user while it runs, and a QR code can be photographed; a code is spent the
//! moment it is used and lasts five minutes.

use std::io::IsTerminal;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::path::Path;
use std::process::{ExitCode, Stdio};

use svolder_ipc::socket::daemon_version::{RESTART_COMMAND, VERSION_WAIT, version_within};
use svolder_ipc::token::Store;
use svolder_ipc::{Client, Config, Request, Response, WebStatus, normal_origin, socket};

use crate::backend::Globals;
use crate::cli::WebCommand;
use crate::commands::token;
use crate::qr;

/// Interface names whose addresses a phone does not reach this computer on: containers' and
/// virtual machines' bridges.
const NOT_LAN: [&str; 4] = ["docker", "br-", "veth", "virbr"];

/// Carry out `web` or one of its sub-commands.
pub(crate) async fn run(
    g: &Globals<'_>,
    qr: bool,
    command: Option<WebCommand>,
) -> Result<ExitCode, String> {
    match command {
        None => open(g, qr).await,
        Some(WebCommand::Pair) => pair(g).await,
        Some(WebCommand::Token { rotate }) => token::token(g, Store::WEB, rotate).await,
    }
}

/// Open the web UI in this machine's browser, or with `qr` print the link for a phone.
async fn open(g: &Globals<'_>, qr_code: bool) -> Result<ExitCode, String> {
    let config = Config::load().map_err(|e| e.to_string())?;
    let socket = socket::path::resolve(g.socket, config.socket.as_deref());
    let listen = served(&socket).await?;
    let base = match qr_code {
        true => phone_base(config.web.url.as_deref(), listen, first_lan(&interfaces()))?,
        false => local_base(listen),
    };
    let (code, secs) = code(&socket, true).await?;
    let link = login_link(&base, &code);
    if qr_code {
        for line in qr_lines(&link, std::io::stdout().is_terminal())? {
            println!("{line}");
        }
        println!("{link}");
        println!("Scan within {}; it signs in one browser.", minutes(secs));
        return Ok(ExitCode::SUCCESS);
    }
    eprintln!("svolder: opening the web UI in your browser: {base}");
    if let Err(why) = open_link(Path::new("xdg-open"), &link) {
        eprintln!(
            "svolder: {why}; open this link within {}, once:",
            minutes(secs)
        );
        println!("{link}");
    }
    Ok(ExitCode::SUCCESS)
}

/// `link` as the lines of a QR code, painted black on white only when `terminal` says the
/// output is one (a pipe or a file gets the plain glyphs).
fn qr_lines(link: &str, terminal: bool) -> Result<Vec<String>, String> {
    let lines = qr::half_blocks(link)?;
    Ok(match terminal {
        true => qr::painted(&lines),
        false => lines,
    })
}

/// Print a pairing code on stdout, and on stderr how long it lasts and where to type it.
async fn pair(g: &Globals<'_>) -> Result<ExitCode, String> {
    let config = Config::load().map_err(|e| e.to_string())?;
    let socket = socket::path::resolve(g.socket, config.socket.as_deref());
    let listen = served(&socket).await?;
    let (code, secs) = code(&socket, false).await?;
    let base = phone_base(config.web.url.as_deref(), listen, first_lan(&interfaces()))
        .unwrap_or_else(|_| local_base(listen));
    println!("{code}");
    eprintln!("svolder: {}", pairing_note(&minutes(secs), &base));
    Ok(ExitCode::SUCCESS)
}

/// Where a link's fragment names the code that signs a browser in.
pub(crate) const LOGIN_FRAGMENT: &str = "#login=";

/// The link that signs a browser in: `base` and the one-use `code` in the fragment, which a
/// browser never sends to a server and a proxy never logs.
fn login_link(base: &str, code: &str) -> String {
    format!("{base}{LOGIN_FRAGMENT}{code}")
}

/// What `web pair` says on stderr: where to type the code and how long it lasts. A pairing code
/// has five tries, and junk sent to the web UI's address can use them up, so the link and the QR
/// code are named as the way round.
fn pairing_note(lasts: &str, base: &str) -> String {
    format!(
        "type it within {lasts} on the web UI's sign-in page, under \"or pair this browser\": \
         {base}; if it stops working, `svolder web` opens a link and `svolder web --qr` shows one \
         for a phone, and neither can be used up by anyone else's tries"
    )
}

/// Where the web UI of the daemon on `socket` listens, or why there is nothing to reach.
async fn served(socket: &Path) -> Result<SocketAddr, String> {
    match version_within(socket, VERSION_WAIT).await {
        Some(info) => listening_at(info.web.as_ref()),
        None => Err(format!(
            "no daemon answers on {}: the daemon serves the web UI; {}",
            socket.display(),
            svolder_ipc::WEB_OFF
                .split_once(": ")
                .map_or("", |(_, fix)| fix)
        )),
    }
}

/// The address a web UI that `web` describes listens on, or why there is none, naming the fix.
fn listening_at(web: Option<&WebStatus>) -> Result<SocketAddr, String> {
    match web {
        Some(WebStatus::Listening { listen, .. }) => Ok(*listen),
        Some(WebStatus::Off) => Err(svolder_ipc::WEB_OFF.to_string()),
        Some(WebStatus::Failed { listen, error }) => Err(format!(
            "the web UI could not start on {listen}: {error}; see `svolder doctor`"
        )),
        Some(WebStatus::Unknown) | None => Err(format!(
            "this daemon serves no web UI: it is older than this svolder; run {RESTART_COMMAND}"
        )),
    }
}

/// A link code (`link`) or a pairing code from the daemon on `socket`, and its seconds.
async fn code(socket: &Path, link: bool) -> Result<(String, u64), String> {
    let mut client = Client::connect(socket).await.map_err(|e| e.to_string())?;
    let ask = match link {
        true => Request::WebLink,
        false => Request::WebPair,
    };
    match client.request(ask).await.map_err(|e| e.to_string())? {
        Response::WebLink {
            code,
            expires_in_secs,
        }
        | Response::WebPair {
            code,
            expires_in_secs,
        } => Ok((code, expires_in_secs)),
        Response::Error { message, .. } => Err(message),
        other => Err(format!("the daemon answered {other:?}")),
    }
}

/// The address this machine's browser opens: the loopback address on a loopback or wildcard
/// listen, the port kept, and the listen address otherwise.
pub(crate) fn local_base(listen: SocketAddr) -> String {
    let ip = match listen.ip() {
        IpAddr::V4(ip) if ip.is_unspecified() => IpAddr::V4(Ipv4Addr::LOCALHOST),
        IpAddr::V6(ip) if ip.is_unspecified() => IpAddr::V6(Ipv6Addr::LOCALHOST),
        ip => ip,
    };
    format!("http://{}/", SocketAddr::new(ip, listen.port()))
}

/// The address a phone opens: `[web] url`, else the listen address, else on a wildcard listen
/// `lan`. A loopback listen with no url is refused: a phone cannot reach it.
pub(crate) fn phone_base(
    url: Option<&str>,
    listen: SocketAddr,
    lan: Option<Ipv4Addr>,
) -> Result<String, String> {
    if let Some(url) = url {
        return url_base(url);
    }
    let ip = listen.ip();
    if ip.is_loopback() {
        return Err(format!(
            "a phone cannot reach {ip}; choose your network in the TUI's Web box, or set [web] \
             listen"
        ));
    }
    if !ip.is_unspecified() {
        return Ok(format!("http://{listen}/"));
    }
    lan.map(|lan| {
        format!(
            "http://{}/",
            SocketAddr::new(IpAddr::V4(lan), listen.port())
        )
    })
    .ok_or_else(|| "no network address of this computer was found: set [web] url".to_string())
}

/// `[web] url` as the base of a link: its scheme, host, port and path put together again, the
/// path ending in `/`. An address with user info, a port that is not a number or a scheme other
/// than http and https is refused, without echoing it.
fn url_base(url: &str) -> Result<String, String> {
    let refused = || {
        "[web] url must be an http:// or https:// address with a host and an optional numeric \
         port, no user info, query or fragment, such as \"https://svolder.home.lan/\""
            .to_string()
    };
    let (scheme, rest) = url.trim().split_once("://").ok_or_else(refused)?;
    let (authority, path) = match rest.find('/') {
        Some(at) => rest.split_at(at),
        None => (rest, "/"),
    };
    if path.contains(['?', '#']) {
        return Err(refused());
    }
    let origin = normal_origin(&format!("{scheme}://{authority}")).ok_or_else(refused)?;
    Ok(match path.ends_with('/') {
        true => format!("{origin}{path}"),
        false => format!("{origin}{path}/"),
    })
}

/// The first up, non-loopback IPv4 address among `interfaces` (name, address, up, loopback) of
/// an interface not named for a container or a virtual bridge.
pub(crate) fn first_lan(interfaces: &[(String, Ipv4Addr, bool, bool)]) -> Option<Ipv4Addr> {
    interfaces
        .iter()
        .find(|(name, ip, up, loopback)| {
            *up && !*loopback
                && !ip.is_loopback()
                && !NOT_LAN.iter().any(|prefix| name.starts_with(prefix))
        })
        .map(|(_, ip, ..)| *ip)
}

/// This computer's interfaces that have an IPv4 address, as `getifaddrs` lists them.
pub(crate) fn interfaces() -> Vec<(String, Ipv4Addr, bool, bool)> {
    let mut out = Vec::new();
    let mut list: *mut libc::ifaddrs = std::ptr::null_mut();
    // SAFETY: `getifaddrs` fills `list` with a list it allocated, read here and handed back to
    // `freeifaddrs` once; each entry's name is a NUL-terminated string, and its address, when not
    // null, is a `sockaddr` of the family it names, here `sockaddr_in`.
    unsafe {
        if libc::getifaddrs(&mut list) != 0 {
            return out;
        }
        let mut at = list;
        while !at.is_null() {
            let entry = &*at;
            if !entry.ifa_addr.is_null() && i32::from((*entry.ifa_addr).sa_family) == libc::AF_INET
            {
                let address = &*entry.ifa_addr.cast::<libc::sockaddr_in>();
                let ip = Ipv4Addr::from(u32::from_be(address.sin_addr.s_addr));
                let name = std::ffi::CStr::from_ptr(entry.ifa_name)
                    .to_string_lossy()
                    .into_owned();
                let flags = entry.ifa_flags;
                out.push((
                    name,
                    ip,
                    flags & libc::IFF_UP as u32 != 0,
                    flags & libc::IFF_LOOPBACK as u32 != 0,
                ));
            }
            at = entry.ifa_next;
        }
        libc::freeifaddrs(list);
    }
    out
}

/// How long an opener has to fail before it is taken to have opened the browser.
const OPENER_MOMENT: std::time::Duration = std::time::Duration::from_secs(2);

/// Hand `link` to `opener`, quietly: an opener that is missing or fails says why.
pub(crate) fn open_link(opener: &Path, link: &str) -> Result<(), String> {
    open_link_within(opener, link, OPENER_MOMENT)
}

/// Hand `link` to `opener` in a process group of its own (so a Ctrl-C here does not close the
/// browser) and wait up to `moment` for it: one that has failed by then is reported, one still
/// running is the browser held in the foreground (xdg-open without a desktop) and is left, a
/// thread waiting for it so that it does not stay a defunct process once it ends.
fn open_link_within(opener: &Path, link: &str, moment: std::time::Duration) -> Result<(), String> {
    use std::os::unix::process::CommandExt;
    let mut child = std::process::Command::new(opener)
        .arg(link)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .map_err(|e| format!("{} could not be run: {e}", opener.display()))?;
    let until = std::time::Instant::now() + moment;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(status)) => {
                return Err(format!(
                    "{} could not open the browser ({status})",
                    opener.display()
                ));
            }
            Ok(None) if std::time::Instant::now() < until => {
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            Ok(None) => {
                std::thread::spawn(move || {
                    let _ = child.wait();
                });
                return Ok(());
            }
            Err(e) => return Err(format!("{} could not be waited for: {e}", opener.display())),
        }
    }
}

/// `secs` in words: `5 minutes`, `1 minute`, `90 seconds`.
fn minutes(secs: u64) -> String {
    match (secs % 60, secs / 60) {
        (0, 1) => "1 minute".to_string(),
        (0, m) => format!("{m} minutes"),
        _ => format!("{secs} seconds"),
    }
}

#[cfg(test)]
mod tests;
