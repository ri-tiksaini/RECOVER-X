use crate::core::types::ClusterId;
use crate::fs::common::bitmap_cache::PagedBitmapCache;
use crate::fs::common::extent::{Extent, ExtentKind};

/// Computes fraction of candidate clusters currently marked allocated in the FS bitmap [P-13].
pub fn compute_overlap_ratio(
    bitmap: Option<&PagedBitmapCache>,
    extents: &[Extent],
    cluster_size: u64,
    data_start_offset: u64,
) -> f64 {
    let Some(bm) = bitmap else {
        return 0.0;
    };
    if cluster_size == 0 {
        return 0.0;
    }
    let mut total_clusters: u64 = 0;
    let mut overlapped: u64 = 0;
    for ext in extents {
        if ext.kind != ExtentKind::Data || ext.len == 0 || ext.physical_off < data_start_offset {
            continue;
        }
        let first_c = (ext.physical_off - data_start_offset) / cluster_size;
        let count = ((ext.len - 1) / cluster_size) + 1;
        // Cap per-file sampling at 4096 clusters for listing speed [P-12]
        let step = (count / 4096).max(1);
        let mut c = 0;
        while c < count {
            total_clusters += 1;
            if let Ok(Some(true)) = bm.is_cluster_allocated(ClusterId(first_c + c)) {
                overlapped += 1;
            }
            c += step;
        }
    }
    if total_clusters == 0 {
        0.0
    } else {
        (overlapped as f64) / (total_clusters as f64)
    }
}
