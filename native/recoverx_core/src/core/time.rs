/// 100-ns intervals between 1601-01-01 and 1970-01-01.
const FILETIME_UNIX_EPOCH_TICKS: i128 = 116_444_736_000_000_000;

/// Convert Windows NTFS FILETIME (100-ns ticks since 1601-01-01 UTC) to UTC nanoseconds since Unix epoch.
#[inline]
pub fn filetime_to_utc_ns(ft: u64) -> i64 {
    let ticks = (ft as i128) - FILETIME_UNIX_EPOCH_TICKS;
    let ns = ticks.saturating_mul(100);
    ns.clamp(i64::MIN as i128, i64::MAX as i128) as i64
}

/// Convert ext2/3/4 (seconds, nanoseconds) since 1970-01-01 UTC to UTC nanoseconds.
#[inline]
pub fn ext4_to_utc_ns(sec: i64, nsec: u32) -> i64 {
    (sec as i128)
        .saturating_mul(1_000_000_000)
        .saturating_add(nsec as i128)
        .clamp(i64::MIN as i128, i64::MAX as i128) as i64
}

/// Convert FAT date/time bitfields (+ 10ms tenths) to UTC nanoseconds since Unix epoch.
pub fn fat_datetime_to_utc_ns(date: u16, time: u16, tenths_10ms: u8) -> i64 {
    let year = 1980 + ((date >> 9) & 0x7F) as i64;
    let month = ((date >> 5) & 0x0F).clamp(1, 12) as i64;
    let day = (date & 0x1F).clamp(1, 31) as i64;
    let hour = ((time >> 11) & 0x1F).min(23) as i64;
    let min = ((time >> 5) & 0x3F).min(59) as i64;
    let sec = ((time & 0x1F) * 2).min(59) as i64;
    let days = days_from_civil(year, month, day);
    let total_secs = days * 86_400 + hour * 3600 + min * 60 + sec;
    total_secs
        .saturating_mul(1_000_000_000)
        .saturating_add((tenths_10ms as i64) * 10_000_000)
}

/// Howard Hinnant's civil-to-days algorithm (days since 1970-01-01).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = y - if m <= 2 { 1 } else { 0 };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_2020_vector() {
        // 2020-01-01T00:00:00Z = 1577836800 seconds
        let expected_ns: i64 = 1_577_836_800_000_000_000;
        assert_eq!(ext4_to_utc_ns(1_577_836_800, 0), expected_ns);
        let ft_2020: u64 = (116_444_736_000_000_000i128 + 1_577_836_800i128 * 10_000_000) as u64;
        assert_eq!(filetime_to_utc_ns(ft_2020), expected_ns);
        // FAT date: year=40 (2020-1980), month=1, day=1
        let fat_date: u16 = (40 << 9) | (1 << 5) | 1;
        assert_eq!(fat_datetime_to_utc_ns(fat_date, 0, 0), expected_ns);
    }
}
