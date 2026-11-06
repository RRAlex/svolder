// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::ffi::OsStr;
use std::path::Path;

use super::*;

/// The PNG signature followed by a few bytes: enough for the magic check, not a real picture.
const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x01\0\0\0\x01";
const JPEG: &[u8] = b"\xff\xd8\xff\xe0\0\x10JFIF\0";
const ART: &str = "https://i.scdn.co/image/ab67616d0000b273dabb260cf41a2fbb2c842787";

/// A fresh empty directory under the system's temp directory, named for the test.
fn scratch(what: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("svolder-cover-{what}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn places(roots: &[&Path], spotify: &[&Path]) -> CoverPlaces {
    CoverPlaces {
        roots: roots.iter().map(|p| p.to_path_buf()).collect(),
        spotify: spotify.iter().map(|p| p.to_path_buf()).collect(),
    }
}

fn file_url(path: &Path) -> String {
    format!("file://{}", path.display())
}

/// A Spotify cache file for `art_url` as Chromium's simple cache lays it out: the 24-byte
/// header (key length at offset 12), the key, the body, the EOF magic, then other records.
fn cache_file(key: &str, body: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8; 24];
    out[12..16].copy_from_slice(&(key.len() as u32).to_le_bytes());
    out.extend_from_slice(key.as_bytes());
    out.extend_from_slice(body);
    out.extend_from_slice(&EOF_MAGIC);
    out.extend_from_slice(b"\0\0\0\0junk and the second stream");
    out
}

fn spotify_key(art_url: &str) -> String {
    format!("1/0/_dk_https://spotify.com https://spotify.com {art_url}")
}

#[test]
fn a_file_cover_under_a_root_is_read() {
    let dir = scratch("file");
    let path = dir.join("cover.png");
    std::fs::write(&path, PNG).unwrap();
    let got = cover(&file_url(&path), &places(&[&dir], &[]));
    std::fs::remove_dir_all(&dir).unwrap();
    let got = got.expect("a cover");
    assert_eq!(got.mime, "image/png");
    assert_eq!(got.bytes, PNG);
}

#[test]
fn jpeg_and_webp_are_read_too() {
    let dir = scratch("kinds");
    let jpg = dir.join("a.jpg");
    let webp = dir.join("a.webp");
    std::fs::write(&jpg, JPEG).unwrap();
    std::fs::write(&webp, b"RIFF\x10\0\0\0WEBPVP8 ").unwrap();
    let places = places(&[&dir], &[]);
    let jpg = cover(&file_url(&jpg), &places);
    let webp = cover(&file_url(&webp), &places);
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(jpg.unwrap().mime, "image/jpeg");
    assert_eq!(webp.unwrap().mime, "image/webp");
}

#[test]
fn a_percent_encoded_path_is_decoded_and_localhost_is_accepted() {
    let dir = scratch("encoded");
    let path = dir.join("my cover é.png");
    std::fs::write(&path, PNG).unwrap();
    let encoded = format!("file://localhost{}/my%20cover%20%C3%A9.png", dir.display());
    let got = cover(&encoded, &places(&[&dir], &[]));
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(got.unwrap().bytes, PNG);
}

#[test]
fn a_bad_escape_or_a_foreign_host_is_none() {
    let dir = scratch("badurl");
    let path = dir.join("cover.png");
    std::fs::write(&path, PNG).unwrap();
    let places = places(&[&dir], &[]);
    let base = dir.display();
    let bad = [
        format!("file://{base}/cover%2.png"),
        format!("file://{base}/cover%zz.png"),
        format!("file://{base}/cover.png%00"),
        format!("file://example.org{base}/cover.png"),
        "file://".to_string(),
        "file:relative.png".to_string(),
        String::new(),
    ];
    let got: Vec<bool> = bad
        .iter()
        .map(|url| cover(url, &places).is_some())
        .collect();
    std::fs::remove_dir_all(&dir).unwrap();
    assert!(got.iter().all(|found| !found), "{got:?}");
}

#[test]
fn a_file_outside_the_roots_is_refused() {
    let root = scratch("root");
    let elsewhere = scratch("elsewhere");
    let path = elsewhere.join("cover.png");
    std::fs::write(&path, PNG).unwrap();
    let got = cover(&file_url(&path), &places(&[&root], &[]));
    std::fs::remove_dir_all(&root).unwrap();
    std::fs::remove_dir_all(&elsewhere).unwrap();
    assert!(got.is_none());
}

