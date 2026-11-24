use crate::core::types::ByteOffset;
use crate::fs::common::extent::{Extent, ExtentKind};
use crate::platform::raw_reader::{read_logical_slice, RawReader};

/// Measures fraction of 512-byte blocks (or whole slice if < 512 B) that are entirely 0x00 or 0xFF.
pub fn slice_zero_ratio(data: &[u8]) -> f64 {
    if data.is_empty() {
        return 0.0;
    }
    let chunk_sz = data.len().min(512);
    let mut total = 0usize;
    let mut wiped = 0usize;
    for blk in data.chunks(chunk_sz) {
        total += 1;
        if blk.iter().all(|&b| b == 0x00) || blk.iter().all(|&b| b == 0xFF) {
            wiped += 1;
        }
    }
    (wiped as f64) / (total as f64)
}

/// Samples head + tail of candidate extents within logical_size to detect TRIM/wipe [P-12].
pub fn region_zero_ratio(reader: &dyn RawReader, extents: &[Extent], logical_size: u64) -> f64 {
    let mut remaining = logical_size;
    let mut total_blocks = 0usize;
    let mut wiped_blocks = 0usize;
    for ext in extents.iter().take(16) {
        if remaining == 0 {
            break;
        }
        let eff_len = ext.len.min(remaining);
        remaining = remaining.saturating_sub(eff_len);
        if ext.kind != ExtentKind::Data || eff_len == 0 {
            continue;
        }
        let head_len = eff_len.min(4096) as usize;
        if let Ok(buf) = read_logical_slice(reader, ByteOffset(ext.physical_off), head_len) {
            let chunk_sz = buf.len().min(512).max(1);
            for blk in buf.chunks(chunk_sz) {
                total_blocks += 1;
                if blk.iter().all(|&b| b == 0x00) || blk.iter().all(|&b| b == 0xFF) {
                    wiped_blocks += 1;
                }
            }
        }
        if eff_len > 8192 {
            let tail_off = ext.physical_off + eff_len - 4096;
            if let Ok(buf) = read_logical_slice(reader, ByteOffset(tail_off), 4096) {
                for blk in buf.chunks(512) {
                    total_blocks += 1;
                    if blk.iter().all(|&b| b == 0x00) || blk.iter().all(|&b| b == 0xFF) {
                        wiped_blocks += 1;
                    }
                }
            }
        }
    }
    if total_blocks == 0 {
        0.0
    } else {
        (wiped_blocks as f64) / (total_blocks as f64)
    }
}
