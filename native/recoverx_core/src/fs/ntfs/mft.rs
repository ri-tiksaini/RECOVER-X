use crate::core::endian::{read_le_u16, read_le_u32, read_le_u64};
use crate::core::time::filetime_to_utc_ns;
use crate::error::{RxError, RxResult};
use crate::fs::common::extent::Extent;
use crate::fs::ntfs::dataruns::decode_dataruns;

#[derive(Debug, Clone)]
pub struct ParsedMftRecord {
    pub record_num: u64,
    pub in_use: bool,
    pub is_directory: bool,
    pub name: Option<String>,
    pub name_raw: Vec<u8>,
    pub name_namespace: u8,
    pub parent_mft_ref: Option<u64>,
    pub logical_size: u64,
    pub mtime_utc_ns: i64,
    pub ctime_utc_ns: i64,
    pub resident_data: Option<Vec<u8>>,
    pub extents: Vec<Extent>,
    pub child_mft_refs: Vec<u64>,
    pub is_compressed: bool,
    pub is_encrypted: bool,
}

/// Applies the NTFS Update Sequence Array (USA/fixup) in place before attribute parsing.
pub fn apply_usa_fixup(rec: &mut [u8]) -> RxResult<()> {
    if rec.len() < 48 || &rec[0..4] != b"FILE" {
        return Err(RxError::FsCorrupt("invalid MFT FILE signature".into()));
    }
    let usa_off = read_le_u16(rec, 4)? as usize;
    let usa_count = read_le_u16(rec, 6)? as usize;
    if usa_count == 0 || usa_off + usa_count * 2 > rec.len() {
        return Err(RxError::FsCorrupt("invalid MFT USA bounds".into()));
    }
    let usn0 = rec[usa_off];
    let usn1 = rec[usa_off + 1];
    for i in 1..usa_count {
        let end = i * 512;
        if end > rec.len() {
            break;
        }
        if rec[end - 2] == usn0 && rec[end - 1] == usn1 {
            rec[end - 2] = rec[usa_off + i * 2];
            rec[end - 1] = rec[usa_off + i * 2 + 1];
        }
    }
    Ok(())
}

pub fn parse_mft_record(
    record_num: u64,
    raw: &[u8],
    bytes_per_cluster: u64,
) -> RxResult<ParsedMftRecord> {
    let mut rec = raw.to_vec();
    apply_usa_fixup(&mut rec)?;

    let flags = read_le_u16(&rec, 22)?;
    let in_use = (flags & 0x0001) != 0;
    let is_directory = (flags & 0x0002) != 0;
    let mut off = read_le_u16(&rec, 20)? as usize;

    let mut out = ParsedMftRecord {
        record_num,
        in_use,
        is_directory,
        name: None,
        name_raw: Vec::new(),
        name_namespace: 2, // DOS=2 lowest preference
        parent_mft_ref: None,
        logical_size: 0,
        mtime_utc_ns: 0,
        ctime_utc_ns: 0,
        resident_data: None,
        extents: Vec::new(),
        child_mft_refs: Vec::new(),
        is_compressed: false,
        is_encrypted: false,
    };

    while off + 16 <= rec.len() {
        let attr_type = read_le_u32(&rec, off)?;
        if attr_type == 0xFFFF_FFFF {
            break;
        }
        let attr_len = read_le_u32(&rec, off + 4)? as usize;
        if attr_len < 16 || off + attr_len > rec.len() {
            break;
        }
        let non_resident = rec[off + 8] != 0;
        let name_len = rec[off + 9] as usize;
        let attr_flags = read_le_u16(&rec, off + 12)?;

        match attr_type {
            // $STANDARD_INFORMATION (0x10)
            0x10 if !non_resident => {
                let val_off = read_le_u16(&rec, off + 20)? as usize;
                let base = off + val_off;
                if base + 24 <= off + attr_len {
                    out.ctime_utc_ns = filetime_to_utc_ns(read_le_u64(&rec, base)?);
                    out.mtime_utc_ns = filetime_to_utc_ns(read_le_u64(&rec, base + 8)?);
                }
            }
            // $ATTRIBUTE_LIST (0x20) - resident entries for fragmented files / $MFT
            0x20 if !non_resident => {
                let val_len = read_le_u32(&rec, off + 16)? as usize;
                let val_off = read_le_u16(&rec, off + 20)? as usize;
                let mut cur = off + val_off;
                let end = (cur + val_len).min(off + attr_len);
                while cur + 26 <= end {
                    let entry_type = read_le_u32(&rec, cur)?;
                    let entry_len = read_le_u16(&rec, cur + 4)? as usize;
                    if entry_len < 26 || cur + entry_len > end {
                        break;
                    }
                    if entry_type == 0x80 {
                        let ref_raw = read_le_u64(&rec, cur + 16)?;
                        let child_ref = ref_raw & 0x0000_FFFF_FFFF_FFFF;
                        if child_ref != record_num && !out.child_mft_refs.contains(&child_ref) {
                            out.child_mft_refs.push(child_ref);
                        }
                    }
                    cur += entry_len;
                }
            }
            // $FILE_NAME (0x30)
            0x30 if !non_resident => {
                let val_off = read_le_u16(&rec, off + 20)? as usize;
                let base = off + val_off;
                if base + 66 <= off + attr_len {
                    let parent_raw = read_le_u64(&rec, base)?;
                    let parent_ref = parent_raw & 0x0000_FFFF_FFFF_FFFF;
                    let fn_len = rec[base + 64] as usize;
                    let ns = rec[base + 65];
                    let name_start = base + 66;
                    let name_end = name_start + fn_len * 2;
                    if name_end <= off + attr_len {
                        let raw_u16: Vec<u16> = rec[name_start..name_end]
                            .chunks_exact(2)
                            .map(|c| u16::from_le_bytes([c[0], c[1]]))
                            .collect();
                        let decoded = String::from_utf16_lossy(&raw_u16);
                        // Prefer Win32 (1) or Win32+DOS (3) over DOS-only (2)
                        let better = out.name.is_none() || (out.name_namespace == 2 && ns != 2);
                        if better {
                            out.name = Some(decoded.clone());
                            out.name_raw = decoded.into_bytes();
                            out.name_namespace = ns;
                            out.parent_mft_ref = Some(parent_ref);
                        }
                    }
                }
            }
            // $DATA (0x80) - unnamed default stream only
            0x80 if name_len == 0 => {
                if (attr_flags & 0x0001) != 0 {
                    out.is_compressed = true;
                }
                if (attr_flags & 0x4000) != 0 {
                    out.is_encrypted = true;
                }
                if !non_resident {
                    let val_len = read_le_u32(&rec, off + 16)? as usize;
                    let val_off = read_le_u16(&rec, off + 20)? as usize;
                    let start = off + val_off;
                    if start + val_len <= off + attr_len {
                        out.logical_size = val_len as u64;
                        out.resident_data = Some(rec[start..start + val_len].to_vec());
                    }
                } else if off + 56 <= rec.len() {
                    let run_off = read_le_u16(&rec, off + 32)? as usize;
                    let real_size = read_le_u64(&rec, off + 48)?;
                    if out.logical_size == 0 {
                        out.logical_size = real_size;
                    }
                    if off + run_off < off + attr_len {
                        if let Ok(runs) = decode_dataruns(&rec[off + run_off..off + attr_len], bytes_per_cluster) {
                            out.extents.extend(runs);
                        }
                    }
                }
            }
            _ => {}
        }
        off += attr_len;
    }

    Ok(out)
}
