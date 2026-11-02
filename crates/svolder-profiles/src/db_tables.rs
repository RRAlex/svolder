// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Numeric tables: the value in dB each raw wire value stands for, indexed by that raw value.

/// Attenuation in dB for the 128-step digital volume of the DACs, expressed as gain: index 0
/// is silence, `-inf`.
pub static DAC_ATTENUATION_DB: [f64; 128] = [
    f64::NEG_INFINITY,
    -96.33,
    -93.319,
    -90.309,
    -87.299,
    -84.288,
    -81.278,
    -78.268,
    -76.261,
    -74.254,
    -72.247,
    -70.24,
    -68.233,
    -66.227,
    -64.22,
    -62.213,
    -60.206,
    -58.199,
    -57.196,
    -56.192,
    -55.189,
    -54.185,
    -53.182,
    -52.179,
    -51.175,
    -50.172,
    -49.168,
    -48.165,
    -47.412,
    -46.66,
    -45.907,
    -45.154,
    -44.402,
    -43.649,
    -42.897,
    -42.144,
    -41.392,
    -40.639,
    -39.886,
    -39.134,
    -38.381,
    -37.629,
    -36.876,
    -36.124,
    -35.622,
    -35.12,
    -34.618,
    -34.117,
    -33.615,
    -33.113,
    -32.612,
    -32.11,
    -31.608,
    -31.106,
    -30.605,
    -30.103,
    -29.601,
    -29.1,
    -28.598,
    -28.096,
    -27.594,
    -27.093,
    -26.591,
    -26.089,
    -25.588,
    -25.086,
    -24.584,
    -24.082,
    -23.581,
    -23.079,
    -22.577,
    -22.076,
    -21.574,
    -21.072,
    -20.57,
    -20.069,
    -19.567,
    -19.065,
    -18.564,
    -18.062,
    -17.56,
    -17.058,
    -16.557,
    -16.055,
    -15.553,
    -15.051,
    -14.55,
    -14.048,
    -13.546,
    -13.045,
    -12.543,
    -12.041,
    -11.707,
    -11.372,
    -11.038,
    -10.703,
    -10.369,
    -10.034,
    -9.7,
    -9.365,
    -9.031,
    -8.696,
    -8.362,
    -8.027,
    -7.693,
    -7.359,
    -7.024,
    -6.69,
    -6.355,
    -6.021,
    -5.686,
    -5.352,
    -5.017,
    -4.683,
    -4.348,
    -4.014,
    -3.679,
    -3.345,
    -3.01,
    -2.676,
    -2.341,
    -2.007,
    -1.672,
    -1.338,
    -1.003,
    -0.669,
    -0.334,
    0.0,
];

/// Look a table up by the name used in profile files.
pub fn table(name: &str) -> Option<&'static [f64]> {
    match name {
        "dac-attenuation-db" => Some(&DAC_ATTENUATION_DB),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dac_table_shape() {
        let t = table("dac-attenuation-db").unwrap();
        assert_eq!(t.len(), 128);
        assert_eq!(t[0], f64::NEG_INFINITY);
        assert_eq!(t[1], -96.33);
        assert_eq!(t[109], -6.021);
        assert_eq!(t[127], 0.0);
        assert!(t.windows(2).skip(1).all(|w| w[0] < w[1]), "monotonic");
    }

    #[test]
    fn unknown_table_is_none() {
        assert!(table("nope").is_none());
    }
}
