use crate::core::cancel::CancelToken;
use crate::core::endian::{read_le_u32, read_le_u64};
use crate::core::types::{ByteOffset, CandidateId};
use crate::error::RxResult;
use crate::fs::common::bitmap_cache::PagedBitmapCache;
use crate::fs::common::candidate::{CandidateOrigin, FsKind, RecoverCandidate};
use crate::fs::common::confidence::{evaluate, ConfidenceInput};
use crate::fs::common::extent::{Extent, ExtentKind};
use crate::fs::exfat::boot::ExFatBoot;
use crate::platform::raw_reader::{read_logical_slice, RawReader};
use crate::platform::trim::MediaKind;
use crate::validate::{header_match, overlap, zero_scan};
use std::sync::Arc;

pub fn scan_exfat_deleted(
    reader: Arc<dyn RawReader>,
    boot: &ExFatBoot,
    device_id: &str,
    media_kind: MediaKind,
    cancel: &CancelToken,
    max_candidates: Option<usize>,
) -> RxResult<Vec<RecoverCandidate>> {
    let Some(root_off) = boot.cluster_to_offset(boot.root_dir_cluster) else {
        return Ok(Vec::new());
    };
    let root_buf = read_logical_slice(reader.as_ref(), ByteOffset(root_off), boot.bytes_per_cluster as usize)?;

    // Locate Allocation Bitmap (0x81) in root directory
    let mut bitmap_cache: Option<PagedBitmapCache> = None;
    for ent in root_buf.chunks_exact(32) {
        if ent[0] == 0x81 {
            let first_cl = read_le_u32(ent, 20).unwrap_or(0);
            let bm_len = read_le_u64(ent, 24).unwrap_or(0);
            if let Some(bm_off) = boot.cluster_to_offset(first_cl) {
                bitmap_cache = Some(PagedBitmapCache::new(
                    Arc::clone(&reader),
                    vec![Extent {
                        logical_off: 0,
                        physical_off: bm_off,
                        len: bm_len,
                        kind: ExtentKind::Data,
                    }],
                    bm_len,
                    32,
                ));
            }
            break;
        }
    }

    let mut out = Vec::new();
    let entries: Vec<&[u8]> = root_buf.chunks_exact(32).collect();
    let mut i = 0usize;
    while i < entries.len() {
        cancel.check()?;
        if let Some(lim) = max_candidates {
            if out.len() >= lim {
                break;
            }
        }
        let ent = entries[i];
        if ent[0] == 0x00 {
            break;
        }
        // Deleted File Directory Entry = 0x05 (high bit of 0x85 cleared)
        if ent[0] == 0x05 {
            let sec_count = ent[1] as usize;
            if i + sec_count < entries.len() && sec_count >= 2 {
                let stream = entries[i + 1];
                if stream[0] == 0x40 {
                    let name_len = stream[3] as usize;
                    let valid_len = read_le_u64(stream, 8).unwrap_or(0);
                    let first_cl = read_le_u32(stream, 20).unwrap_or(0);
                    let mut utf16 = Vec::new();
                    for k in 2..=sec_count {
                        let fn_ent = entries[i + k];
                        if fn_ent[0] == 0x41 {
                            for p in (2..32).step_by(2) {
                                if utf16.len() < name_len {
                                    utf16.push(u16::from_le_bytes([fn_ent[p], fn_ent[p + 1]]));
                                }
                            }
                        }
                    }
                    let name = String::from_utf16_lossy(&utf16);
                    if valid_len > 0 && first_cl >= 2 {
                        if let Some(phys_off) = boot.cluster_to_offset(first_cl) {
                            let cl_cnt = ((valid_len - 1) / boot.bytes_per_cluster) + 1;
                            let extents = vec![Extent {
                                logical_off: 0,
                                physical_off: phys_off,
                                len: cl_cnt * boot.bytes_per_cluster,
                                kind: ExtentKind::Data,
                            }];
                            let ov = overlap::compute_overlap_ratio(
                                bitmap_cache.as_ref(),
                                &extents,
                                boot.bytes_per_cluster,
                                boot.cluster_heap_offset,
                            );
                            let zr = zero_scan::region_zero_ratio(reader.as_ref(), &extents, valid_len);
                            let head = read_logical_slice(
                                reader.as_ref(),
                                ByteOffset(phys_off),
                                valid_len.min(512) as usize,
                            )
                            .unwrap_or_default();
                            let header_ok = header_match::check_header_agreement(&name, &head);
                            let (status, reason, conf) = evaluate(&ConfidenceInput {
                                has_name: !name.is_empty(),
                                has_parent_path: true,
                                runlist_intact: true,
                                overlap_ratio: ov,
                                zero_ratio: zr,
                                header_agreement: header_ok,
                                structural_valid: header_ok.unwrap_or(true),
                                has_io_errors: false,
                                is_compressed: false,
                                is_encrypted: false,
                                high_entropy_no_header: false,
                                media_kind,
                            });
                            let cid = CandidateId::derive(
                                device_id,
                                CandidateOrigin::Metadata as u8,
                                first_cl as u64,
                                ByteOffset(phys_off),
                                valid_len,
                            );
                            out.push(RecoverCandidate {
                                id_hex: RecoverCandidate::id_to_hex(cid),
                                id_bytes: cid.0,
                                name_bytes: name.as_bytes().to_vec(),
                                name_display: name.clone(),
                                path_display: format!("/{name}"),
                                logical_size: valid_len,
                                first_byte_offset: phys_off,
                                fs_kind: FsKind::ExFat,
                                origin: CandidateOrigin::Metadata,
                                status,
                                reason_code: reason,
                                confidence: conf,
                                mtime_utc_ns: 0,
                                ctime_utc_ns: 0,
                                extents,
                                resident_data: None,
                                is_compressed: false,
                                is_encrypted: false,
                            });
                        }
                    }
                }
                i += sec_count + 1;
                continue;
            }
        }
        i += 1;
    }
    Ok(out)
}
