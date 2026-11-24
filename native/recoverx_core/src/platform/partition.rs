use crate::core::endian::{read_le_u32, read_le_u64};
use crate::core::types::ByteOffset;
use crate::error::{RxError, RxResult};
use crate::platform::raw_reader::RawReader;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionEntry {
    pub index: usize,
    pub scheme: String,
    pub type_label: String,
    pub start_lba: u64,
    pub length_lbas: u64,
    pub start_bytes: u64,
    pub length_bytes: u64,
}

/// Bounds-checked, sector-aligned slice over an underlying RawReader.
pub struct SubSliceReader {
    inner: Arc<dyn RawReader>,
    base_offset: ByteOffset,
    len_bytes: u64,
}

impl SubSliceReader {
    pub fn new(inner: Arc<dyn RawReader>, base_offset: ByteOffset, len_bytes: u64) -> RxResult<Self> {
        let ss = inner.sector_size() as u64;
        if base_offset.0 % ss != 0 {
            return Err(RxError::UnalignedAccess {
                offset: base_offset.0,
                len: 0,
                sector: inner.sector_size(),
            });
        }
        let max_len = inner.size().saturating_sub(base_offset.0);
        Ok(Self {
            inner,
            base_offset,
            len_bytes: len_bytes.min(max_len),
        })
    }
}

impl RawReader for SubSliceReader {
    fn sector_size(&self) -> u32 {
        self.inner.sector_size()
    }

    fn size(&self) -> u64 {
        self.len_bytes
    }

    fn read_at(&self, off: ByteOffset, buf: &mut [u8]) -> RxResult<usize> {
        if off.0 >= self.len_bytes {
            return Ok(0);
        }
        let max_read = (self.len_bytes - off.0) as usize;
        let capped = buf.len().min(max_read);
        let phys = self
            .base_offset
            .checked_add(off.0)
            .ok_or_else(|| RxError::Internal("SubSliceReader offset overflow".into()))?;
        self.inner.read_at(phys, &mut buf[..capped])
    }
}

/// Parses GPT (preferred, CRC32-verified) or MBR partition tables from a RawReader.
pub fn parse_partitions(reader: &dyn RawReader) -> RxResult<Vec<PartitionEntry>> {
    let ss = reader.sector_size() as usize;
    if reader.size() < (ss * 2) as u64 {
        return Ok(Vec::new());
    }

    let mut lba1 = vec![0u8; ss];
    let got = reader.read_at(ByteOffset(ss as u64), &mut lba1)?;
    if got >= 92 && &lba1[0..8] == b"EFI PART" {
        if let Ok(parts) = parse_gpt(reader, &lba1) {
            if !parts.is_empty() {
                return Ok(parts);
            }
        }
    }

    let mut lba0 = vec![0u8; ss];
    let got0 = reader.read_at(ByteOffset(0), &mut lba0)?;
    if got0 >= 512 && lba0[510] == 0x55 && lba0[511] == 0xAA {
        return parse_mbr(&lba0, ss as u64);
    }

    Ok(Vec::new())
}

