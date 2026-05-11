use num_format::{Locale, ToFormattedString};
use std::sync::OnceLock;

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum Units {
    /// Binary divisions (1024), labeled with SI suffixes (KB/MB/GB).
    /// Imprecise but matches what most users expect from disk tools.
    #[default]
    Default,
    /// Binary divisions (1024), labeled correctly as KiB/MiB/GiB.
    Iec,
    /// Decimal divisions (1000), labeled KB/MB/GB.
    Si,
}

static UNITS: OnceLock<Units> = OnceLock::new();

pub fn set_units(u: Units) {
    let _ = UNITS.set(u);
}

pub fn bytes(b: u64) -> String {
    bytes_with(b, *UNITS.get().unwrap_or(&Units::Default))
}

pub fn bytes_with(b: u64, units: Units) -> String {
    let (k, suffix_k, suffix_m, suffix_g, suffix_t) = match units {
        Units::Default => (1024_u64, "KB", "MB", "GB", "TB"),
        Units::Iec => (1024_u64, "KiB", "MiB", "GiB", "TiB"),
        Units::Si => (1000_u64, "KB", "MB", "GB", "TB"),
    };
    let m = k * k;
    let g = m * k;
    let t = g * k;
    if b < k {
        format!("{} B", b)
    } else if b < m {
        format!("{:.1} {}", b as f64 / k as f64, suffix_k)
    } else if b < g {
        format!("{:.1} {}", b as f64 / m as f64, suffix_m)
    } else if b < t {
        format!("{:.2} {}", b as f64 / g as f64, suffix_g)
    } else {
        format!("{:.2} {}", b as f64 / t as f64, suffix_t)
    }
}

pub fn count(n: usize) -> String {
    n.to_formatted_string(&Locale::en)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_units_preserve_legacy_output() {
        assert_eq!(bytes(0), "0 B");
        assert_eq!(bytes(512), "512 B");
        assert_eq!(bytes(1024), "1.0 KB");
        assert_eq!(bytes(1024 * 1024), "1.0 MB");
        assert_eq!(bytes(1024_u64.pow(3)), "1.00 GB");
        assert_eq!(bytes(1024_u64.pow(4)), "1.00 TB");
    }

    #[test]
    fn iec_units_use_binary_labels() {
        assert_eq!(bytes_with(1024, Units::Iec), "1.0 KiB");
        assert_eq!(bytes_with(1024 * 1024, Units::Iec), "1.0 MiB");
    }

    #[test]
    fn si_units_use_decimal_powers() {
        assert_eq!(bytes_with(1_000, Units::Si), "1.0 KB");
        assert_eq!(bytes_with(1_000_000, Units::Si), "1.0 MB");
    }

    #[test]
    fn counts_use_thousand_separators() {
        assert_eq!(count(0), "0");
        assert_eq!(count(999), "999");
        assert_eq!(count(1_234), "1,234");
        assert_eq!(count(1_234_567), "1,234,567");
    }
}
