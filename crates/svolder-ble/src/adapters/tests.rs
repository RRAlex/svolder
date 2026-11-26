// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

fn adapter(name: &str, address: &str) -> AdapterInfo {
    AdapterInfo {
        name: name.to_string(),
        address: address.to_string(),
        usb: None,
    }
}

fn two() -> Vec<AdapterInfo> {
    vec![
        adapter("hci0", "AA:BB:CC:DD:EE:01"),
        adapter("hci2", "AA:BB:CC:DD:EE:02"),
    ]
}

#[test]
fn an_address_resolves_to_the_adapter_wearing_it_in_either_case() {
    assert_eq!(resolve("AA:BB:CC:DD:EE:02", &two()), Ok("hci2".to_string()));
    assert_eq!(resolve("aa:bb:cc:dd:ee:02", &two()), Ok("hci2".to_string()));
    assert_eq!(
        resolve(" AA:BB:CC:DD:EE:01 ", &two()),
        Ok("hci0".to_string())
    );
}

#[test]
fn a_name_resolves_as_it_always_did() {
    assert_eq!(resolve("hci0", &two()), Ok("hci0".to_string()));
    assert_eq!(resolve("hci2", &two()), Ok("hci2".to_string()));
}

#[test]
fn an_adapter_that_moved_to_another_number_still_resolves_by_its_address() {
    let before = two();
    let after = vec![
        adapter("hci1", "AA:BB:CC:DD:EE:02"),
        adapter("hci3", "AA:BB:CC:DD:EE:01"),
    ];
    assert_eq!(
        resolve("AA:BB:CC:DD:EE:02", &before),
        Ok("hci2".to_string())
    );
    assert_eq!(resolve("AA:BB:CC:DD:EE:02", &after), Ok("hci1".to_string()));
}

#[test]
fn an_unknown_address_is_an_error_naming_the_adapters_there_are() {
    let err = resolve("AA:BB:CC:DD:EE:FF", &two()).unwrap_err();
    assert_eq!(
        err,
        "no adapter has the address AA:BB:CC:DD:EE:FF; adapters: hci0 AA:BB:CC:DD:EE:01, hci2 AA:BB:CC:DD:EE:02"
    );
}

#[test]
fn an_unknown_name_is_an_error_naming_the_adapters_there_are() {
    let err = resolve("hci9", &two()).unwrap_err();
    assert_eq!(
        err,
        "no adapter named hci9; adapters: hci0 AA:BB:CC:DD:EE:01, hci2 AA:BB:CC:DD:EE:02"
    );
    assert_eq!(
        resolve("hci9", &[]).unwrap_err(),
        "no adapter named hci9; no adapters present"
    );
}

#[test]
fn only_six_hex_pairs_with_colons_are_an_address() {
    assert!(is_address("AA:BB:CC:DD:EE:01"));
    assert!(!is_address("hci0"));
    assert!(!is_address("AA:BB:CC:DD:EE"));
    assert!(!is_address("AA:BB:CC:DD:EE:GG"));
}

fn heard(unit: &str, rssi: Option<i16>) -> Heard {
    Heard {
        unit: unit.to_string(),
        rssi,
    }
}

fn hearing(a: AdapterInfo, units: &[(&str, Option<i16>)]) -> Hearing {
    Hearing {
        adapter: a,
        heard: Ok(units.iter().map(|(u, r)| heard(u, *r)).collect()),
    }
}

const UNIT: &str = "0A1B2C3D4E5F6071";

#[test]
fn the_pointer_names_the_louder_adapter_by_its_address() {
    let hearings = vec![
        hearing(two()[0].clone(), &[(UNIT, Some(-62))]),
        hearing(two()[1].clone(), &[(UNIT, Some(-74))]),
    ];
    assert_eq!(
        pointer("hci2", "BF3", UNIT, &hearings).as_deref(),
        Some(
            "hci2 hears BF3 at −74 dBm and cannot hold a link; hci0 hears it at −62 dBm: try adapter = \"AA:BB:CC:DD:EE:01\""
        )
    );
}

#[test]
fn the_pointer_says_so_when_the_failing_adapter_hears_nothing() {
    let hearings = vec![
        hearing(two()[0].clone(), &[(UNIT, Some(-62))]),
        hearing(two()[1].clone(), &[]),
    ];
    assert_eq!(
        pointer("hci2", "BF3", UNIT, &hearings).as_deref(),
        Some(
            "hci2 does not hear BF3; hci0 hears it at −62 dBm: try adapter = \"AA:BB:CC:DD:EE:01\""
        )
    );
}

#[test]
fn the_pointer_is_silent_when_no_other_adapter_hears_the_device() {
    let hearings = vec![
        hearing(two()[0].clone(), &[]),
        hearing(two()[1].clone(), &[(UNIT, Some(-74))]),
    ];
    assert_eq!(pointer("hci2", "BF3", UNIT, &hearings), None);
    let failed = vec![
        Hearing {
            adapter: two()[0].clone(),
            heard: Err("adapter unavailable".to_string()),
        },
        hearing(two()[1].clone(), &[(UNIT, Some(-74))]),
    ];
    assert_eq!(pointer("hci2", "BF3", UNIT, &failed), None);
}

