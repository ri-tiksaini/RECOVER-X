pub mod extents;
pub mod inode;
pub mod superblock;

use crate::core::cancel::CancelToken;
use crate::error::RxResult;
use crate::fs::common::candidate::RecoverCandidate;
use crate::fs::ext::inode::scan_ext_deleted;
use crate::fs::ext::superblock::ExtSuperblock;
use crate::platform::raw_reader::RawReader;
use crate::platform::trim::MediaKind;
use std::sync::Arc;

pub struct ExtVolume {
    reader: Arc<dyn RawReader>,
    pub sb: ExtSuperblock,
    device_id: String,
    media_kind: MediaKind,
}

impl ExtVolume {
    pub fn open(reader: Arc<dyn RawReader>, device_id: String, media_kind: MediaKind) -> RxResult<Self> {
        let sb = ExtSuperblock::parse(reader.as_ref())?;
        Ok(Self { reader, sb, device_id, media_kind })
    }

    pub fn scan_deleted(
        &self,
        cancel: &CancelToken,
        max_candidates: Option<usize>,
    ) -> RxResult<Vec<RecoverCandidate>> {
        scan_ext_deleted(
            self.reader.as_ref(),
            &self.sb,
            &self.device_id,
            self.media_kind,
            cancel,
            max_candidates,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::common::candidate::RecoveryStatus;
    use crate::platform::image_reader::ImageReader;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn ext4_golden_recovers_deleted_extent_inode() {
        let blk = 4096usize;
        let mut img = vec![0u8; blk * 20];
        // Superblock at offset 1024
        let sb = 1024usize;
        img[sb..sb + 4].copy_from_slice(&32u32.to_le_bytes()); // inodes_count
        img[sb + 4..sb + 8].copy_from_slice(&20u32.to_le_bytes()); // blocks_count
        img[sb + 24..sb + 28].copy_from_slice(&2u32.to_le_bytes()); // log_block_size=2 -> 4096
        img[sb + 32..sb + 36].copy_from_slice(&20u32.to_le_bytes()); // blocks_per_group
        img[sb + 40..sb + 44].copy_from_slice(&32u32.to_le_bytes()); // inodes_per_group
        img[sb + 56..sb + 58].copy_from_slice(&0xEF53u16.to_le_bytes());
        img[sb + 88..sb + 90].copy_from_slice(&256u16.to_le_bytes()); // inode_size = 256
        img[sb + 96..sb + 100].copy_from_slice(&0x0040u32.to_le_bytes()); // INCOMPAT_EXTENTS

        // Group descriptor table at block 1 (offset 4096):
        // bg_block_bitmap_lo = block 2, bg_inode_table_lo = block 4
        let gdt = blk;
        img[gdt..gdt + 4].copy_from_slice(&2u32.to_le_bytes());
        img[gdt + 8..gdt + 12].copy_from_slice(&4u32.to_le_bytes());

        // Inode 12 (index 11) in inode table at block 4: deleted regular file with ext4 extent pointing to block 10
        let ino = 4 * blk + 11 * 256;
        img[ino..ino + 2].copy_from_slice(&0x81A4u16.to_le_bytes()); // regular file
        img[ino + 4..ino + 8].copy_from_slice(&256u32.to_le_bytes()); // size = 256
        img[ino + 20..ino + 24].copy_from_slice(&1700000000u32.to_le_bytes()); // i_dtime != 0
        img[ino + 26..ino + 28].copy_from_slice(&0u16.to_le_bytes()); // i_links_count == 0
        img[ino + 32..ino + 36].copy_from_slice(&0x0008_0000u32.to_le_bytes()); // EXT4_EXTENTS_FL
        // Extent header at ino + 40
        let eh = ino + 40;
        img[eh..eh + 2].copy_from_slice(&0xF30Au16.to_le_bytes());
        img[eh + 2..eh + 4].copy_from_slice(&1u16.to_le_bytes()); // 1 entry
        img[eh + 4..eh + 6].copy_from_slice(&4u16.to_le_bytes()); // max 4
        img[eh + 6..eh + 8].copy_from_slice(&0u16.to_le_bytes()); // leaf depth 0
        // Extent leaf at eh + 12: logical 0, len 1, start_lo = 10
        let ex = eh + 12;
        img[ex + 4..ex + 6].copy_from_slice(&1u16.to_le_bytes());
        img[ex + 8..ex + 12].copy_from_slice(&10u32.to_le_bytes());

        // Block 10 payload: PDF magic
        img[10 * blk..10 * blk + 5].copy_from_slice(b"%PDF-");

        let mut tmp = NamedTempFile::new().unwrap();
        tmp.write_all(&img).unwrap();
        let reader: Arc<dyn RawReader> = Arc::new(ImageReader::open(tmp.path(), 512).unwrap());
        let vol = ExtVolume::open(reader, "ext0".into(), MediaKind::Hdd).unwrap();
        let cands = vol.scan_deleted(&CancelToken::new(), None).unwrap();

        assert_eq!(cands.len(), 1);
        assert_eq!(cands[0].status, RecoveryStatus::Intact);
        assert_eq!(cands[0].logical_size, 256);
    }
}
