use crate::core::align::split_unaligned;
use crate::core::cancel::CancelToken;
use crate::core::chunk_pool::AlignedBuf;
use crate::core::types::ByteOffset;
use crate::error::{RxError, RxResult};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

/// Strictly read-only block/image reader abstraction [LAW-1].
pub trait RawReader: Send + Sync {
    fn sector_size(&self) -> u32;
    fn size(&self) -> u64;
    fn read_at(&self, off: ByteOffset, buf: &mut [u8]) -> RxResult<usize>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BadSectorRange {
    pub offset: u64,
    pub length: u64,
    pub reason: String,
}

#[derive(Debug, Default)]
pub struct BadSectorMap {
    ranges: Mutex<Vec<BadSectorRange>>,
}

impl BadSectorMap {
    pub fn record(&self, offset: u64, length: u64, reason: impl Into<String>) {
        if let Ok(mut guard) = self.ranges.lock() {
            guard.push(BadSectorRange {
                offset,
                length,
                reason: reason.into(),
            });
        }
    }

    pub fn snapshot(&self) -> Vec<BadSectorRange> {
        self.ranges.lock().map(|g| g.clone()).unwrap_or_default()
    }
}

/// Reads an arbitrary logical slice using a 4096-byte page-aligned buffer (O_DIRECT safe) [P-4].
pub fn read_logical_slice(
    reader: &dyn RawReader,
    off: ByteOffset,
    len: usize,
) -> RxResult<Vec<u8>> {
    if len == 0 {
        return Ok(Vec::new());
    }
    let sector = reader.sector_size();
    let w = split_unaligned(off.0, len, sector);
    let mut aligned = AlignedBuf::new_zeroed(w.aligned_len)?;
    let got = reader.read_at(
        ByteOffset(w.aligned_off),
        &mut aligned.as_mut_slice()[..w.aligned_len],
    )?;
    let avail = got.saturating_sub(w.lead_skip).min(len);
    Ok(aligned.as_slice()[w.lead_skip..w.lead_skip + avail].to_vec())
}

/// Adaptive bad-sector skip-ahead (ddrescue-style): retries transient IO up to 2x,
/// then records exponential skip size (64 KiB -> 1 MiB -> 16 MiB).
pub fn read_with_adaptive_skip(
    reader: &dyn RawReader,
    off: ByteOffset,
    buf: &mut [u8],
    consecutive_errors: &mut u32,
    bad_map: &BadSectorMap,
    cancel: &CancelToken,
) -> RxResult<Option<usize>> {
    for attempt in 0..3u32 {
        cancel.check()?;
        match reader.read_at(off, buf) {
            Ok(n) => {
                *consecutive_errors = 0;
                return Ok(Some(n));
            }
            Err(RxError::IoRetryable { msg, .. }) if attempt < 2 => {
                tracing::warn!(offset = off.0, attempt, %msg, "transient read error, retrying");
                thread::sleep(Duration::from_millis(25 * (attempt as u64 + 1)));
            }
            Err(e) => {
                *consecutive_errors = consecutive_errors.saturating_add(1);
                let skip_bytes: u64 = match *consecutive_errors {
                    1 => 64 * 1024,
                    2 => 1024 * 1024,
                    _ => 16 * 1024 * 1024,
                };
                bad_map.record(off.0, skip_bytes.max(buf.len() as u64), e.to_string());
                return Ok(None);
            }
        }
    }
    Ok(None)
}