#[test]
fn a_dotdot_path_out_of_the_roots_is_refused() {
    let root = scratch("dotdot-root");
    let elsewhere = scratch("dotdot-else");
    std::fs::write(elsewhere.join("cover.png"), PNG).unwrap();
    let sneaky = format!(
        "file://{}/../{}/cover.png",
        root.display(),
        elsewhere.file_name().unwrap().to_str().unwrap()
    );
    let got = cover(&sneaky, &places(&[&root], &[]));
    std::fs::remove_dir_all(&root).unwrap();
    std::fs::remove_dir_all(&elsewhere).unwrap();
    assert!(got.is_none());
}

#[test]
fn a_symlink_out_of_the_roots_is_refused() {
    let root = scratch("link-root");
    let elsewhere = scratch("link-else");
    let target = elsewhere.join("secret.png");
    std::fs::write(&target, PNG).unwrap();
    let link = root.join("cover.png");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    let got = cover(&file_url(&link), &places(&[&root], &[]));
    std::fs::remove_dir_all(&root).unwrap();
    std::fs::remove_dir_all(&elsewhere).unwrap();
    assert!(got.is_none());
}

#[test]
fn a_symlink_inside_the_roots_is_followed() {
    let root = scratch("link-in");
    let target = root.join("real.png");
    std::fs::write(&target, PNG).unwrap();
    let link = root.join("cover.png");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    let got = cover(&file_url(&link), &places(&[&root], &[]));
    std::fs::remove_dir_all(&root).unwrap();
    assert_eq!(got.unwrap().bytes, PNG);
}

#[test]
fn a_directory_is_refused() {
    let root = scratch("dir");
    let sub = root.join("cover.png");
    std::fs::create_dir(&sub).unwrap();
    let got = cover(&file_url(&sub), &places(&[&root], &[]));
    std::fs::remove_dir_all(&root).unwrap();
    assert!(got.is_none());
}

#[test]
fn a_file_over_4_mib_is_refused() {
    let dir = scratch("big");
    let path = dir.join("cover.png");
    let mut big = PNG.to_vec();
    big.resize(MAX_COVER_BYTES as usize + 1, 0);
    std::fs::write(&path, &big).unwrap();
    let over = cover(&file_url(&path), &places(&[&dir], &[]));
    let mut exact = PNG.to_vec();
    exact.resize(MAX_COVER_BYTES as usize, 0);
    std::fs::write(&path, &exact).unwrap();
    let at_the_cap = cover(&file_url(&path), &places(&[&dir], &[]));
    std::fs::remove_dir_all(&dir).unwrap();
    assert!(over.is_none());
    assert_eq!(at_the_cap.unwrap().bytes.len() as u64, MAX_COVER_BYTES);
}

#[test]
fn a_non_image_is_refused() {
    let dir = scratch("text");
    let path = dir.join("cover.png");
    std::fs::write(&path, b"#!/bin/sh\necho not a picture\n").unwrap();
    let got = cover(&file_url(&path), &places(&[&dir], &[]));
    std::fs::remove_dir_all(&dir).unwrap();
    assert!(got.is_none());
}

#[test]
fn a_missing_file_is_none() {
    let dir = scratch("missing");
    let got = cover(&file_url(&dir.join("nope.png")), &places(&[&dir], &[]));
    std::fs::remove_dir_all(&dir).unwrap();
    assert!(got.is_none());
}

/// `cover` has no client and no socket to reach with: it takes an address and the places on
/// disk, and an address that is not a file or Spotify's picture is simply no cover.
#[test]
fn an_http_address_is_none_and_nothing_is_fetched() {
    let none = CoverPlaces::default();
    for url in [
        "http://127.0.0.1:1/cover.jpg",
        "https://example.org/cover.jpg",
        "https://i.scdn.co.example.org/image/abc",
        "http://i.scdn.co/image/ab67616d0000b273dabb260cf41a2fbb2c842787",
        "https://i.scdn.co/image/",
        "https://i.scdn.co/image/../etc/passwd",
        "https://i.scdn.co/image/abc?x=1",
        "ftp://example.org/cover.png",
    ] {
        assert!(cover(url, &none).is_none(), "{url}");
    }
}

#[test]
fn spotify_cache_name_matches_the_verified_example() {
    assert_eq!(spotify_cache_name(ART), "4fdb4d4eaadd0d7a");
}

