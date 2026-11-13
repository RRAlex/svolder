// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A loopback HTTP server for tests: it answers each request from a route function, on a port
//! the system picks, so no test reaches a network.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// What the test server answers one request with.
pub enum Reply {
    /// 200 with this body.
    Ok(Vec<u8>),
    /// An empty answer with this status.
    Status(u16),
    /// A 302 to this location.
    Redirect(String),
    /// Reads the request and then says nothing until the test ends.
    Silent,
    /// A 200 with a body of this many bytes, written until the client hangs up.
    Flood(usize),
}

/// A 127.0.0.1 HTTP/1.1 server answering each request by `route(path)`, closing after one
/// response. It records every request line and header.
pub struct Server {
    /// The base URL of the server, ending in its results directory.
    pub url: String,
    seen: Arc<Mutex<Vec<String>>>,
    hits: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    addr: std::net::SocketAddr,
}

impl Server {
    /// Start a server answering each request by `route(path)`.
    pub fn start(route: impl Fn(&str) -> Reply + Send + Sync + 'static) -> Server {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let hits = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let route = Arc::new(route);
        let (seen2, hits2, stop2) = (seen.clone(), hits.clone(), stop.clone());
        thread::spawn(move || {
            for stream in listener.incoming() {
                if stop2.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(stream) = stream else { continue };
                let (route, seen, hits, stop) =
                    (route.clone(), seen2.clone(), hits2.clone(), stop2.clone());
                thread::spawn(move || serve(stream, &*route, &seen, &hits, &stop));
            }
        });
        Server {
            url: format!("http://{addr}/results/"),
            seen,
            hits,
            stop,
            addr,
        }
    }

    /// How many requests came in.
    pub fn hits(&self) -> usize {
        self.hits.load(Ordering::SeqCst)
    }

    /// Every request line and header seen, in order.
    pub fn requests(&self) -> Vec<String> {
        self.seen.lock().unwrap().clone()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(self.addr);
    }
}

fn serve(
    mut stream: TcpStream,
    route: &dyn Fn(&str) -> Reply,
    seen: &Mutex<Vec<String>>,
    hits: &AtomicUsize,
    stop: &AtomicBool,
) {
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        match stream.read(&mut byte) {
            Ok(1) => head.push(byte[0]),
            _ => return,
        }
    }
    let head = String::from_utf8_lossy(&head).into_owned();
    let path = head
        .lines()
        .next()
        .and_then(|line| line.split(' ').nth(1))
        .unwrap_or("/")
        .to_string();
    hits.fetch_add(1, Ordering::SeqCst);
    seen.lock()
        .unwrap()
        .extend(head.lines().map(str::to_string));
    let head_of = |status: &str, extra: &str, len: usize| {
        format!("HTTP/1.1 {status}\r\n{extra}Content-Length: {len}\r\nConnection: close\r\n\r\n")
    };
    match route(&path) {
        Reply::Ok(body) => {
            let _ = stream.write_all(head_of("200 OK", "", body.len()).as_bytes());
            let _ = stream.write_all(&body);
        }
        Reply::Status(code) => {
            let _ = stream.write_all(head_of(&format!("{code} Nope"), "", 0).as_bytes());
        }
        Reply::Redirect(to) => {
            let extra = format!("Location: {to}\r\n");
            let _ = stream.write_all(head_of("302 Found", &extra, 0).as_bytes());
        }
        Reply::Silent => {
            while !stop.load(Ordering::SeqCst) {
                thread::sleep(Duration::from_millis(20));
            }
        }
        Reply::Flood(total) => {
            let _ = stream.write_all(head_of("200 OK", "", total).as_bytes());
            let chunk = [b'x'; 8192];
            let mut sent = 0;
            while sent < total && stream.write_all(&chunk).is_ok() {
                sent += chunk.len();
            }
        }
    }
}
