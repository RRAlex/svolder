// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Profiles embedded in the binary. Each entry is `(name, TOML text)`.

macro_rules! profile {
    ($name:literal) => {
        (
            $name,
            include_str!(concat!("../../../profiles/", $name, ".toml")),
        )
    };
}

/// Every profile compiled into the binary, in the order they are offered to a registry.
pub static BUILTIN: &[(&str, &str)] = &[
    profile!("dac-common"),
    profile!("mesh-eq-5"),
    profile!("mesh-eq-3"),
    profile!("gungnir2-v1"),
    profile!("gungnir2-v2"),
    profile!("yggdrasil"),
    profile!("bifrost3"),
    profile!("mimir"),
    profile!("meshcard"),
    profile!("eitr2"),
    profile!("preamp-common"),
    profile!("freya"),
    profile!("kara"),
    profile!("saga2"),
    profile!("lyr5"),
    profile!("amp-common"),
    profile!("amp-stereo"),
    profile!("amp-standby"),
    profile!("aegir2"),
    profile!("vidar2"),
    profile!("gjallarhorn"),
    profile!("kraken"),
    profile!("tyr"),
    profile!("wotan"),
    profile!("phono-common"),
    profile!("skoll"),
    profile!("stjarna"),
    profile!("lokimax"),
    profile!("freya2"),
    profile!("hati"),
    // Not a Schiit product: the amplifier the simulator and the in-process fake serve.
    profile!("fake-amp"),
    // Not a Schiit product: the same, with one meter, for a front end with one channel to draw.
    profile!("fake-mono"),
    // Not a Schiit product: the same, with five meters, for a front end with more than two.
    profile!("fake-five"),
];