#[test]
fn the_pointer_takes_the_loudest_of_several() {
    let three = vec![
        hearing(adapter("hci0", "00:00:00:00:00:01"), &[(UNIT, Some(-80))]),
        hearing(adapter("hci1", "00:00:00:00:00:02"), &[(UNIT, Some(-50))]),
        hearing(adapter("hci2", "00:00:00:00:00:03"), &[(UNIT, Some(-70))]),
    ];
    let said = pointer("hci2", "BF3", UNIT, &three).unwrap();
    assert!(said.contains("hci1 hears it at −50 dBm"), "{said}");
    assert!(said.contains("00:00:00:00:00:02"), "{said}");
}

#[test]
fn the_fake_adapters_follow_the_knob_and_default_to_one() {
    let units = vec![UNIT.to_string()];
    let one = fake_hearings(None, &units).unwrap();
    assert_eq!(one.len(), 1);
    assert_eq!(one[0].adapter.name, "hci0");
    assert_eq!(one[0].signal_of(UNIT), Some(Some(-60)));

    let both = fake_hearings(
        Some("hci0=AA:BB:CC:DD:EE:01@-62,hci2=AA:BB:CC:DD:EE:02@none"),
        &units,
    )
    .unwrap();
    assert_eq!(both[0].signal_of(UNIT), Some(Some(-62)));
    assert_eq!(both[1].signal_of(UNIT), None);
    assert!(fake_hearings(Some("hci0"), &units).is_err());
    assert!(fake_hearings(Some("hci0=nope@-60"), &units).is_err());
}

/// A USB dongle's sysfs node: the adapter's `device` link points at the USB interface, whose
/// parent is the USB device with the product string and the ids.
fn sysfs(product: Option<&str>) -> tempfile_dir::Dir {
    let dir = tempfile_dir::Dir::new();
    let usb = dir.path().join("devices/usb1/1-2");
    let interface = usb.join("1-2:1.0");
    std::fs::create_dir_all(&interface).unwrap();
    std::fs::write(usb.join("idVendor"), "2357\n").unwrap();
    std::fs::write(usb.join("idProduct"), "0604\n").unwrap();
    if let Some(product) = product {
        std::fs::write(usb.join("product"), format!("{product}\n")).unwrap();
    }
    let class = dir.path().join("class/bluetooth/hci0");
    std::fs::create_dir_all(&class).unwrap();
    std::os::unix::fs::symlink(&interface, class.join("device")).unwrap();
    dir
}

#[test]
fn the_usb_product_is_the_sysfs_string_with_the_ids() {
    let dir = sysfs(Some("UB500"));
    assert_eq!(
        usb_label(dir.path(), "hci0").as_deref(),
        Some("UB500 (2357:0604)")
    );
}

#[test]
fn without_a_product_string_the_ids_stand_alone() {
    let dir = sysfs(None);
    assert_eq!(usb_label(dir.path(), "hci0").as_deref(), Some("2357:0604"));
}

#[test]
fn an_adapter_that_is_not_usb_has_no_product() {
    let dir = tempfile_dir::Dir::new();
    let platform = dir.path().join("devices/platform/serial/hci0dev");
    std::fs::create_dir_all(&platform).unwrap();
    let class = dir.path().join("class/bluetooth/hci0");
    std::fs::create_dir_all(&class).unwrap();
    std::os::unix::fs::symlink(&platform, class.join("device")).unwrap();
    assert_eq!(usb_label(dir.path(), "hci0"), None);
    assert_eq!(usb_label(dir.path(), "hci7"), None);
}

/// A scratch directory removed when it goes out of scope.
mod tempfile_dir {
    use std::path::{Path, PathBuf};

    pub struct Dir(PathBuf);

    impl Dir {
        pub fn new() -> Dir {
            static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
            let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let root = std::env::var_os("TMPDIR").map_or_else(std::env::temp_dir, PathBuf::from);
            let path = root.join(format!("svolder-adapters-{}-{n}", std::process::id()));
            std::fs::create_dir_all(&path).unwrap();
            Dir(path)
        }

        pub fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

/// What is remembered for a device is the adapter's address; a name no adapter wears, and an
/// adapter BlueZ gave no address, are kept as they were.
#[test]
fn a_name_is_remembered_as_the_address_that_stays_with_the_dongle() {
    assert_eq!(stable_name("hci2", &two()), "AA:BB:CC:DD:EE:02");
    assert_eq!(
        stable_name("AA:BB:CC:DD:EE:01", &two()),
        "AA:BB:CC:DD:EE:01"
    );
    assert_eq!(stable_name("hci9", &two()), "hci9");
    assert_eq!(stable_name("hci0", &[adapter("hci0", "")]), "hci0");
}

/// The remembered address, then the dongles swapped to the other numbers: it resolves to the
/// number its dongle has now, which a remembered `hci2` would not.
#[test]
fn a_remembered_address_follows_its_dongle_to_the_new_number() {
    let remembered = stable_name("hci2", &two());
    let swapped = vec![
        adapter("hci0", "AA:BB:CC:DD:EE:02"),
        adapter("hci2", "AA:BB:CC:DD:EE:01"),
    ];
    assert_eq!(resolve(&remembered, &swapped), Ok("hci0".to_string()));
    assert_eq!(resolve("hci2", &swapped), Ok("hci2".to_string()));
}
