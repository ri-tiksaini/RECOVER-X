use crate::core::endian::{read_le_u16, read_le_u32};
use crate::core::types::ByteOffset;
use crate::error::{RxError, RxResult};
use crate::platform::raw_reader::{read_logical_slice, RawReader};

#[derive(Debug, Clone)]
pub struct Fat32Bpb {
    pub bytes_per_sector: u32,
    pub sectors_per_cluster: u32,
    pub bytes_per_cluster: u64,
    pub reserved_sectors: u64,
    pub num_fats: u64,
    pub fat_size_sectors: u64,
    pub root_cluster: u32,
    pub fat_start_offset: u64,
    pub data_start_offset: u64,
}

impl Fat32Bpb {
    pub fn parse(reader: &dyn RawReader) -> RxResult<Self> {
        let buf = read_logical_slice(reader, ByteOffset(0), 512)?;
        if buf.len() < 90 || &buf[82..90] != b"FAT32   " {
            return Err(RxError::FsUnknown("not a FAT32 volume".into()));
        }
        let bytes_per_sector = read_le_u16(&buf, 11)? as u32;
        let sectors_per_cluster = buf[13] as u32;
        let reserved_sectors = read_le_u16(&buf, 14)? as u64;
        let num_fats = buf[16] as u64;
        let fat_size_sectors = read_le_u32(&buf, 36)? as u64;
        let root_cluster = read_le_u32(&buf, 44)?;
        if bytes_per_sector < 512 || !bytes_per_sector.is_power_of_two() || sectors_per_cluster == 0 || fat_size_sectors == 0 {
            return Err(RxError::FsCorrupt("invalid FAT32 BPB".into()));
        }
        let bytes_per_cluster = (bytes_per_sector as u64) * (sectors_per_cluster as u64);
        let fat_start_offset = reserved_sectors * (bytes_per_sector as u64);
        let data_start_offset = (reserved_sectors + num_fats * fat_size_sectors) * (bytes_per_sector as u64);
        Ok(Self {
            bytes_per_sector,
            sectors_per_cluster,
            bytes_per_cluster,
            reserved_sectors,
            num_fats,
            fat_size_sectors,
            root_cluster,
            fat_start_offset,
            data_start_offset,
        })
    }

    pub fn cluster_to_offset(&self, cluster: u32) -> Option<u64> {
        if cluster < 2 {
            return None;
        }
        Some(self.data_start_offset + ((cluster as u64) - 2) * self.bytes_per_cluster)
    }
}
