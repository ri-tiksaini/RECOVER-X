use crate::core::endian::read_le_u32;
use crate::core::types::ByteOffset;
use crate::error::RxResult;
use crate::fs::fat::bpb::Fat32Bpb;
use crate::platform::raw_reader::{read_logical_slice, RawReader};

pub fn read_fat32_entry(reader: &dyn RawReader, bpb: &Fat32Bpb, cluster: u32) -> RxResult<u32> {
    let off = bpb.fat_start_offset + (cluster as u64) * 4;
    let raw = read_logical_slice(reader, ByteOffset(off), 4)?;
    Ok(read_le_u32(&raw, 0)? & 0x0FFF_FFFF)
}

/// Walks an active FAT32 cluster chain (capped at max_clusters to avoid loops on corrupt FAT).
pub fn follow_chain(reader: &dyn RawReader, bpb: &Fat32Bpb, start: u32, max_clusters: usize) -> Vec<u32> {
    let mut chain = Vec::new();
    let mut cur = start;
    while cur >= 2 && cur < 0x0FFF_FFF8 && chain.len() < max_clusters {
        chain.push(cur);
        match read_fat32_entry(reader, bpb, cur) {
            Ok(next) if next != cur => cur = next,
            _ => break,
        }
    }
    chain
}

/// Computes overlap ratio for a deleted contiguous cluster run (FAT entry != 0 means reused).
pub fn contiguous_overlap_ratio(reader: &dyn RawReader, bpb: &Fat32Bpb, start: u32, count: u64) -> f64 {
    if count == 0 {
        return 0.0;
    }
    let sample = count.min(512);
    let mut used = 0u64;
    for i in 0..sample {
        if let Ok(val) = read_fat32_entry(reader, bpb, start.saturating_add(i as u32)) {
            if val != 0 {
                used += 1;
            }
        }
    }
    (used as f64) / (sample as f64)
}
