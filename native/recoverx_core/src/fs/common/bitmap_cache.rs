use crate::core::types::{ByteOffset, ClusterId};
use crate::error::RxResult;
use crate::fs::common::extent::{Extent, ExtentKind};
use crate::platform::raw_reader::{read_logical_slice, RawReader};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

const PAGE_BYTES: usize = 65_536; // 64 KiB pages -> 512 pages = 32 MiB max cap

struct CacheInner {
    pages: HashMap<u64, Vec<u8>>,
    order: VecDeque<u64>,
    max_pages: usize,
}

/// Paged LRU allocation bitmap cache bounded to 16-32 MiB even on 16 TB+ volumes [LAW-7][P-13].
pub struct PagedBitmapCache {
    reader: Arc<dyn RawReader>,
    bitmap_extents: Vec<Extent>,
    total_bitmap_bytes: u64,
    inner: Mutex<CacheInner>,
}

impl PagedBitmapCache {
    pub fn new(
        reader: Arc<dyn RawReader>,
        bitmap_extents: Vec<Extent>,
        total_bitmap_bytes: u64,
        max_cache_mb: usize,
    ) -> Self {
        let max_pages = ((max_cache_mb.max(1) * 1024 * 1024) / PAGE_BYTES).max(4);
        Self {
            reader,
            bitmap_extents,
            total_bitmap_bytes,
            inner: Mutex::new(CacheInner {
                pages: HashMap::new(),
                order: VecDeque::new(),
                max_pages,
            }),
        }
    }

    pub fn is_cluster_allocated(&self, cluster: ClusterId) -> RxResult<Option<bool>> {
        let byte_idx = cluster.0 / 8;
        if byte_idx >= self.total_bitmap_bytes {
            return Ok(None);
        }
        let bit_idx = (cluster.0 % 8) as u8;
        let page_idx = byte_idx / (PAGE_BYTES as u64);
        let in_page = (byte_idx % (PAGE_BYTES as u64)) as usize;

        if let Ok(guard) = self.inner.lock() {
            if let Some(page) = guard.pages.get(&page_idx) {
                if let Some(&b) = page.get(in_page) {
                    return Ok(Some(((b >> bit_idx) & 1) != 0));
                }
            }
        }

        let page_start = page_idx * (PAGE_BYTES as u64);
        let to_read = (self.total_bitmap_bytes - page_start).min(PAGE_BYTES as u64) as usize;
        let page_data = self.read_bitmap_logical(page_start, to_read)?;
        let bit = page_data.get(in_page).map(|&b| ((b >> bit_idx) & 1) != 0);

        if let Ok(mut guard) = self.inner.lock() {
            if guard.pages.len() >= guard.max_pages {
                if let Some(evict) = guard.order.pop_front() {
                    guard.pages.remove(&evict);
                }
            }
            guard.pages.insert(page_idx, page_data);
            guard.order.push_back(page_idx);
        }
        Ok(bit)
    }

    fn read_bitmap_logical(&self, logical_off: u64, len: usize) -> RxResult<Vec<u8>> {
        let mut out = vec![0u8; len];
        let mut written = 0usize;
        for ext in &self.bitmap_extents {
            if written >= len {
                break;
            }
            let ext_start = ext.logical_off;
            let ext_end = ext.logical_off.saturating_add(ext.len);
            let req_start = logical_off + written as u64;
            let req_end = logical_off + len as u64;
            if ext_end <= req_start || ext_start >= req_end {
                continue;
            }
            let overlap_start = req_start.max(ext_start);
            let overlap_end = req_end.min(ext_end);
            let chunk_len = (overlap_end - overlap_start) as usize;
            let dst_off = (overlap_start - logical_off) as usize;
            if ext.kind == ExtentKind::Data {
                let phys = ext.physical_off + (overlap_start - ext_start);
                let data = read_logical_slice(self.reader.as_ref(), ByteOffset(phys), chunk_len)?;
                let n = data.len().min(chunk_len);
                out[dst_off..dst_off + n].copy_from_slice(&data[..n]);
            }
            written += chunk_len;
        }
        Ok(out)
    }
}
