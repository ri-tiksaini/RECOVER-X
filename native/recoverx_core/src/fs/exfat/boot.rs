use crate::core::endian::read_le_u32;
use crate::core::types::ByteOffset;
use crate::error::{RxError, RxResult};
use crate::platform::raw_reader::{read_logical_slice, RawReader};

#[derive(Debug, Clone)]
pub struct ExFatBoot {
    pub bytes_per_sector: u32,
    pub bytes_per_cluster: u64,
    pub cluster_heap_offset: u64,
    pub cluster_count: u32,
    pub root_dir_cluster: u32,
}

impl ExFatBoot {
    pub fn parse(reader: &dyn RawReader) -> RxResult<Self> {
        let buf = read_logical_slice(reader, ByteOffset(0), 512)?;
        if buf.len() < 112 || &buf[3..11] != b"EXFAT   " {
            return Err(RxError::FsUnknown("not an exFAT volume".into()));
        }
        let bps_shift = buf[108] as u32;
        let spc_shift = buf[109] as u32;
        if !(9..=12).contains(&bps_shift) || bps_shift + spc_shift > 25 {
            return Err(RxError::FsCorrupt("invalid exFAT shift parameters".into()));
        }
        let bytes_per_sector = 1u32 << bps_shift;
        let bytes_per_cluster = 1u64 << (bps_shift + spc_shift);
        let heap_sec = read_le_u32(&buf, 88)? as u64;
        let cluster_count = read_le_u32(&buf, 92)?;
        let root_dir_cluster = read_le_u32(&buf, 96)?;
        Ok(Self {
            bytes_per_sector,
            bytes_per_cluster,
            cluster_heap_offset: heap_sec * (bytes_per_sector as u64),
            cluster_count,
            root_dir_cluster,
        })
    }

    pub fn cluster_to_offset(&self, cluster: u32) -> Option<u64> {
        if cluster < 2 {
            return None;
        }
        Some(self.cluster_heap_offset + ((cluster as u64) - 2) * self.bytes_per_cluster)
    }
}
