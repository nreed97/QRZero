//! ADIF band table and frequency-to-band mapping.

/// (band name, lower edge MHz, upper edge MHz), per the ADIF Band enumeration.
pub const BANDS: &[(&str, f64, f64)] = &[
    ("2190m", 0.1357, 0.1378),
    ("630m", 0.472, 0.479),
    ("560m", 0.501, 0.504),
    ("160m", 1.8, 2.0),
    ("80m", 3.5, 4.0),
    ("60m", 5.06, 5.45),
    ("40m", 7.0, 7.3),
    ("30m", 10.1, 10.15),
    ("20m", 14.0, 14.35),
    ("17m", 18.068, 18.168),
    ("15m", 21.0, 21.45),
    ("12m", 24.890, 24.99),
    ("10m", 28.0, 29.7),
    ("8m", 40.0, 45.0),
    ("6m", 50.0, 54.0),
    ("5m", 54.000001, 69.9),
    ("4m", 70.0, 71.0),
    ("2m", 144.0, 148.0),
    ("1.25m", 222.0, 225.0),
    ("70cm", 420.0, 450.0),
    ("33cm", 902.0, 928.0),
    ("23cm", 1240.0, 1300.0),
    ("13cm", 2300.0, 2450.0),
    ("9cm", 3300.0, 3500.0),
    ("6cm", 5650.0, 5925.0),
    ("3cm", 10000.0, 10500.0),
    ("1.25cm", 24000.0, 24250.0),
    ("6mm", 47000.0, 47200.0),
    ("4mm", 75500.0, 81000.0),
    ("2.5mm", 119980.0, 123000.0),
    ("2mm", 134000.0, 149000.0),
    ("1mm", 241000.0, 250000.0),
    ("submm", 300000.0, 7500000.0),
];

/// Returns the ADIF band for a frequency in MHz, if it falls inside an amateur band.
pub fn band_for_freq(mhz: f64) -> Option<&'static str> {
    BANDS
        .iter()
        .find(|(_, lo, hi)| mhz >= *lo - 1e-9 && mhz <= *hi + 1e-9)
        .map(|(name, _, _)| *name)
}

/// Normalizes a band string to ADIF form (lowercase, known band), or None if unknown.
pub fn normalize_band(band: &str) -> Option<&'static str> {
    let b = band.trim().to_ascii_lowercase();
    BANDS.iter().find(|(name, _, _)| *name == b).map(|(n, _, _)| *n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_common_frequencies() {
        assert_eq!(band_for_freq(14.025), Some("20m"));
        assert_eq!(band_for_freq(7.074), Some("40m"));
        assert_eq!(band_for_freq(1.8), Some("160m"));
        assert_eq!(band_for_freq(432.1), Some("70cm"));
        assert_eq!(band_for_freq(13.0), None);
        assert_eq!(normalize_band("20M"), Some("20m"));
        assert_eq!(normalize_band("70CM"), Some("70cm"));
    }
}
