pub mod boot_sector;
pub mod dataruns;
pub mod mft;

use crate::core::cancel::CancelToken;
use crate::core::types::{ByteOffset, CandidateId};
use crate::error::RxResult;
use crate::fs::common::bitmap_cache::PagedBitmapCache;
use crate::fs::common::candidate::{CandidateOrigin, FsKind, RecoverCandidate};
use crate::fs::common::confidence::{evaluate, ConfidenceInput};
use crate::fs::common::extent::{Extent, ExtentKind};
use crate::fs::ntfs::boot_sector::NtfsBootSector;
use crate::fs::ntfs::mft::{parse_mft_record, ParsedMftRecord};
use crate::platform::raw_reader::{read_logical_slice, RawReader};
use crate::platform::trim::MediaKind;
use crate::validate::{entropy, header_match, overlap, zero_scan};
use std::collections::HashMap;
use std::sync::Arc;

pub struct NtfsVolume {
    reader: Arc<dyn RawReader>,
    pub boot: NtfsBootSector,
    device_id: String,
    media_kind: MediaKind,
    mft_extents: Vec<Extent>,
    mft_total_bytes: u64,
    bitmap_cache: Option<Arc<PagedBitmapCache>>,
}

impl NtfsVolume {
    pub fn open(
        reader: Arc<dyn RawReader>,
        device_id: String,
        media_kind: MediaKind,
        bitmap_cache_mb: usize,
    ) -> RxResult<Self> {
        let boot = NtfsBootSector::parse(reader.as_ref())?;
        // Read MFT record 0 ($MFT) to get the full $MFT runlist [P-9]
        let mft0_raw = read_logical_slice(
            reader.as_ref(),
            ByteOffset(boot.mft_physical_offset()),
            boot.mft_record_size,
        )?;
        let mft0 = parse_mft_record(0, &mft0_raw, boot.bytes_per_cluster)?;
        let mft_extents = if mft0.extents.is_empty() {
            vec![Extent {
                logical_off: 0,
                physical_off: boot.mft_physical_offset(),
                len: (boot.mft_record_size as u64) * 16,
                kind: ExtentKind::Data,
            }]
        } else {
            mft0.extents
        };
        let mft_total_bytes = mft0
            .logical_size
            .max(mft_extents.iter().map(|e| e.len).sum::<u64>());

        // Read MFT record 6 ($Bitmap) for paged O(1) allocation overlap checks [P-13]
        let mut bitmap_cache = None;
        if let Ok(bm_raw) = read_mft_record_by_num(
            reader.as_ref(),
            &mft_extents,
            boot.mft_record_size,
            6,
        ) {
            if let Ok(bm_rec) = parse_mft_record(6, &bm_raw, boot.bytes_per_cluster) {
                if !bm_rec.extents.is_empty() && bm_rec.logical_size > 0 {
                    bitmap_cache = Some(Arc::new(PagedBitmapCache::new(
                        Arc::clone(&reader),
                        bm_rec.extents,
                        bm_rec.logical_size,
                        bitmap_cache_mb,
                    )));
                }
            }
        }

        Ok(Self {
            reader,
            boot,
            device_id,
            media_kind,
            mft_extents,
            mft_total_bytes,
            bitmap_cache,
        })
    }