#[test]
fn spotify_cache_names_are_16_lowercase_hex_characters() {
    for id in ["a", "b", "c", "ab67616d0000b273000000000000000000000000"] {
        let name = spotify_cache_name(&format!("https://i.scdn.co/image/{id}"));
        assert_eq!(name.len(), 16, "{name}");
        assert!(
            name.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        );
    }
}

#[test]
fn a_spotify_cover_is_read_from_a_simple_cache_file() {
    let cache = scratch("spotify");
    let file = cache.join(format!("{}_0", spotify_cache_name(ART)));
    std::fs::write(&file, cache_file(&spotify_key(ART), PNG)).unwrap();
    let got = cover(ART, &places(&[], &[&cache]));
    std::fs::remove_dir_all(&cache).unwrap();
    let got = got.expect("a cover");
    assert_eq!(got.mime, "image/png");
    assert_eq!(got.bytes, PNG);
}

#[test]
fn the_first_cache_root_with_the_cover_wins_and_a_missing_root_is_skipped() {
    let empty = scratch("spotify-empty");
    let cache = scratch("spotify-second");
    let gone = empty.join("not-there");
    std::fs::write(
        cache.join(format!("{}_0", spotify_cache_name(ART))),
        cache_file(&spotify_key(ART), JPEG),
    )
    .unwrap();
    let got = cover(ART, &places(&[], &[&gone, &empty, &cache]));
    std::fs::remove_dir_all(&empty).unwrap();
    std::fs::remove_dir_all(&cache).unwrap();
    assert_eq!(got.unwrap().mime, "image/jpeg");
}

/// An id that is more than letters and digits (a path, an escape) never reaches the cache,
/// even where a well-formed entry sits under the name its address would give.
#[test]
fn a_spotify_id_that_is_not_plain_never_reaches_the_cache() {
    let cache = scratch("spotify-id");
    for url in [
        "https://i.scdn.co/image/abc/..",
        "https://i.scdn.co/image/abc%2F..%2Fx",
        "https://i.scdn.co/image/ab-c",
    ] {
        let file = cache.join(format!("{}_0", spotify_cache_name(url)));
        std::fs::write(&file, cache_file(&spotify_key(url), PNG)).unwrap();
        assert!(cover(url, &places(&[], &[&cache])).is_none(), "{url}");
    }
    // The same cache does give a plain id its picture.
    let plain = "https://i.scdn.co/image/abc";
    let file = cache.join(format!("{}_0", spotify_cache_name(plain)));
    std::fs::write(&file, cache_file(&spotify_key(plain), PNG)).unwrap();
    let got = cover(plain, &places(&[], &[&cache]));
    std::fs::remove_dir_all(&cache).unwrap();
    assert!(got.is_some());
}

/// The file that opens is checked, not the path it was looked at as: a link out of the roots,
/// a directory and a missing file open nothing, a file inside does.
#[test]
fn a_file_is_opened_only_when_it_sits_inside_the_roots() {
    let root = scratch("open-root");
    let outside = scratch("open-outside");
    std::fs::write(root.join("in.png"), PNG).unwrap();
    std::fs::write(outside.join("out.png"), PNG).unwrap();
    std::os::unix::fs::symlink(outside.join("out.png"), root.join("link.png")).unwrap();
    std::fs::create_dir(root.join("dir")).unwrap();
    let roots = [root.clone()];
    assert!(open_inside(&root.join("in.png"), &roots).is_some());
    assert!(open_inside(&root.join("link.png"), &roots).is_none());
    assert!(open_inside(&root.join("dir"), &roots).is_none());
    assert!(open_inside(&root.join("gone.png"), &roots).is_none());
    assert!(open_inside(&outside.join("out.png"), &roots).is_none());
    std::fs::remove_dir_all(&root).unwrap();
    std::fs::remove_dir_all(&outside).unwrap();
}

#[test]
fn a_key_mismatch_is_refused() {
    let cache = scratch("spotify-key");
    let other = "https://i.scdn.co/image/ab67616d0000b273ffffffffffffffffffffffffffffffff";
    let file = cache.join(format!("{}_0", spotify_cache_name(ART)));
    std::fs::write(&file, cache_file(&spotify_key(other), PNG)).unwrap();
    let got = cover(ART, &places(&[], &[&cache]));
    std::fs::remove_dir_all(&cache).unwrap();
    assert!(got.is_none());
}

