use crate::core::cancel::CancelToken;
use crate::core::hashing::Sha256Stream;
use crate::core::types::ByteOffset;
use crate::error::{RxError, RxResult};
use crate::export::manifest::ExportAuditRecord;
use crate::export::naming::safe_export_path;
use crate::fs::common::candidate::RecoverCandidate;
use crate::fs::common::extent::ExtentKind;
use crate::platform::path_safety::same_physical_device;
use crate::platform::raw_reader::{read_logical_slice, RawReader};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::Path;

/// Safely exports selected candidates to `dest_dir` with [LAW-3] device guard,
/// single-pass SHA-256 hashing [LAW-8][P-14], and atomic `.part` -> rename [LAW-10].
pub fn export_candidates(
    source_path: &Path,
    reader: &dyn RawReader,
    candidates: &[RecoverCandidate],
    dest_dir: &Path,
    cancel: &CancelToken,
) -> RxResult<Vec<ExportAuditRecord>> {
    if same_physical_device(source_path, dest_dir)? {
        return Err(RxError::DestOnSource);
    }
    fs::create_dir_all(dest_dir)?;

    let mut records = Vec::with_capacity(candidates.len());
    let manifest_path = dest_dir.join("manifest.jsonl");
    let mut manifest_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&manifest_path)?;

    for cand in candidates {
        cancel.check()?;
        let final_path = safe_export_path(dest_dir, &cand.path_display, &cand.name_display);
        if let Some(parent) = final_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let part_path = final_path.with_extension("part");

        let res = export_one(reader, cand, &part_path, cancel);
        match res {
            Ok((bytes_written, src_hash, exp_hash, io_errs, sparse)) => {
                fs::rename(&part_path, &final_path)?;
                let rec = ExportAuditRecord {
                    id_hex: cand.id_hex.clone(),
                    name_display: cand.name_display.clone(),
                    path_display: cand.path_display.clone(),
                    exported_path: final_path.to_string_lossy().into_owned(),
                    logical_size: cand.logical_size,
                    bytes_written,
                    status: cand.status,
                    reason_code: cand.reason_code,
                    confidence: cand.confidence,
                    sha256_source_region: src_hash,
                    sha256_exported: exp_hash,
                    extents: cand.extents.clone(),
                    io_error_ranges: io_errs,
                    sparse_ranges: sparse,
                };
                writeln!(manifest_file, "{}", serde_json::to_string(&rec).unwrap_or_default())?;
                records.push(rec);
            }
            Err(e) => {
                let _ = fs::remove_file(&part_path);
                return Err(e);
            }
        }
    }
    manifest_file.sync_all()?;
    Ok(records)
}

fn export_one(
    reader: &dyn RawReader,
    cand: &RecoverCandidate,
    part_path: &Path,
    cancel: &CancelToken,
) -> RxResult<(u64, String, String, Vec<(u64, u64)>, Vec<(u64, u64)>)> {
    let mut out = File::create(part_path)?;
    let mut src_hash = Sha256Stream::new();
    let mut exp_hash = Sha256Stream::new();
    let mut written: u64 = 0;
    let mut io_errs = Vec::new();
    let mut sparse = Vec::new();

    if let Some(ref res) = cand.resident_data {
        let take = res.len().min(cand.logical_size as usize);
        src_hash.update(&res[..take]);
        exp_hash.update(&res[..take]);
        out.write_all(&res[..take])?;
        written = take as u64;
        out.sync_all()?;
        return Ok((written, src_hash.finalize_hex(), exp_hash.finalize_hex(), io_errs, sparse));
    }

    let target_len = cand.logical_size;
    let mut remaining = target_len;
    for ext in &cand.extents {
        if remaining == 0 {
            break;
        }
        let mut ext_rem = ext.len.min(remaining);
        let mut ext_pos = 0u64;
        while ext_rem > 0 {
            cancel.check()?;
            let step = ext_rem.min(1024 * 1024) as usize;
            match ext.kind {
                ExtentKind::Data => {
                    match read_logical_slice(reader, ByteOffset(ext.physical_off + ext_pos), step) {
                        Ok(chunk) => {
                            src_hash.update(&chunk);
                            exp_hash.update(&chunk);
                            out.write_all(&chunk)?;
                            written += chunk.len() as u64;
                        }
                        Err(_) => {
                            let zeros = vec![0u8; step];
                            exp_hash.update(&zeros);
                            out.write_all(&zeros)?;
                            io_errs.push((ext.logical_off + ext_pos, step as u64));
                            written += step as u64;
                        }
                    }
                }
                ExtentKind::SparseHole | ExtentKind::IoError | ExtentKind::Resident => {
                    let zeros = vec![0u8; step];
                    src_hash.update(&zeros);
                    exp_hash.update(&zeros);
                    out.write_all(&zeros)?;
                    sparse.push((ext.logical_off + ext_pos, step as u64));
                    written += step as u64;
                }
            }
            ext_pos += step as u64;
            ext_rem -= step as u64;
            remaining -= step as u64;
        }
    }

    out.sync_all()?;
    Ok((written, src_hash.finalize_hex(), exp_hash.finalize_hex(), io_errs, sparse))
}
