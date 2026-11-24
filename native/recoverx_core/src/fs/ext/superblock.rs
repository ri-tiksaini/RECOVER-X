use crate::core::endian::{read_le_u16, read_le_u32};
use crate::core::types::ByteOffset;
use crate::error::{RxError, RxResult};
use crate::platform::raw_reader::{read_logical_slice, RawReader};

#[derive(Debug, Clone)]
pub struct ExtSuperblock {
    pub inodes_count: u32,
    pub blocks_count: u64,
    pub block_size: u64,
    pub blocks_per_group: u32,
    pub inodes_per_group: u32,
    pub inode_size: usize,
    pub desc_size: usize,
    pub group_count: u32,
    pub first_data_block: u32,
}

impl ExtSuperblock {
    pub fn parse(reader: &dyn RawReader) -> RxResult<Self> {
        let buf = read_logical_slice(reader, ByteOffset(1024), 1024)?;
        if let Ok(sb) = Self::from_bytes(&buf) {
            return Ok(sb);
        }
        // Fallback to backup superblock at block 32768 (4 KiB blocks)
        let b32k = 32768u64 * 4096;
        if reader.size() >= b32k + 1024 {
            let bak = read_logical_slice(reader, ByteOffset(b32k), 1024)?;
            if let Ok(sb) = Self::from_bytes(&bak) {
                tracing::warn!("ext primary superblock damaged; recovered from backup @32768");
                return Ok(sb);
            }
        }
        Err(RxError::FsUnknown("not an ext2/3/4 volume".into()))
    }

    fn from_bytes(buf: &[u8]) -> RxResult<Self> {
        if buf.len() < 260 || read_le_u16(buf, 56)? != 0xEF53 {
            return Err(RxError::FsUnknown("missing ext 0xEF53 magic".into()));
        }
        let inodes_count = read_le_u32(buf, 0)?;
        let blocks_lo = read_le_u32(buf, 4)? as u64;
        let first_data_block = read_le_u32(buf, 20)?;
        let log_bs = read_le_u32(buf, 24)?;
        if log_bs > 6 {
            return Err(RxError::FsCorrupt("invalid ext log_block_size".into()));
        }
        let block_size = 1024u64 << log_bs;
        let blocks_per_group = read_le_u32(buf, 32)?.max(1);
        let inodes_per_group = read_le_u32(buf, 40)?.max(1);
        let inode_size = read_le_u16(buf, 88).unwrap_or(128).max(128) as usize;
        let incompat = read_le_u32(buf, 96).unwrap_or(0);
        let is_64bit = (incompat & 0x0080) != 0;
        let desc_size = if is_64bit {
            read_le_u16(buf, 254).unwrap_or(64).max(64) as usize
        } else {
            32
        };
        let group_count = ((blocks_lo.saturating_sub(first_data_block as u64) + (blocks_per_group as u64) - 1)
            / (blocks_per_group as u64))
            .max(1) as u32;

        Ok(Self {
            inodes_count,
            blocks_count: blocks_lo,
            block_size,
            blocks_per_group,
            inodes_per_group,
            inode_size,
            desc_size,
            group_count,
            first_data_block,
        })
    }
}