fn parse_gpt(reader: &dyn RawReader, hdr: &[u8]) -> RxResult<Vec<PartitionEntry>> {
    let ss = reader.sector_size() as u64;
    let hdr_size = read_le_u32(hdr, 12)? as usize;
    if hdr_size < 92 || hdr_size > hdr.len() {
        return Err(RxError::FsCorrupt("invalid GPT header size".into()));
    }
    let expected_crc = read_le_u32(hdr, 16)?;
    let mut check_buf = hdr[..hdr_size].to_vec();
    check_buf[16..20].fill(0);
    if crc32fast::hash(&check_buf) != expected_crc {
        return Err(RxError::FsCorrupt("GPT header CRC32 mismatch".into()));
    }

    let entries_lba = read_le_u64(hdr, 72)?;
    let num_entries = read_le_u32(hdr, 80)? as usize;
    let entry_size = read_le_u32(hdr, 84)? as usize;
    if entry_size < 128 || num_entries > 4096 {
        return Err(RxError::FsCorrupt("unreasonable GPT entry geometry".into()));
    }

    let raw_bytes = num_entries * entry_size;
    let read_len = (((raw_bytes as u64 - 1) / ss) + 1) * ss;
    let mut table = vec![0u8; read_len as usize];
    reader.read_at(ByteOffset(entries_lba * ss), &mut table)?;

    let mut out = Vec::new();
    for i in 0..num_entries {
        let base = i * entry_size;
        let type_guid = &table[base..base + 16];
        if type_guid.iter().all(|&b| b == 0) {
            continue;
        }
        let first_lba = read_le_u64(&table, base + 32)?;
        let last_lba = read_le_u64(&table, base + 40)?;
        if last_lba < first_lba {
            continue;
        }
        let length_lbas = last_lba - first_lba + 1;
        out.push(PartitionEntry {
            index: out.len(),
            scheme: "GPT".into(),
            type_label: format!("{:02X?}", &type_guid[..4]),
            start_lba: first_lba,
            length_lbas,
            start_bytes: first_lba * ss,
            length_bytes: length_lbas * ss,
        });
    }
    Ok(out)
}

fn parse_mbr(mbr: &[u8], ss: u64) -> RxResult<Vec<PartitionEntry>> {
    let mut out = Vec::new();
    for i in 0..4usize {
        let base = 446 + i * 16;
        let ptype = mbr[base + 4];
        if ptype == 0 || ptype == 0xEE {
            continue;
        }
        let start_lba = read_le_u32(mbr, base + 8)? as u64;
        let length_lbas = read_le_u32(mbr, base + 12)? as u64;
        if length_lbas == 0 {
            continue;
        }
        let label = match ptype {
            0x07 => "NTFS/exFAT",
            0x0B | 0x0C => "FAT32",
            0x83 => "Linux",
            _ => "Other",
        };
        out.push(PartitionEntry {
            index: out.len(),
            scheme: "MBR".into(),
            type_label: format!("{label} (0x{ptype:02X})"),
            start_lba,
            length_lbas,
            start_bytes: start_lba * ss,
            length_bytes: length_lbas * ss,
        });
    }
    Ok(out)
}

/// Probes backup filesystem headers (NTFS backup boot sector at last sector, ext2/3/4 backup superblocks).
pub fn probe_backup_fs_hint(reader: &dyn RawReader) -> Option<String> {
    let ss = reader.sector_size() as u64;
    if reader.size() >= ss {
        let mut last = vec![0u8; ss as usize];
        if reader.read_at(ByteOffset(reader.size() - ss), &mut last).is_ok()
            && last.len() >= 11
            && &last[3..11] == b"NTFS    "
        {
            return Some("NTFS (backup boot sector)".into());
        }
    }
    // Check ext4 backup superblock at block 32768 (for 4 KiB blocks -> offset 128 MiB)
    let ext4_b32k = 32768u64 * 4096;
    if reader.size() >= ext4_b32k + 4096 {
        let mut blk = vec![0u8; 4096];
        if reader.read_at(ByteOffset(ext4_b32k), &mut blk).is_ok()
            && blk[56] == 0x53
            && blk[57] == 0xEF
        {
            return Some("ext2/3/4 (backup superblock @32768)".into());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::image_reader::ImageReader;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn parses_synthetic_mbr() {
        let mut tmp = NamedTempFile::new().unwrap();
        let mut img = vec![0u8; 4096];
        // Partition 0: NTFS starting at LBA 2, length 4 sectors
        img[446 + 4] = 0x07;
        img[446 + 8..446 + 12].copy_from_slice(&2u32.to_le_bytes());
        img[446 + 12..446 + 16].copy_from_slice(&4u32.to_le_bytes());
        img[510] = 0x55;
        img[511] = 0xAA;
        tmp.write_all(&img).unwrap();

        let reader = ImageReader::open(tmp.path(), 512).unwrap();
        let parts = parse_partitions(&reader).unwrap();
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].scheme, "MBR");
        assert_eq!(parts[0].start_lba, 2);
        assert_eq!(parts[0].length_lbas, 4);
    }
}
