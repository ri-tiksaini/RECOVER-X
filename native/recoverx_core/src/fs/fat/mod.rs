pub mod bpb;
pub mod directory;
pub mod fat_table;

use crate::core::cancel::CancelToken;
use crate::error::RxResult;
use crate::fs::common::candidate::RecoverCandidate;
use crate::fs::fat::bpb::Fat32Bpb;
use crate::fs::fat::directory::scan_fat32_deleted;
use crate::platform::raw_reader::RawReader;
use crate::platform::trim::MediaKind;
use std::sync::Arc;

pub struct Fat32Volume {
    reader: Arc<dyn RawReader>,
    pub bpb: Fat32Bpb,
    device_id: String,
    media_kind: MediaKind,
}

impl Fat32Volume {
    pub fn open(reader: Arc<dyn RawReader>, device_id: String, media_kind: MediaKind) -> RxResult<Self> {
        let bpb = Fat32Bpb::parse(reader.as_ref())?;
        Ok(Self { reader, bpb, device_id, media_kind })
    }

    pub fn scan_deleted(
        &self,
        cancel: &CancelToken,
        max_candidates: Option<usize>,
    ) -> RxResult<Vec<RecoverCandidate>> {
        scan_fat32_deleted(
            self.reader.as_ref(),
            &self.bpb,
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
    fn fat32_golden_recovers_deleted_lfn_file() {
        let sector = 512usize;
        let spc = 2usize; // 1024 bytes/cluster
        let cluster = sector * spc;
        let reserved = 32usize;
        let fat_secs = 8usize;
        let data_off = (reserved + 2 * fat_secs) * sector;
        let mut img = vec![0u8; data_off + cluster * 16];

        // FAT32 BPB
        img[11..13].copy_from_slice(&(sector as u16).to_le_bytes());
        img[13] = spc as u8;
        img[14..16].copy_from_slice(&(reserved as u16).to_le_bytes());
        img[16] = 2; // num_fats
        img[36..40].copy_from_slice(&(fat_secs as u32).to_le_bytes());
        img[44..48].copy_from_slice(&2u32.to_le_bytes()); // root_cluster = 2
        img[82..90].copy_from_slice(b"FAT32   ");
        img[510] = 0x55;
        img[511] = 0xAA;

        // FAT entry 2 (root dir) = EOC
        let fat0 = reserved * sector;
        img[fat0 + 8..fat0 + 12].copy_from_slice(&0x0FFF_FFFFu32.to_le_bytes());

        // Root dir at cluster 2 (data_off): LFN entry + deleted 8.3 entry for "HolidayPhoto.jpg"
        let short_name = *b"HOLIDA~1JPG";
        let csum = crate::fs::fat::directory::lfn_checksum(&short_name);
        // LFN entry 2 (last, seq 0x42, deleted -> 0xE5? Keep seq=0xE5 with 0x0F attr as real FAT32 does)
        let lfn_chars: Vec<u16> = "HolidayPhoto.jpg\0".encode_utf16().collect();
        let mut write_lfn = |slot: usize, chars: &[u16]| {
            let base = data_off + slot * 32;
            img[base] = 0xE5; // deleted LFN marker
            img[base + 11] = 0x0F;
            img[base + 13] = csum;
            let offsets = [1, 3, 5, 7, 9, 14, 16, 18, 20, 22, 24, 28, 30];
            for (i, &o) in offsets.iter().enumerate() {
                let ch = chars.get(i).copied().unwrap_or(0xFFFF);
                img[base + o..base + o + 2].copy_from_slice(&ch.to_le_bytes());
            }
        };
        write_lfn(0, &lfn_chars[13..]);
        write_lfn(1, &lfn_chars[..13]);

        // Deleted 8.3 entry at slot 2 pointing to cluster 4 (free in FAT!), size 256
        let e = data_off + 2 * 32;
        img[e..e + 11].copy_from_slice(&short_name);
        img[e] = 0xE5; // deleted marker
        img[e + 11] = 0x20; // Archive
        img[e + 26..e + 28].copy_from_slice(&4u16.to_le_bytes()); // start cluster low = 4
        img[e + 28..e + 32].copy_from_slice(&256u32.to_le_bytes()); // size = 256

        // Cluster 4 payload (valid JPEG header)
        let c4 = data_off + (4 - 2) * cluster;
        img[c4..c4 + 6].copy_from_slice(&[0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10]);

        let mut tmp = NamedTempFile::new().unwrap();
        tmp.write_all(&img).unwrap();
        let reader: Arc<dyn RawReader> = Arc::new(ImageReader::open(tmp.path(), 512).unwrap());
        let vol = Fat32Volume::open(reader, "usb0".into(), MediaKind::Hdd).unwrap();
        let cands = vol.scan_deleted(&CancelToken::new(), None).unwrap();

        assert_eq!(cands.len(), 1);
        assert_eq!(cands[0].name_display, "HolidayPhoto.jpg");
        assert_eq!(cands[0].path_display, "/HolidayPhoto.jpg");
        assert_eq!(cands[0].status, RecoveryStatus::Intact);
    }
}