    pub fn scan_deleted(
        &self,
        cancel: &CancelToken,
        max_candidates: Option<usize>,
    ) -> RxResult<Vec<RecoverCandidate>> {
        let rec_size = self.boot.mft_record_size as u64;
        let total_records = self.mft_total_bytes / rec_size;

        // First pass: stream $MFT sequentially in 1 MiB batches to build dir tree & collect deleted files [P-9]
        let mut dir_map: HashMap<u64, (String, u64)> = HashMap::new();
        let mut deleted_raw: Vec<(u64, ParsedMftRecord)> = Vec::new();
        let mut bytes_checked = 0u64;

        for ext in &self.mft_extents {
            if ext.kind != ExtentKind::Data || ext.len == 0 {
                continue;
            }
            let mut ext_off = 0u64;
            while ext_off < ext.len {
                cancel.check()?;
                let chunk_bytes = (ext.len - ext_off).min(1024 * 1024);
                let chunk_bytes = (chunk_bytes / rec_size) * rec_size;
                if chunk_bytes == 0 {
                    break;
                }
                let buf = read_logical_slice(
                    self.reader.as_ref(),
                    ByteOffset(ext.physical_off + ext_off),
                    chunk_bytes as usize,
                )?;
                let base_rec_num = (ext.logical_off + ext_off) / rec_size;
                for (i, rec_slice) in buf.chunks_exact(rec_size as usize).enumerate() {
                    let rec_num = base_rec_num + i as u64;
                    if rec_num < 16 || rec_num >= total_records {
                        continue; // skip NTFS system metadata records 0..15
                    }
                    if &rec_slice[0..4] != b"FILE" {
                        continue;
                    }
                    let Ok(parsed) = parse_mft_record(rec_num, rec_slice, self.boot.bytes_per_cluster) else {
                        continue;
                    };
                    if parsed.is_directory {
                        if let (Some(name), Some(parent)) = (&parsed.name, parsed.parent_mft_ref) {
                            dir_map.insert(rec_num, (name.clone(), parent));
                        }
                        continue;
                    }
                    if !parsed.in_use && (parsed.logical_size > 0 || !parsed.extents.is_empty() || parsed.resident_data.is_some()) {
                        let phys_rec_off = ext.physical_off + ext_off + (i as u64) * rec_size;
                        deleted_raw.push((phys_rec_off, parsed));
                        if let Some(limit) = max_candidates {
                            if deleted_raw.len() >= limit {
                                break;
                            }
                        }
                    }
                }
                bytes_checked += chunk_bytes;
                if bytes_checked >= 4 * 1024 * 1024 {
                    cancel.check()?;
                    bytes_checked = 0;
                }
                if let Some(limit) = max_candidates {
                    if deleted_raw.len() >= limit {
                        break;
                    }
                }
                ext_off += chunk_bytes;
            }
        }

        // Second pass: resolve $ATTRIBUTE_LIST child extents, reconstruct paths, validate
        let mut results = Vec::with_capacity(deleted_raw.len());
        for (phys_rec_off, mut rec) in deleted_raw {
            cancel.check()?;
            for child_ref in rec.child_mft_refs.clone() {
                if let Ok(child_buf) = read_mft_record_by_num(
                    self.reader.as_ref(),
                    &self.mft_extents,
                    self.boot.mft_record_size,
                    child_ref,
                ) {
                    if let Ok(child) = parse_mft_record(child_ref, &child_buf, self.boot.bytes_per_cluster) {
                        rec.extents.extend(child.extents);
                    }
                }
            }

            let name = rec.name.clone().unwrap_or_else(|| format!("mft_{}.bin", rec.record_num));
            let (path_display, has_parent) = reconstruct_path(&dir_map, rec.parent_mft_ref, &name);
            let first_off = rec
                .extents
                .first()
                .map(|e| e.physical_off)
                .unwrap_or(phys_rec_off);

            let (zero_ratio, overlap_ratio, head_bytes) = if let Some(ref res) = rec.resident_data {
                (zero_scan::slice_zero_ratio(res), 0.0, res[..res.len().min(512)].to_vec())
            } else {
                let zr = zero_scan::region_zero_ratio(self.reader.as_ref(), &rec.extents, rec.logical_size);
                let ov = overlap::compute_overlap_ratio(
                    self.bitmap_cache.as_deref(),
                    &rec.extents,
                    self.boot.bytes_per_cluster,
                    0,
                );
                let hb = rec
                    .extents
                    .first()
                    .filter(|e| e.kind == ExtentKind::Data && e.len > 0)
                    .and_then(|e| {
                        read_logical_slice(
                            self.reader.as_ref(),
                            ByteOffset(e.physical_off),
                            e.len.min(512) as usize,
                        )
                        .ok()
                    })
                    .unwrap_or_default();
                (zr, ov, hb)
            };

            let header_ok = header_match::check_header_agreement(&name, &head_bytes);
            let high_ent = header_ok.is_none()
                && head_bytes.len() >= 256
                && entropy::shannon_entropy(&head_bytes) > 7.8;

            let (status, reason, confidence) = evaluate(&ConfidenceInput {
                has_name: rec.name.is_some(),
                has_parent_path: has_parent,
                runlist_intact: rec.resident_data.is_some() || !rec.extents.is_empty(),
                overlap_ratio,
                zero_ratio,
                header_agreement: header_ok,
                structural_valid: header_ok.unwrap_or(true),
                has_io_errors: false,
                is_compressed: rec.is_compressed,
                is_encrypted: rec.is_encrypted,
                high_entropy_no_header: high_ent,
                media_kind: self.media_kind,
            });

            let cid = CandidateId::derive(
                &self.device_id,
                CandidateOrigin::Metadata as u8,
                rec.record_num,
                ByteOffset(first_off),
                rec.logical_size,
            );

            results.push(RecoverCandidate {
                id_hex: RecoverCandidate::id_to_hex(cid),
                id_bytes: cid.0,
                name_bytes: if rec.name_raw.is_empty() { name.as_bytes().to_vec() } else { rec.name_raw },
                name_display: name,
                path_display,
                logical_size: rec.logical_size,
                first_byte_offset: first_off,
                fs_kind: FsKind::Ntfs,
                origin: CandidateOrigin::Metadata,
                status,
                reason_code: reason,
                confidence,
                mtime_utc_ns: rec.mtime_utc_ns,
                ctime_utc_ns: rec.ctime_utc_ns,
                extents: rec.extents,
                resident_data: rec.resident_data,
                is_compressed: rec.is_compressed,
                is_encrypted: rec.is_encrypted,
            });
        }

        Ok(results)
    }
}

