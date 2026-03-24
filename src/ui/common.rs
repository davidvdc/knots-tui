pub fn format_btc(satoshis: u64) -> String {
    let btc = satoshis as f64 / 100_000_000.0;
    if btc >= 1000.0 {
        format!("{:.0} BTC", btc)
    } else {
        format!("{:.2} BTC", btc)
    }
}

pub fn format_btc_fees(satoshis: u64) -> String {
    let btc = satoshis as f64 / 100_000_000.0;
    format!("{:.3} BTC", btc)
}

pub fn format_number(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}

pub fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    const TB: f64 = GB * 1024.0;

    let b = bytes as f64;
    if b >= TB {
        format!("{:.2} TB", b / TB)
    } else if b >= GB {
        format!("{:.2} GB", b / GB)
    } else if b >= MB {
        format!("{:.2} MB", b / MB)
    } else if b >= KB {
        format!("{:.1} KB", b / KB)
    } else {
        format!("{} B", bytes)
    }
}

/// Fixed-width byte formatting: always 5 chars (`XXXXY` where Y is unit).
/// Number portion right-justified in 4 chars, 1 decimal when <10 of unit.
pub fn format_bytes_short(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;

    let b = bytes as f64;
    let (n, u) = if b >= GB {
        let g = b / GB;
        (if g >= 100.0 { format!("{:.0}", g) } else { format!("{:.1}", g) }, "G")
    } else if b >= MB {
        let m = b / MB;
        (if m >= 100.0 { format!("{:.0}", m) } else { format!("{:.1}", m) }, "M")
    } else if b >= KB {
        (format!("{:.0}", b / KB), "K")
    } else {
        (format!("{}", bytes), "B")
    };
    format!("{:>4}{}", n, u)
}

pub fn format_duration(seconds: u64) -> String {
    let days = seconds / 86400;
    let hours = (seconds % 86400) / 3600;
    let mins = (seconds % 3600) / 60;

    if days > 0 {
        format!("{}d {}h {}m", days, hours, mins)
    } else if hours > 0 {
        format!("{}h {}m", hours, mins)
    } else {
        format!("{}m", mins)
    }
}

pub fn format_hashrate(hps: f64) -> String {
    if hps >= 1e18 {
        format!("~{:.2} EH/s", hps / 1e18)
    } else if hps >= 1e15 {
        format!("~{:.2} PH/s", hps / 1e15)
    } else if hps >= 1e12 {
        format!("~{:.2} TH/s", hps / 1e12)
    } else if hps >= 1e9 {
        format!("~{:.2} GH/s", hps / 1e9)
    } else {
        format!("~{:.2} H/s", hps)
    }
}

pub fn format_compact(n: u64) -> String {
    if n < 1_000 {
        format!("{}", n)
    } else if n < 10_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else if n < 1_000_000 {
        format!("{}k", n / 1_000)
    } else if n < 10_000_000 {
        format!("{:.1}m", n as f64 / 1_000_000.0)
    } else {
        format!("{}m", n / 1_000_000)
    }
}

