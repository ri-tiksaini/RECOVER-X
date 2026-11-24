use crate::error::{RxError, RxResult};
use crate::fs::common::extent::{Extent, ExtentKind};

/// Decodes NTFS variable-length dataruns into physical byte Extents.
pub fn decode_dataruns(runlist: &[u8], bytes_per_cluster: u64) -> RxResult<Vec<Extent>> {
    let mut extents = Vec::new();
    let mut pos = 0usize;
    let mut cur_lcn: i64 = 0;
    let mut logical_off: u64 = 0;

    while pos < runlist.len() {
        let header = runlist[pos];
        if header == 0 {
            break;
        }
        pos += 1;
        let len_size = (header & 0x0F) as usize;
        let off_size = ((header >> 4) & 0x0F) as usize;
        if len_size == 0 || len_size > 8 || off_size > 8 || pos + len_size + off_size > runlist.len() {
            return Err(RxError::FsCorrupt("malformed NTFS datarun header".into()));
        }

        let mut cluster_count: u64 = 0;
        for i in 0..len_size {
            cluster_count |= (runlist[pos + i] as u64) << (i * 8);
        }
        pos += len_size;

        let run_bytes = cluster_count.saturating_mul(bytes_per_cluster);
        if off_size == 0 {
            extents.push(Extent {
                logical_off,
                physical_off: 0,
                len: run_bytes,
                kind: ExtentKind::SparseHole,
            });
            logical_off = logical_off.saturating_add(run_bytes);
            continue;
        }

        let mut delta: i64 = 0;
        for i in 0..off_size {
            delta |= (runlist[pos + i] as i64) << (i * 8);
        }
        // Sign-extend if MSB of off_size-th byte is set
        if off_size < 8 && (runlist[pos + off_size - 1] & 0x80) != 0 {
            delta |= !0i64 << (off_size * 8);
        }
        pos += off_size;

        cur_lcn = cur_lcn.saturating_add(delta);
        if cur_lcn < 0 {
            return Err(RxError::FsCorrupt("negative NTFS LCN in datarun".into()));
        }
        extents.push(Extent {
            logical_off,
            physical_off: (cur_lcn as u64).saturating_mul(bytes_per_cluster),
            len: run_bytes,
            kind: ExtentKind::Data,
        });
        logical_off = logical_off.saturating_add(run_bytes);
    }

    Ok(extents)
}
