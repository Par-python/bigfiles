pub fn bytes(b: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;
    const TB: u64 = 1024 * GB;
    if b < KB {
        format!("{} B", b)
    } else if b < MB {
        format!("{:.1} KB", b as f64 / KB as f64)
    } else if b < GB {
        format!("{:.1} MB", b as f64 / MB as f64)
    } else if b < TB {
        format!("{:.2} GB", b as f64 / GB as f64)
    } else {
        format!("{:.2} TB", b as f64 / TB as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scales_across_units() {
        assert_eq!(bytes(0), "0 B");
        assert_eq!(bytes(512), "512 B");
        assert_eq!(bytes(1024), "1.0 KB");
        assert_eq!(bytes(1024 * 1024), "1.0 MB");
        assert_eq!(bytes(1024 * 1024 * 1024), "1.00 GB");
        assert_eq!(bytes(1024_u64.pow(4)), "1.00 TB");
    }
}
