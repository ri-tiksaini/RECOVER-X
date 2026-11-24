use crate::core::cancel::CancelToken;
use crate::core::endian::{read_le_u16, read_le_u32};
use crate::core::time::ext4_to_utc_ns;
use crate::core::types::{ByteOffset, CandidateId};
use crate::error::RxResult;
use crate::fs::common::candidate::{CandidateOrigin, FsKind, RecoverCandidate};
use crate::fs::common::confidence::{evaluate, ConfidenceInput};
use crate::fs::ext::extents::parse_inode_extents;
use crate::fs::ext::superblock::ExtSuperblock;
use crate::platform::raw_reader::{read_logical_slice, RawReader};
use crate::platform::trim::MediaKind;
use crate::validate::zero_scan;

pub fn scan_ext_deleted(
    reader: &dyn RawReader,
    sb: &ExtSuperblock,
    device_id: &str,
    media_kind: MediaKind,
    cancel: &CancelToken,
    max_candidates: Option<usize>,
) -> RxResult<Vec<RecoverCandidate>> {
    let gdt_off = if sb.block_size == 1024 { 2048 } else { sb.block_size };
    let mut out = Vec::new();

    for grp in 0..sb.group_count {
        cancel.check()?;
        if let Some(lim) = max_candidates {
            if out.len() >= lim {
                break;
            }
        }
        let gd_off = gdt_off + (grp as u64) * (sb.desc_size as u64);
        let Ok(gd) = read_logical_slice(reader, ByteOffset(gd_off), sb.desc_size) else {
            continue;
        };
        let mut itable_blk = read_le_u32(&gd, 8).unwrap_or(0) as u64;
        if sb.desc_size >= 64 {
            itable_blk |= (read_le_u32(&gd, 40).unwrap_or(0) as u64) << 32;
        }
        if itable_blk == 0 {
            continue;
        }

        let table_bytes = (sb.inodes_per_group as u64) * (sb.inode_size as u64);
        let base_off = itable_blk * sb.block_size;
        let mut pos = 0u64;
        while pos < table_bytes {
            cancel.check()?;
            let step = (table_bytes - pos).min(1024 * 1024) as usize;
            let Ok(buf) = read_logical_slice(reader, ByteOffset(base_off + pos), step) else {
                break;
            };
            for (idx, raw) in buf.chunks_exact(sb.inode_size).enumerate() {
                let ino_num = (grp as u64) * (sb.inodes_per_group as u64) + (pos / sb.inode_size as u64) + (idx as u64) + 1;
                if ino_num < 11 {
                    continue; // reserved system inodes
                }
                let mode = read_le_u16(raw, 0).unwrap_or(0);
                if (mode & 0xF000) != 0x8000 {
                    continue; // regular files only
                }
                let size_lo = read_le_u32(raw, 4).unwrap_or(0) as u64;
                let size_hi = if raw.len() >= 112 { read_le_u32(raw, 108).unwrap_or(0) as u64 } else { 0 };
                let size = (size_hi << 32) | size_lo;
                let dtime = read_le_u32(raw, 20).unwrap_or(0);
                let links = read_le_u16(raw, 26).unwrap_or(1);
                if links != 0 || dtime == 0 || size == 0 {
                    continue;
                }

                let flags = read_le_u32(raw, 32).unwrap_or(0);
                let uses_extents = (flags & 0x0008_0000) != 0;
                let extents = parse_inode_extents(reader, &raw[40..100], uses_extents, sb.block_size, size);
                if extents.is_empty() {
                    continue; // ext4 zeroed block pointers -> handled by carve pass
                }

                let first_off = extents[0].physical_off;
                let zr = zero_scan::region_zero_ratio(reader, &extents, size);
                let (status, reason, conf) = evaluate(&ConfidenceInput {
                    has_name: false,
                    has_parent_path: false,
                    runlist_intact: true,
                    overlap_ratio: 0.0,
                    zero_ratio: zr,
                    header_agreement: None,
                    structural_valid: true,
                    has_io_errors: false,
                    is_compressed: false,
                    is_encrypted: false,
                    high_entropy_no_header: false,
                    media_kind,
                });

                let name = format!("inode_{ino_num}.bin");
                let mtime = ext4_to_utc_ns(read_le_u32(raw, 16).unwrap_or(0) as i64, 0);
                let ctime = ext4_to_utc_ns(read_le_u32(raw, 12).unwrap_or(0) as i64, 0);
                let cid = CandidateId::derive(
                    device_id,
                    CandidateOrigin::Metadata as u8,
                    ino_num,
                    ByteOffset(first_off),
                    size,
                );
                out.push(RecoverCandidate {
                    id_hex: RecoverCandidate::id_to_hex(cid),
                    id_bytes: cid.0,
                    name_bytes: name.as_bytes().to_vec(),
                    name_display: name.clone(),
                    path_display: format!("/$OrphanInodes/{name}"),
                    logical_size: size,
                    first_byte_offset: first_off,
                    fs_kind: FsKind::Ext,
                    origin: CandidateOrigin::Metadata,
                    status,
                    reason_code: reason,
                    confidence: conf,
                    mtime_utc_ns: mtime,
                    ctime_utc_ns: ctime,
                    extents,
                    resident_data: None,
                    is_compressed: false,
                    is_encrypted: false,
                });
                if let Some(lim) = max_candidates {
                    if out.len() >= lim {
                        return Ok(out);
                    }
                }
            }
            pos += step as u64;
        }
    }
    Ok(out)
}
