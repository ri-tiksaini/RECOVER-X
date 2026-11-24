#[inline]
pub fn align_down(off: u64, sector: u32) -> u64 {
    let s = sector as u64;
    (off / s) * s
}

#[inline]
pub fn align_up(len: u64, sector: u32) -> u64 {
    let s = sector as u64;
    if len == 0 {
        return 0;
    }
    ((len - 1) / s + 1) * s
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AlignedWindow {
    pub aligned_off: u64,
    pub aligned_len: usize,
    pub lead_skip: usize,
    pub tail_drop: usize,
}

/// Computes sector-aligned read bounds for an arbitrary logical `(off, len)` request [P-4].
pub fn split_unaligned(off: u64, len: usize, sector: u32) -> AlignedWindow {
    let aligned_off = align_down(off, sector);
    let lead_skip = (off - aligned_off) as usize;
    let total = lead_skip + len;
    let aligned_len = align_up(total as u64, sector) as usize;
    let tail_drop = aligned_len - total;
    AlignedWindow {
        aligned_off,
        aligned_len,
        lead_skip,
        tail_drop,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_reconstructs_exact_logical_slice() {
        let data: Vec<u8> = (0..16384u32).map(|i| (i % 251) as u8).collect();
        for &(off, len, sector) in &[
            (0u64, 512usize, 512u32),
            (13, 1000, 512),
            (511, 2, 512),
            (4095, 5000, 4096),
        ] {
            let w = split_unaligned(off, len, sector);
            assert_eq!(w.aligned_off % sector as u64, 0);
            assert_eq!(w.aligned_len % sector as usize, 0);
            let raw = &data[w.aligned_off as usize..w.aligned_off as usize + w.aligned_len];
            let logical = &raw[w.lead_skip..w.aligned_len - w.tail_drop];
            assert_eq!(logical, &data[off as usize..off as usize + len]);
        }
    }
}
