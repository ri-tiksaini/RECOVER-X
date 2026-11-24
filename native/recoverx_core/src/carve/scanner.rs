use crate::carve::formats::{find_subslice, measure_structural_size, refine_extension};
use crate::carve::signature_db::default_signatures;
use crate::core::cancel::CancelToken;
use crate::core::chunk_pool::AlignedBuf;
use crate::core::types::{ByteOffset, CandidateId};
use crate::error::RxResult;
use crate::fs::common::candidate::{CandidateOrigin, FsKind, ReasonCode, RecoverCandidate, RecoveryStatus};
use crate::fs::common::extent::{Extent, ExtentKind};
use crate::platform::raw_reader::RawReader;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CarveAlignment {
    Sector,
    Byte,
}

/// Streams a RawReader in 4 MiB aligned chunks and carves files [P-11].
/// Defaults to 512-byte sector-aligned header checks (10-50x faster, avoids embedded thumbnails).
/// Supports full-size structural carving for MP4/MOV/HEIC (ISO BMFF boxes), WAV/AVI/WEBP (RIFF), and BMP.
pub fn carve_volume(
    reader: &dyn RawReader,
    device_id: &str,
    fs_kind: FsKind,
    alignment: CarveAlignment,
    cancel: &CancelToken,
    max_candidates: Option<usize>,
) -> RxResult<Vec<RecoverCandidate>> {
    let sigs = default_signatures();
    let step = match alignment {
        CarveAlignment::Sector => reader.sector_size().max(512) as usize,
        CarveAlignment::Byte => 1usize,
    };
    let chunk_bytes: usize = 4 * 1024 * 1024;
    let total = reader.size();
    let mut buf = AlignedBuf::new_zeroed(chunk_bytes)?;
    let mut out = Vec::new();
    let mut pos: u64 = 0;

    while pos < total {
        cancel.check()?;
        if let Some(lim) = max_candidates {
            if out.len() >= lim {
                break;
            }
        }
        let ss = reader.sector_size() as u64;
        let to_read = (total - pos).min(chunk_bytes as u64);
        let aligned_read = ((to_read / ss) * ss) as usize;
        if aligned_read == 0 {
            break;
        }
        let got = reader.read_at(ByteOffset(pos), &mut buf.as_mut_slice()[..aligned_read])?;
        if got == 0 {
            break;
        }
        let data = &buf.as_slice()[..got];

        let mut off = 0usize;
        while off + 16 <= data.len() {
            for sig in &sigs {
                let h_start = off + sig.header_offset;
                let h_end = h_start + sig.header.len();
                if h_end <= data.len() && &data[h_start..h_end] == sig.header {
                    let phys_off = pos + off as u64;
                    let tail = &data[off..];

                    let Some(ext) = refine_extension(sig.kind, sig.ext, &tail[..tail.len().min(4096)]) else {
                        continue;
                    };

                    let (est_size, structural_ok) = if matches!(sig.kind, "isobmff" | "riff" | "bmp") {
                        match measure_structural_size(reader, sig.kind, phys_off, sig.max_size) {
                            Some(res) => res,
                            None => continue, // false-positive header
                        }
                    } else if let Some(footer) = sig.footer {
                        if let Some(fpos) = find_subslice(tail, footer) {
                            let extra = if sig.kind == "zip_ooxml" { 22 } else { footer.len() };
                            ((fpos + extra).min(tail.len()) as u64, true)
                        } else {
                            (tail.len().min(sig.max_size as usize) as u64, false)
                        }
                    } else {
                        // Stream formats without global size header (MKV, MP3, FLAC, OGG)
                        let rem = total.saturating_sub(phys_off);
                        (rem.min(sig.max_size).min(16 * 1024 * 1024), false)
                    };

                    let name = format!("carved_{:012x}.{ext}", phys_off);
                    let cid = CandidateId::derive(
                        device_id,
                        CandidateOrigin::Carve as u8,
                        0,
                        ByteOffset(phys_off),
                        est_size,
                    );
                    let status = if structural_ok {
                        RecoveryStatus::Intact
                    } else {
                        RecoveryStatus::LowConfidence
                    };
                    let reason = if structural_ok {
                        ReasonCode::NameLost
                    } else {
                        ReasonCode::FragmentedLost
                    };
                    out.push(RecoverCandidate {
                        id_hex: RecoverCandidate::id_to_hex(cid),
                        id_bytes: cid.0,
                        name_bytes: name.as_bytes().to_vec(),
                        name_display: name.clone(),
                        path_display: format!("/$Carved/{ext}/{name}"),
                        logical_size: est_size,
                        first_byte_offset: phys_off,
                        fs_kind,
                        origin: CandidateOrigin::Carve,
                        status,
                        reason_code: reason,
                        confidence: if structural_ok { 75 } else { 40 },
                        mtime_utc_ns: 0,
                        ctime_utc_ns: 0,
                        extents: vec![Extent {
                            logical_off: 0,
                            physical_off: phys_off,
                            len: est_size,
                            kind: ExtentKind::Data,
                        }],
                        resident_data: None,
                        is_compressed: false,
                        is_encrypted: false,
                    });
                    if let Some(lim) = max_candidates {
                        if out.len() >= lim {
                            return Ok(out);
                        }
                    }
                    break;
                }
            }
            off += step;
        }
        pos += got as u64;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::image_reader::ImageReader;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn sector_aligned_carve_ignores_embedded_thumbnail() {
        let mut blob = vec![0u8; 8192];
        // Real JPEG at sector 2 (offset 1024)
        blob[1024..1030].copy_from_slice(&[0xFF, 0xD8, 0xFF, 0xE0, 0x11, 0x22]);
        blob[1200..1202].copy_from_slice(&[0xFF, 0xD9]);
        // Embedded thumbnail at non-sector offset 1077
        blob[1077..1083].copy_from_slice(&[0xFF, 0xD8, 0xFF, 0xDB, 0x33, 0x44]);

        let mut tmp = NamedTempFile::new().unwrap();
        tmp.write_all(&blob).unwrap();
        let r = ImageReader::open(tmp.path(), 512).unwrap();

        let sec_hits = carve_volume(&r, "img", FsKind::Unknown, CarveAlignment::Sector, &CancelToken::new(), None).unwrap();
        assert_eq!(sec_hits.len(), 1);
        assert_eq!(sec_hits[0].first_byte_offset, 1024);

        let byte_hits = carve_volume(&r, "img", FsKind::Unknown, CarveAlignment::Byte, &CancelToken::new(), None).unwrap();
        assert_eq!(byte_hits.len(), 2);
    }

    #[test]
    fn carves_structural_mp4_wav_webp_and_heic() {
        let mut blob = vec![0u8; 16384];

        // 1. MP4 at offset 512: ftyp (24B) + mdat (1000B) + moov (64B) = 1088 bytes exact
        let mp4 = 512usize;
        blob[mp4..mp4 + 4].copy_from_slice(&24u32.to_be_bytes());
        blob[mp4 + 4..mp4 + 12].copy_from_slice(b"ftypisom");
        blob[mp4 + 24..mp4 + 28].copy_from_slice(&1000u32.to_be_bytes());
        blob[mp4 + 28..mp4 + 32].copy_from_slice(b"mdat");
        blob[mp4 + 1024..mp4 + 1028].copy_from_slice(&64u32.to_be_bytes());
        blob[mp4 + 1028..mp4 + 1032].copy_from_slice(b"moov");

        // 2. WAV at offset 4096: RIFF payload 400 -> total 408 bytes exact
        let wav = 4096usize;
        blob[wav..wav + 4].copy_from_slice(b"RIFF");
        blob[wav + 4..wav + 8].copy_from_slice(&400u32.to_le_bytes());
        blob[wav + 8..wav + 12].copy_from_slice(b"WAVE");

        // 3. WEBP at offset 6144: RIFF payload 200 -> total 208 bytes exact
        let webp = 6144usize;
        blob[webp..webp + 4].copy_from_slice(b"RIFF");
        blob[webp + 4..webp + 8].copy_from_slice(&200u32.to_le_bytes());
        blob[webp + 8..webp + 12].copy_from_slice(b"WEBP");

        // 4. HEIC photo at offset 8192: ftyp (20B) + meta (120B) = 140 bytes exact
        let heic = 8192usize;
        blob[heic..heic + 4].copy_from_slice(&20u32.to_be_bytes());
        blob[heic + 4..heic + 12].copy_from_slice(b"ftypheic");
        blob[heic + 20..heic + 24].copy_from_slice(&120u32.to_be_bytes());
        blob[heic + 24..heic + 28].copy_from_slice(b"meta");

        let mut tmp = NamedTempFile::new().unwrap();
        tmp.write_all(&blob).unwrap();
        let r = ImageReader::open(tmp.path(), 512).unwrap();

        let hits = carve_volume(&r, "media_img", FsKind::Unknown, CarveAlignment::Sector, &CancelToken::new(), None).unwrap();
        assert_eq!(hits.len(), 4);
        assert!(hits[0].name_display.ends_with(".mp4"));
        assert_eq!(hits[0].logical_size, 1088);
        assert_eq!(hits[0].status, RecoveryStatus::Intact);

        assert!(hits[1].name_display.ends_with(".wav"));
        assert_eq!(hits[1].logical_size, 408);

        assert!(hits[2].name_display.ends_with(".webp"));
        assert_eq!(hits[2].logical_size, 208);

        assert!(hits[3].name_display.ends_with(".heic"));
        assert_eq!(hits[3].logical_size, 140);
    }
}