pub fn pct_str(n: u64, total: u64) -> String {
    if total > 0 { format!("{:.1}", n as f64 / total as f64 * 100.0) } else { "0.0".into() }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- format_compact ---

    #[test]
    fn compact_below_1000() {
        assert_eq!(format_compact(0), "0");
        assert_eq!(format_compact(999), "999");
    }

    #[test]
    fn compact_at_1000() {
        assert_eq!(format_compact(1000), "1.0k");
    }

    #[test]
    fn compact_below_10000() {
        assert_eq!(format_compact(1400), "1.4k");
        assert_eq!(format_compact(9999), "10.0k");
    }

    #[test]
    fn compact_at_10000() {
        assert_eq!(format_compact(10000), "10k");
        assert_eq!(format_compact(52000), "52k");
        assert_eq!(format_compact(999999), "999k");
    }

    #[test]
    fn compact_at_million() {
        assert_eq!(format_compact(1000000), "1.0m");
        assert_eq!(format_compact(1200000), "1.2m");
        assert_eq!(format_compact(9999999), "10.0m");
    }

    #[test]
    fn compact_above_10m() {
        assert_eq!(format_compact(10000000), "10m");
        assert_eq!(format_compact(50000000), "50m");
    }

    // --- format_btc ---

    #[test]
    fn btc_zero() {
        assert_eq!(format_btc(0), "0.00 BTC");
    }

    #[test]
    fn btc_one() {
        assert_eq!(format_btc(100_000_000), "1.00 BTC");
    }

    #[test]
    fn btc_above_1000() {
        assert_eq!(format_btc(100_000_000_000), "1000 BTC");
    }

    // --- format_btc_fees ---

    #[test]
    fn btc_fees_zero() {
        assert_eq!(format_btc_fees(0), "0.000 BTC");
    }

    #[test]
    fn btc_fees_typical() {
        assert_eq!(format_btc_fees(12345678), "0.123 BTC");
    }

    // --- format_number ---

    #[test]
    fn number_zero() {
        assert_eq!(format_number(0), "0");
    }

    #[test]
    fn number_below_1000() {
        assert_eq!(format_number(999), "999");
    }

    #[test]
    fn number_at_1000() {
        assert_eq!(format_number(1000), "1,000");
    }

    #[test]
    fn number_million() {
        assert_eq!(format_number(1000000), "1,000,000");
    }

    // --- format_bytes ---

    #[test]
    fn bytes_zero() {
        assert_eq!(format_bytes(0), "0 B");
    }

    #[test]
    fn bytes_kb() {
        assert_eq!(format_bytes(1024), "1.0 KB");
    }

    #[test]
    fn bytes_mb() {
        assert_eq!(format_bytes(1024 * 1024), "1.00 MB");
    }

    #[test]
    fn bytes_gb() {
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.00 GB");
    }

    #[test]
    fn bytes_tb() {
        assert_eq!(format_bytes(1024u64 * 1024 * 1024 * 1024), "1.00 TB");
    }

    // --- format_bytes_short ---

    #[test]
    fn bytes_short_zero() {
        assert_eq!(format_bytes_short(0), "   0B");
    }

    #[test]
    fn bytes_short_bytes() {
        assert_eq!(format_bytes_short(512), " 512B");
    }

    #[test]
    fn bytes_short_kb() {
        assert_eq!(format_bytes_short(1024), "   1K");
        assert_eq!(format_bytes_short(10 * 1024), "  10K");
        assert_eq!(format_bytes_short(100 * 1024), " 100K");
    }

    #[test]
    fn bytes_short_mb() {
        assert_eq!(format_bytes_short(1024 * 1024), " 1.0M");
        assert_eq!(format_bytes_short(10 * 1024 * 1024), "10.0M");
        assert_eq!(format_bytes_short(100 * 1024 * 1024), " 100M");
    }

    #[test]
    fn bytes_short_gb() {
        assert_eq!(format_bytes_short(1024 * 1024 * 1024), " 1.0G");
        assert_eq!(format_bytes_short(10 * 1024 * 1024 * 1024), "10.0G");
    }

    #[test]
    fn bytes_short_always_5_chars() {
        for &v in &[0, 512, 1024, 10240, 102400, 1048576, 10485760, 104857600, 1073741824] {
            assert_eq!(format_bytes_short(v).len(), 5, "failed for {}", v);
        }
    }

    // --- format_duration ---

    #[test]
    fn duration_zero() {
        assert_eq!(format_duration(0), "0m");
    }

    #[test]
    fn duration_minutes() {
        assert_eq!(format_duration(300), "5m");
    }

    #[test]
    fn duration_hours() {
        assert_eq!(format_duration(3661), "1h 1m");
    }

    #[test]
    fn duration_days() {
        assert_eq!(format_duration(90061), "1d 1h 1m");
    }

    // --- format_hashrate ---

    #[test]
    fn hashrate_gh() {
        assert_eq!(format_hashrate(1e9), "~1.00 GH/s");
    }

    #[test]
    fn hashrate_th() {
        assert_eq!(format_hashrate(1e12), "~1.00 TH/s");
    }

    #[test]
    fn hashrate_ph() {
        assert_eq!(format_hashrate(1e15), "~1.00 PH/s");
    }

    #[test]
    fn hashrate_eh() {
        assert_eq!(format_hashrate(1e18), "~1.00 EH/s");
    }

    #[test]
    fn hashrate_low() {
        assert_eq!(format_hashrate(1000.0), "~1000.00 H/s");
    }
}
