pub mod boot;
pub mod directory;

use crate::core::cancel::CancelToken;
use crate::error::RxResult;
use crate::fs::common::candidate::RecoverCandidate;
use crate::fs::exfat::boot::ExFatBoot;
use crate::fs::exfat::directory::scan_exfat_deleted;
use crate::platform::raw_reader::RawReader;
use crate::platform::trim::MediaKind;
use std::sync::Arc;

pub struct ExFatVolume {
    reader: Arc<dyn RawReader>,
    pub boot: ExFatBoot,
    device_id: String,
    media_kind: MediaKind,
}

impl ExFatVolume {
    pub fn open(reader: Arc<dyn RawReader>, device_id: String, media_kind: MediaKind) -> RxResult<Self> {
        let boot = ExFatBoot::parse(reader.as_ref())?;
        Ok(Self { reader, boot, device_id, media_kind })
    }

    pub fn scan_deleted(
        &self,
        cancel: &CancelToken,
        max_candidates: Option<usize>,
    ) -> RxResult<Vec<RecoverCandidate>> {
        scan_exfat_deleted(
            Arc::clone(&self.reader),
            &self.boot,
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
    fn exfat_golden_recovers_deleted_unicode_file() {
        let bps = 512usize;
        let bpc = 4096usize;
        let heap_sec = 64u32;
        let heap_off = (heap_sec as usize) * bps;
        let mut img = vec![0u8; heap_off + bpc * 12];

        img[3..11].copy_from_slice(b"EXFAT   ");
        img[80..84].copy_from_slice(&32u32.to_le_bytes()); // fat_offset
        img[84..88].copy_from_slice(&32u32.to_le_bytes()); // fat_length
        img[88..92].copy_from_slice(&heap_sec.to_le_bytes());
        img[92..96].copy_from_slice(&12u32.to_le_bytes()); // cluster_count
        img[96..100].copy_from_slice(&2u32.to_le_bytes()); // root_dir_cluster = 2
        img[108] = 9;  // 512 bytes/sector
        img[109] = 3;  // 8 sectors/cluster = 4096

        // Root dir at cluster 2 (heap_off):
        // Slot 0: Allocation Bitmap (0x81) at cluster 3, size 8 bytes (all 0 = free)
        img[heap_off] = 0x81;
        img[heap_off + 20..heap_off + 24].copy_from_slice(&3u32.to_le_bytes());
        img[heap_off + 24..heap_off + 32].copy_from_slice(&8u64.to_le_bytes());

        // Slot 1: Deleted File entry (0x05), secondary_count = 2
        let s1 = heap_off + 32;
        img[s1] = 0x05;
        img[s1 + 1] = 2;
        // Slot 2: Deleted Stream Extension (0x40), no_fat_chain=1, name_len=9, valid_data_len=200, first_cluster=5
        let s2 = heap_off + 64;
        img[s2] = 0x40;
        img[s2 + 1] = 0x03; // no_fat_chain
        img[s2 + 3] = 9;
        img[s2 + 8..s2 + 16].copy_from_slice(&200u64.to_le_bytes());
        img[s2 + 20..s2 + 24].copy_from_slice(&5u32.to_le_bytes());
        // Slot 3: Deleted File Name (0x41) "clip.mp4\0" (actually "video.mp4" = 9 chars)
        let s3 = heap_off + 96;
        img[s3] = 0x41;
        for (i, ch) in "video.mp4".encode_utf16().enumerate() {
            img[s3 + 2 + i * 2..s3 + 4 + i * 2].copy_from_slice(&ch.to_le_bytes());
        }

        // Cluster 5 payload: valid MP4 ftyp header
        let c5 = heap_off + (5 - 2) * bpc;
        img[c5..c5 + 8].copy_from_slice(b"\x00\x00\x00\x18ftyp");

        let mut tmp = NamedTempFile::new().unwrap();
        tmp.write_all(&img).unwrap();
        let reader: Arc<dyn RawReader> = Arc::new(ImageReader::open(tmp.path(), 512).unwrap());
        let vol = ExFatVolume::open(reader, "exfat0".into(), MediaKind::Hdd).unwrap();
        let cands = vol.scan_deleted(&CancelToken::new(), None).unwrap();

        assert_eq!(cands.len(), 1);
        assert_eq!(cands[0].name_display, "video.mp4");
        assert_eq!(cands[0].status, RecoveryStatus::Intact);
    }
}
