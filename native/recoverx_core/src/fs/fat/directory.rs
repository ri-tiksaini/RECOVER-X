use crate::core::cancel::CancelToken;
use crate::core::endian::{read_le_u16, read_le_u32};
use crate::core::time::fat_datetime_to_utc_ns;
use crate::core::types::{ByteOffset, CandidateId};
use crate::error::RxResult;
use crate::fs::common::candidate::{CandidateOrigin, FsKind, RecoverCandidate};
use crate::fs::common::confidence::{evaluate, ConfidenceInput};
use crate::fs::common::extent::{Extent, ExtentKind};
use crate::fs::fat::bpb::Fat32Bpb;
use crate::fs::fat::fat_table::{contiguous_overlap_ratio, follow_chain};
use crate::platform::raw_reader::{read_logical_slice, RawReader};
use crate::platform::trim::MediaKind;
use crate::validate::{header_match, zero_scan};

pub fn lfn_checksum(short_11: &[u8]) -> u8 {
    let mut sum: u8 = 0;
    for &b in short_11.iter().take(11) {
        sum = sum.rotate_right(1).wrapping_add(b);
    }
    sum
}

fn extract_lfn_fragment(entry: &[u8]) -> Vec<u16> {
    let offsets = [1, 3, 5, 7, 9, 14, 16, 18, 20, 22, 24, 28, 30];
    let mut out = Vec::with_capacity(13);
    for &o in &offsets {
        if o + 2 <= entry.len() {
            let ch = u16::from_le_bytes([entry[o], entry[o + 1]]);
            if ch == 0x0000 || ch == 0xFFFF {
                break;
            }
            out.push(ch);
        }
    }
    out
}

fn format_83_name(entry: &[u8]) -> String {
    let mut name: Vec<u8> = entry[0..8].to_vec();
    if name[0] == 0xE5 {
        name[0] = b'_';
    }
    let stem = String::from_utf8_lossy(&name).trim_end().to_string();
    let ext = String::from_utf8_lossy(&entry[8..11]).trim_end().to_string();
    if ext.is_empty() {
        stem
    } else {
        format!("{stem}.{ext}")
    }
}

pub fn scan_fat32_deleted(
    reader: &dyn RawReader,
    bpb: &Fat32Bpb,
    device_id: &str,
    media_kind: MediaKind,
    cancel: &CancelToken,
    max_candidates: Option<usize>,
) -> RxResult<Vec<RecoverCandidate>> {
    let mut out = Vec::new();
    let mut stack: Vec<(u32, String, usize)> = vec![(bpb.root_cluster, String::new(), 0)];

    while let Some((dir_cluster, dir_path, depth)) = stack.pop() {
        cancel.check()?;
        if depth > 32 {
            continue;
        }
        if let Some(lim) = max_candidates {
            if out.len() >= lim {
                break;
            }
        }
        let chain = follow_chain(reader, bpb, dir_cluster, 2048);
        let mut pending_lfn: Vec<(u8, Vec<u16>)> = Vec::new();

        for cl in chain {
            cancel.check()?;
            let Some(off) = bpb.cluster_to_offset(cl) else { continue };
            let Ok(buf) = read_logical_slice(reader, ByteOffset(off), bpb.bytes_per_cluster as usize) else {
                continue;
            };
            for ent in buf.chunks_exact(32) {
                if ent[0] == 0x00 {
                    break;
                }
                let attr = ent[11];
                if attr == 0x0F {
                    // LFN entry (active or deleted 0xE5)
                    pending_lfn.push((ent[13], extract_lfn_fragment(ent)));
                    continue;
                }
                if (attr & 0x08) != 0 {
                    pending_lfn.clear();
                    continue; // volume label
                }

                let is_deleted = ent[0] == 0xE5;
                let is_dir = (attr & 0x10) != 0;
                let hi = read_le_u16(ent, 20).unwrap_or(0) as u32;
                let lo = read_le_u16(ent, 26).unwrap_or(0) as u32;
                let start_cluster = (hi << 16) | lo;
                let size = read_le_u32(ent, 28).unwrap_or(0) as u64;

                let name = if !pending_lfn.is_empty() {
                    let mut utf16 = Vec::new();
                    for (_, frag) in pending_lfn.iter().rev() {
                        utf16.extend_from_slice(frag);
                    }
                    let s = String::from_utf16_lossy(&utf16);
                    if s.is_empty() { format_83_name(ent) } else { s }
                } else {
                    format_83_name(ent)
                };
                pending_lfn.clear();

                if name == "." || name == ".." {
                    continue;
                }
                let full_path = if dir_path.is_empty() {
                    format!("/{name}")
                } else {
                    format!("{dir_path}/{name}")
                };

                if is_dir && !is_deleted && start_cluster >= 2 {
                    stack.push((start_cluster, full_path, depth + 1));
                    continue;
                }

                if is_deleted && !is_dir && size > 0 && start_cluster >= 2 {
                    let Some(phys_off) = bpb.cluster_to_offset(start_cluster) else { continue };
                    let cluster_count = ((size - 1) / bpb.bytes_per_cluster) + 1;
                    let extents = vec![Extent {
                        logical_off: 0,
                        physical_off: phys_off,
                        len: cluster_count * bpb.bytes_per_cluster,
                        kind: ExtentKind::Data,
                    }];
                    let ov = contiguous_overlap_ratio(reader, bpb, start_cluster, cluster_count);
                    let zr = zero_scan::region_zero_ratio(reader, &extents, size);
                    let head = read_logical_slice(reader, ByteOffset(phys_off), size.min(512) as usize)
                        .unwrap_or_default();
                    let header_ok = header_match::check_header_agreement(&name, &head);
                    let (status, reason, conf) = evaluate(&ConfidenceInput {
                        has_name: true,
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

                    let mtime = fat_datetime_to_utc_ns(
                        read_le_u16(ent, 24).unwrap_or(0),
                        read_le_u16(ent, 22).unwrap_or(0),
                        0,
                    );
                    let ctime = fat_datetime_to_utc_ns(
                        read_le_u16(ent, 16).unwrap_or(0),
                        read_le_u16(ent, 14).unwrap_or(0),
                        ent[13],
                    );
                    let cid = CandidateId::derive(
                        device_id,
                        CandidateOrigin::Metadata as u8,
                        start_cluster as u64,
                        ByteOffset(phys_off),
                        size,
                    );
                    out.push(RecoverCandidate {
                        id_hex: RecoverCandidate::id_to_hex(cid),
                        id_bytes: cid.0,
                        name_bytes: name.as_bytes().to_vec(),
                        name_display: name,
                        path_display: full_path,
                        logical_size: size,
                        first_byte_offset: phys_off,
                        fs_kind: FsKind::Fat32,
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
                }
            }
        }
    }
    Ok(out)
}