fn read_mft_record_by_num(
    reader: &dyn RawReader,
    mft_extents: &[Extent],
    rec_size: usize,
    rec_num: u64,
) -> RxResult<Vec<u8>> {
    let target_logical = rec_num.saturating_mul(rec_size as u64);
    for ext in mft_extents {
        if ext.kind == ExtentKind::Data
            && target_logical >= ext.logical_off
            && target_logical + (rec_size as u64) <= ext.logical_off + ext.len
        {
            let phys = ext.physical_off + (target_logical - ext.logical_off);
            return read_logical_slice(reader, ByteOffset(phys), rec_size);
        }
    }
    Err(crate::error::RxError::FsCorrupt(format!("MFT record {rec_num} outside $MFT extents")))
}

fn reconstruct_path(
    dir_map: &HashMap<u64, (String, u64)>,
    parent_ref: Option<u64>,
    filename: &str,
) -> (String, bool) {
    let Some(mut cur) = parent_ref else {
        return (format!("/$OrphanFiles/{filename}"), false);
    };
    let mut parts = Vec::new();
    let mut depth = 0;
    while cur != 5 && depth < 64 {
        if let Some((dname, next_parent)) = dir_map.get(&cur) {
            parts.push(dname.clone());
            if *next_parent == cur {
                break;
            }
            cur = *next_parent;
            depth += 1;
        } else {
            return (format!("/$OrphanFiles/{filename}"), false);
        }
    }
    parts.reverse();
    if parts.is_empty() {
        (format!("/{filename}"), true)
    } else {
        (format!("/{}/{filename}", parts.join("/")), true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::common::candidate::{ReasonCode, RecoveryStatus};
    use crate::platform::image_reader::ImageReader;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn make_mft_record(in_use: bool, is_dir: bool, parent_ref: u64, name: &str, data_attr: &[u8]) -> Vec<u8> {
        let mut rec = vec![0u8; 1024];
        rec[0..4].copy_from_slice(b"FILE");
        rec[4..6].copy_from_slice(&48u16.to_le_bytes()); // USA off
        rec[6..8].copy_from_slice(&3u16.to_le_bytes());  // USA count (USN + 2 sectors)
        rec[20..22].copy_from_slice(&56u16.to_le_bytes()); // first attr off
        let mut flags: u16 = 0;
        if in_use { flags |= 1; }
        if is_dir { flags |= 2; }
        rec[22..24].copy_from_slice(&flags.to_le_bytes());
        // USN = 0x0001
        rec[48..50].copy_from_slice(&1u16.to_le_bytes());
        rec[510..512].copy_from_slice(&1u16.to_le_bytes());
        rec[1022..1024].copy_from_slice(&1u16.to_le_bytes());

        let mut off = 56usize;
        // $FILE_NAME attribute (0x30)
        let name_u16: Vec<u16> = name.encode_utf16().collect();
        let fn_val_len = 66 + name_u16.len() * 2;
        let fn_attr_len = ((24 + fn_val_len + 7) / 8) * 8;
        rec[off..off + 4].copy_from_slice(&0x30u32.to_le_bytes());
        rec[off + 4..off + 8].copy_from_slice(&(fn_attr_len as u32).to_le_bytes());
        rec[off + 16..off + 20].copy_from_slice(&(fn_val_len as u32).to_le_bytes());
        rec[off + 20..off + 22].copy_from_slice(&24u16.to_le_bytes());
        rec[off + 24..off + 32].copy_from_slice(&parent_ref.to_le_bytes());
        rec[off + 24 + 64] = name_u16.len() as u8;
        rec[off + 24 + 65] = 1; // Win32
        for (i, ch) in name_u16.iter().enumerate() {
            rec[off + 24 + 66 + i * 2..off + 24 + 66 + i * 2 + 2].copy_from_slice(&ch.to_le_bytes());
        }
        off += fn_attr_len;

        if !data_attr.is_empty() {
            rec[off..off + data_attr.len()].copy_from_slice(data_attr);
            off += data_attr.len();
        }
        rec[off..off + 4].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        rec
    }

    fn make_nonres_data_attr(real_size: u64, start_lcn: u8, clusters: u8) -> Vec<u8> {
        let mut a = vec![0u8; 72];
        a[0..4].copy_from_slice(&0x80u32.to_le_bytes());
        a[4..8].copy_from_slice(&72u32.to_le_bytes());
        a[8] = 1; // non-resident
        a[32..34].copy_from_slice(&64u16.to_le_bytes()); // runlist off
        a[48..56].copy_from_slice(&real_size.to_le_bytes());
        a[64] = 0x11; // 1-byte len, 1-byte lcn delta
        a[65] = clusters;
        a[66] = start_lcn;
        a[67] = 0x00;
        a
    }

    fn make_res_data_attr(payload: &[u8]) -> Vec<u8> {
        let attr_len = ((24 + payload.len() + 7) / 8) * 8;
        let mut a = vec![0u8; attr_len];
        a[0..4].copy_from_slice(&0x80u32.to_le_bytes());
        a[4..8].copy_from_slice(&(attr_len as u32).to_le_bytes());
        a[16..20].copy_from_slice(&(payload.len() as u32).to_le_bytes());
        a[20..22].copy_from_slice(&24u16.to_le_bytes());
        a[24..24 + payload.len()].copy_from_slice(payload);
        a
    }

    #[test]
    fn ntfs_golden_recovers_intact_and_flags_overwritten_and_trimmed() {
        let cluster = 4096usize;
        let mut img = vec![0u8; cluster * 40];
        // Boot sector
        img[3..11].copy_from_slice(b"NTFS    ");
        img[11..13].copy_from_slice(&512u16.to_le_bytes());
        img[13] = 8; // 4096 bytes/cluster
        img[40..48].copy_from_slice(&(40u64 * 8).to_le_bytes());
        img[48..56].copy_from_slice(&2u64.to_le_bytes()); // $MFT at cluster 2
        img[64] = (-10i8) as u8; // 1024 byte MFT records

        // Cluster 2..9 = $MFT (32 records = 8 clusters)
        let mft_base = 2 * cluster;
        let mft0 = make_mft_record(true, false, 5, "$MFT", &make_nonres_data_attr(32 * 1024, 2, 8));
        img[mft_base..mft_base + 1024].copy_from_slice(&mft0);

        // Record 6 = $Bitmap at cluster 12 (mark cluster 22 as allocated/overwritten!)
        let bm6 = make_mft_record(true, false, 5, "$Bitmap", &make_nonres_data_attr(64, 12, 1));
        img[mft_base + 6 * 1024..mft_base + 7 * 1024].copy_from_slice(&bm6);
        img[12 * cluster + (22 / 8)] |= 1 << (22 % 8); // cluster 22 allocated!

        // Record 16 = Deleted directory "Docs" under root (5)
        let dir16 = make_mft_record(true, true, 5, "Docs", &[]);
        img[mft_base + 16 * 1024..mft_base + 17 * 1024].copy_from_slice(&dir16);

        // Record 17 = Deleted intact JPEG at cluster 20
        let jpg_attr = make_nonres_data_attr(128, 20, 1);
        let rec17 = make_mft_record(false, false, 16, "holiday.jpg", &jpg_attr);
        img[mft_base + 17 * 1024..mft_base + 18 * 1024].copy_from_slice(&rec17);
        img[20 * cluster..20 * cluster + 6].copy_from_slice(&[0xFF, 0xD8, 0xFF, 0xE0, 0x11, 0x22]);

        // Record 18 = Deleted overwritten file at cluster 22
        let ov_attr = make_nonres_data_attr(512, 22, 1);
        let rec18 = make_mft_record(false, false, 16, "reused.pdf", &ov_attr);
        img[mft_base + 18 * 1024..mft_base + 19 * 1024].copy_from_slice(&rec18);
        img[22 * cluster..22 * cluster + 4].copy_from_slice(b"%PDF");

        // Record 19 = Deleted TRIMmed/zeroed file at cluster 24 (all zeros)
        let trim_attr = make_nonres_data_attr(1024, 24, 1);
        let rec19 = make_mft_record(false, false, 16, "trimmed.jpg", &trim_attr);
        img[mft_base + 19 * 1024..mft_base + 20 * 1024].copy_from_slice(&rec19);

        // Record 20 = Deleted resident tiny text file
        let res_attr = make_res_data_attr(b"secret note");
        let rec20 = make_mft_record(false, false, 16, "note.txt", &res_attr);
        img[mft_base + 20 * 1024..mft_base + 21 * 1024].copy_from_slice(&rec20);

        let mut tmp = NamedTempFile::new().unwrap();
        tmp.write_all(&img).unwrap();
        let reader: Arc<dyn RawReader> = Arc::new(ImageReader::open(tmp.path(), 512).unwrap());
        let vol = NtfsVolume::open(reader, "testdev".into(), MediaKind::Ssd, 16).unwrap();
        let cands = vol.scan_deleted(&CancelToken::new(), None).unwrap();

        assert_eq!(cands.len(), 4);
        let by_name: HashMap<_, _> = cands.iter().map(|c| (c.name_display.as_str(), c)).collect();
        assert_eq!(by_name["holiday.jpg"].status, RecoveryStatus::Intact);
        assert_eq!(by_name["holiday.jpg"].path_display, "/Docs/holiday.jpg");
        assert_eq!(by_name["reused.pdf"].status, RecoveryStatus::Unrecoverable);
        assert_eq!(by_name["reused.pdf"].reason_code, ReasonCode::Overwritten);
        assert_eq!(by_name["trimmed.jpg"].status, RecoveryStatus::Unrecoverable);
        assert_eq!(by_name["trimmed.jpg"].reason_code, ReasonCode::TrimOrWipe);
        assert_eq!(by_name["note.txt"].status, RecoveryStatus::Intact);
        assert_eq!(by_name["note.txt"].resident_data.as_deref(), Some(b"secret note".as_slice()));
    }
}