#[test]
fn a_cache_body_that_is_not_an_image_is_refused() {
    let cache = scratch("spotify-body");
    let file = cache.join(format!("{}_0", spotify_cache_name(ART)));
    std::fs::write(
        &file,
        cache_file(&spotify_key(ART), b"<html>not a picture</html>"),
    )
    .unwrap();
    let got = cover(ART, &places(&[], &[&cache]));
    std::fs::remove_dir_all(&cache).unwrap();
    assert!(got.is_none());
}

#[test]
fn a_cache_file_with_a_lying_key_length_or_no_end_is_refused() {
    let cache = scratch("spotify-odd");
    let file = cache.join(format!("{}_0", spotify_cache_name(ART)));
    let key = spotify_key(ART);

    let mut huge_key = cache_file(&key, PNG);
    huge_key[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
    std::fs::write(&file, &huge_key).unwrap();
    let huge = cover(ART, &places(&[], &[&cache]));

    let mut no_end = cache_file(&key, PNG);
    let end = no_end.len() - EOF_MAGIC.len() - 30;
    no_end.truncate(end);
    std::fs::write(&file, &no_end).unwrap();
    let unended = cover(ART, &places(&[], &[&cache]));

    std::fs::write(&file, [0u8; 10]).unwrap();
    let short = cover(ART, &places(&[], &[&cache]));

    std::fs::remove_dir_all(&cache).unwrap();
    assert!(huge.is_none() && unended.is_none() && short.is_none());
}

#[test]
fn a_cache_body_over_4_mib_is_refused() {
    let cache = scratch("spotify-big");
    let file = cache.join(format!("{}_0", spotify_cache_name(ART)));
    let mut body = PNG.to_vec();
    body.resize(MAX_COVER_BYTES as usize + 1, 0);
    std::fs::write(&file, cache_file(&spotify_key(ART), &body)).unwrap();
    let got = cover(ART, &places(&[], &[&cache]));
    std::fs::remove_dir_all(&cache).unwrap();
    assert!(got.is_none());
}

#[test]
fn the_places_come_from_the_environment_s_directories() {
    let got = places_from(
        Some(OsStr::new("/home/ann")),
        Some(OsStr::new("/run/user/1000")),
        None,
    );
    assert_eq!(
        got.roots,
        [
            PathBuf::from("/home/ann"),
            PathBuf::from("/tmp"),
            PathBuf::from("/run/user/1000")
        ]
    );
    let cache = "Browser/Cache/Cache_Data";
    assert_eq!(
        got.spotify,
        [
            PathBuf::from(format!(
                "/home/ann/snap/spotify/common/.cache/spotify/{cache}"
            )),
            PathBuf::from(format!("/home/ann/.cache/spotify/{cache}")),
            PathBuf::from(format!(
                "/home/ann/.var/app/com.spotify.Client/cache/spotify/{cache}"
            )),
        ]
    );
}

#[test]
fn unset_empty_or_relative_directories_are_left_out() {
    let none = places_from(None, None, None);
    assert_eq!(none.roots, [PathBuf::from("/tmp")]);
    assert!(none.spotify.is_empty());
    let odd = places_from(
        Some(OsStr::new("")),
        Some(OsStr::new("run")),
        Some(OsStr::new("pw")),
    );
    assert_eq!(odd.roots, [PathBuf::from("/tmp")]);
    assert!(odd.spotify.is_empty());
}

/// A demo with the host on keeps the real runtime directory in `PIPEWIRE_RUNTIME_DIR` while its
/// own is private: both are roots, once each.
#[test]
fn the_pipewire_runtime_directory_is_a_root_too() {
    let got = places_from(
        Some(OsStr::new("/home/ann")),
        Some(OsStr::new("/tmp/sd-abcdef")),
        Some(OsStr::new("/run/user/1000")),
    );
    assert_eq!(
        got.roots,
        [
            PathBuf::from("/home/ann"),
            PathBuf::from("/tmp"),
            PathBuf::from("/tmp/sd-abcdef"),
            PathBuf::from("/run/user/1000")
        ]
    );
    let same = places_from(
        None,
        Some(OsStr::new("/run/user/1000")),
        Some(OsStr::new("/run/user/1000")),
    );
    assert_eq!(
        same.roots,
        [PathBuf::from("/tmp"), PathBuf::from("/run/user/1000")]
    );
}
