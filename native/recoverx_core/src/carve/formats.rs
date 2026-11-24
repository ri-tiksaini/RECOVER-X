use crate::core::types::ByteOffset;
use crate::platform::raw_reader::{read_logical_slice, RawReader};

/// Refines extension from container subtypes (OOXML, RIFF WAV/AVI/WEBP, ISO BMFF MP4/MOV/HEIC/M4A).
/// Returns None if a container header fails subtype validation (prevents false positives).
pub fn refine_extension(kind: &str, default_ext: &str, sample: &[u8]) -> Option<String> {
    match kind {
        "zip_ooxml" => {
            if find_subslice(sample, b"word/document.xml").is_some() {
                Some("docx".into())
            } else if find_subslice(sample, b"xl/workbook.xml").is_some() {
                Some("xlsx".into())
            } else if find_subslice(sample, b"ppt/presentation.xml").is_some() {
                Some("pptx".into())
            } else {
                Some("zip".into())
            }
        }
        "riff" => {
            if sample.len() < 12 {
                return None;
            }
            match &sample[8..12] {
                b"WAVE" => Some("wav".into()),
                b"AVI " => Some("avi".into()),
                b"WEBP" => Some("webp".into()),
                _ => None,
            }
        }
        "isobmff" => {
            if sample.len() < 12 {
                return None;
            }
            match &sample[8..12] {
                b"heic" | b"heix" | b"mif1" | b"msf1" => Some("heic".into()),
                b"qt  " => Some("mov".into()),
                b"M4A " => Some("m4a".into()),
                _ => Some("mp4".into()),
            }
        }
        "mkv" => {
            if find_subslice(sample, b"webm").is_some() {
                Some("webm".into())
            } else {
                Some("mkv".into())
            }
        }
        _ => Some(default_ext.to_string()),
    }
}

/// Computes exact structural file size directly from `RawReader` (supports multi-GB MP4/MOV/AVI/WAV).
/// Returns `Some((size_bytes, structural_valid))` for container formats, or `None` for footer-based formats.
pub fn measure_structural_size(
    reader: &dyn RawReader,
    kind: &str,
    phys_off: u64,
    max_size: u64,
) -> Option<(u64, bool)> {
    let vol_rem = reader.size().saturating_sub(phys_off);
    let cap = max_size.min(vol_rem);
    match kind {
        "bmp" => {
            let hdr = read_logical_slice(reader, ByteOffset(phys_off), 14).ok()?;
            if hdr.len() < 14 || &hdr[0..2] != b"BM" {
                return None;
            }
            let sz = u32::from_le_bytes([hdr[2], hdr[3], hdr[4], hdr[5]]) as u64;
            let data_off = u32::from_le_bytes([hdr[10], hdr[11], hdr[12], hdr[13]]) as u64;
            if sz >= 54 && sz <= cap && data_off >= 54 && data_off < sz {
                Some((sz, true))
            } else {
                None
            }
        }
        "riff" => {
            let hdr = read_logical_slice(reader, ByteOffset(phys_off), 12).ok()?;
            if hdr.len() < 12 || &hdr[0..4] != b"RIFF" {
                return None;
            }
            let riff_payload = u32::from_le_bytes([hdr[4], hdr[5], hdr[6], hdr[7]]) as u64;
            let total = riff_payload.saturating_add(8);
            if total >= 12 && total <= cap {
                Some((total, true))
            } else {
                None
            }
        }
        "isobmff" => walk_isobmff_boxes(reader, phys_off, cap),
        _ => None,
    }
}

/// Walks top-level ISO BMFF boxes (ftyp, moov, mdat, free, meta, etc.) with O(boxes) tiny reads.
/// Recovers full multi-GB MP4/MOV/HEIC videos without truncating at a 1-4 MiB buffer boundary.
fn walk_isobmff_boxes(reader: &dyn RawReader, phys_off: u64, cap: u64) -> Option<(u64, bool)> {
    let mut rel: u64 = 0;
    let mut box_count = 0usize;
    let mut saw_payload_box = false;

    while rel + 8 <= cap && box_count < 256 {
        let hdr = read_logical_slice(reader, ByteOffset(phys_off + rel), 16).ok()?;
        if hdr.len() < 8 {
            break;
        }
        let sz32 = u32::from_be_bytes([hdr[0], hdr[1], hdr[2], hdr[3]]) as u64;
        let btype = &hdr[4..8];

        // First box must be 'ftyp'
        if box_count == 0 && btype != b"ftyp" {
            return None;
        }
        // Subsequent boxes must have printable ASCII FourCC
        if !btype.iter().all(|&b| (0x20..=0x7E).contains(&b)) {
            break;
        }
        if matches!(btype, b"moov" | b"mdat" | b"meta" | b"moof") {
            saw_payload_box = true;
        }

        let box_size = if sz32 == 1 {
            if hdr.len() < 16 {
                break;
            }
            u64::from_be_bytes([
                hdr[8], hdr[9], hdr[10], hdr[11], hdr[12], hdr[13], hdr[14], hdr[15],
            ])
        } else if sz32 == 0 {
            // Box extends to EOF (rare in raw disk carving -> stop cleanly)
            break;
        } else {
            sz32
        };

        if box_size < 8 || rel.saturating_add(box_size) > cap {
            break;
        }
        rel += box_size;
        box_count += 1;
    }

    if box_count >= 2 && saw_payload_box {
        Some((rel, true))
    } else if box_count >= 1 && rel >= 16 {
        Some((rel, false))
    } else {
        None
    }
}

pub fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}
