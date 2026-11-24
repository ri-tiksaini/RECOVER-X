use crate::core::endian::{read_le_u16, read_le_u64};
use crate::core::types::ByteOffset;
use crate::error::{RxError, RxResult};
use crate::platform::raw_reader::{read_logical_slice, RawReader};

#[derive(Debug, Clone)]
pub struct NtfsBootSector {
    pub bytes_per_sector: u32,
    pub sectors_per_cluster: u32,
    pub bytes_per_cluster: u64,
    pub total_sectors: u64,
    pub mft_lcn: u64,
    pub mft_mirr_lcn: u64,
    pub mft_record_size: usize,
}

impl NtfsBootSector {
    pub fn parse(reader: &dyn RawReader) -> RxResult<Self> {
        let sector0 = read_logical_slice(reader, ByteOffset(0), 512)?;
        if let Ok(bs) = Self::from_bytes(&sector0) {
            return Ok(bs);
        }
        // Fallback to NTFS backup boot sector at last sector of volume
        let ss = reader.sector_size() as u64;
        if reader.size() >= ss {
            let last = read_logical_slice(reader, ByteOffset(reader.size() - ss), 512)?;
            if let Ok(bs) = Self::from_bytes(&last) {
                tracing::warn!("NTFS primary boot sector damaged; recovered geometry from backup boot sector");
                return Ok(bs);
            }
        }
        Err(RxError::FsUnknown("not an NTFS volume".into()))
    }

    pub fn from_bytes(buf: &[u8]) -> RxResult<Self> {
        if buf.len() < 84 || &buf[3..11] != b"NTFS    " {
            return Err(RxError::FsUnknown("missing NTFS OEM ID".into()));
        }
        let bytes_per_sector = read_le_u16(buf, 11)? as u32;
        let sectors_per_cluster = buf[13] as u32;
        if bytes_per_sector < 512 || !bytes_per_sector.is_power_of_two() || sectors_per_cluster == 0 {
            return Err(RxError::FsCorrupt("invalid NTFS BPB geometry".into()));
        }
        let bytes_per_cluster = (bytes_per_sector as u64) * (sectors_per_cluster as u64);
        let total_sectors = read_le_u64(buf, 40)?;
        let mft_lcn = read_le_u64(buf, 48)?;
        let mft_mirr_lcn = read_le_u64(buf, 56)?;
        let raw_rec = buf[64] as i8;
        let mft_record_size = if raw_rec < 0 {
            1usize << ((-raw_rec) as u32)
        } else {
            (raw_rec as usize) * (bytes_per_cluster as usize)
        };
        if !(512..=65536).contains(&mft_record_size) {
            return Err(RxError::FsCorrupt("invalid MFT record size".into()));
        }
        Ok(Self {
            bytes_per_sector,
            sectors_per_cluster,
            bytes_per_cluster,
            total_sectors,
            mft_lcn,
            mft_mirr_lcn,
            mft_record_size,
        })
    }

    pub fn mft_physical_offset(&self) -> u64 {
        self.mft_lcn.saturating_mul(self.bytes_per_cluster)
    }
}
