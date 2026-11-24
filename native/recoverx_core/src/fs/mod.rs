pub mod common;
pub mod exfat;
pub mod ext;
pub mod fat;
pub mod ntfs;

use crate::carve::scanner::{carve_volume, CarveAlignment};
use crate::core::cancel::CancelToken;
use crate::core::types::ByteOffset;
use crate::error::RxResult;
use crate::fs::common::candidate::{FsKind, RecoverCandidate};
use crate::fs::exfat::ExFatVolume;
use crate::fs::ext::ExtVolume;
use crate::fs::fat::Fat32Volume;
use crate::fs::ntfs::NtfsVolume;
use crate::pipeline::merge::merge_candidates;
use crate::platform::raw_reader::{read_logical_slice, RawReader};
use crate::platform::trim::MediaKind;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanMode {
    Quick, // Metadata pass only
    Deep,  // Metadata + sector-aligned carving + merge dedup
}

pub fn detect_fs(reader: &dyn RawReader) -> RxResult<FsKind> {
    let head = read_logical_slice(reader, ByteOffset(0), 2048)?;
    if head.len() >= 11 && &head[3..11] == b"NTFS    " {
        return Ok(FsKind::Ntfs);
    }
    if head.len() >= 11 && &head[3..11] == b"EXFAT   " {
        return Ok(FsKind::ExFat);
    }
    if head.len() >= 90 && &head[82..90] == b"FAT32   " {
        return Ok(FsKind::Fat32);
    }
    if head.len() >= 1082 && head[1080] == 0x53 && head[1081] == 0xEF {
        return Ok(FsKind::Ext);
    }
    if head.len() >= 36 && &head[32..36] == b"NXSB" {
        return Ok(FsKind::Apfs);
    }
    Ok(FsKind::Unknown)
}

pub fn scan_volume(
    reader: Arc<dyn RawReader>,
    device_id: String,
    media_kind: MediaKind,
    mode: ScanMode,
    cancel: &CancelToken,
    max_candidates: Option<usize>,
) -> RxResult<(FsKind, Vec<RecoverCandidate>)> {
    let kind = detect_fs(reader.as_ref())?;
    let meta = match kind {
        FsKind::Ntfs => {
            let vol = NtfsVolume::open(Arc::clone(&reader), device_id.clone(), media_kind, 32)?;
            vol.scan_deleted(cancel, max_candidates)?
        }
        FsKind::Fat32 => {
            let vol = Fat32Volume::open(Arc::clone(&reader), device_id.clone(), media_kind)?;
            vol.scan_deleted(cancel, max_candidates)?
        }
        FsKind::ExFat => {
            let vol = ExFatVolume::open(Arc::clone(&reader), device_id.clone(), media_kind)?;
            vol.scan_deleted(cancel, max_candidates)?
        }
        FsKind::Ext => {
            let vol = ExtVolume::open(Arc::clone(&reader), device_id.clone(), media_kind)?;
            vol.scan_deleted(cancel, max_candidates)?
        }
        FsKind::Apfs | FsKind::HfsPlus | FsKind::Unknown => Vec::new(),
    };

    if mode == ScanMode::Deep || kind == FsKind::Unknown {
        let carved = carve_volume(
            reader.as_ref(),
            &device_id,
            kind,
            CarveAlignment::Sector,
            cancel,
            max_candidates,
        )?;
        Ok((kind, merge_candidates(meta, carved)))
    } else {
        Ok((kind, meta))
    }
}

/// Backward-compatible alias for metadata-only scan.
pub fn scan_volume_metadata(
    reader: Arc<dyn RawReader>,
    device_id: String,
    media_kind: MediaKind,
    cancel: &CancelToken,
    max_candidates: Option<usize>,
) -> RxResult<(FsKind, Vec<RecoverCandidate>)> {
    scan_volume(reader, device_id, media_kind, ScanMode::Quick, cancel, max_candidates)
}
