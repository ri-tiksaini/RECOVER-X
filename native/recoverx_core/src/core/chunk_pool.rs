use crate::core::cancel::CancelToken;
use crate::core::types::ByteOffset;
use crate::error::{RxError, RxResult};
use crossbeam_channel::{bounded, Receiver, RecvTimeoutError, Sender};
use std::alloc::{alloc_zeroed, dealloc, Layout};
use std::ptr::NonNull;
use std::time::Duration;

const PAGE_ALIGN: usize = 4096;

/// Page-aligned heap buffer compatible with O_DIRECT / FILE_FLAG_NO_BUFFERING.
pub struct AlignedBuf {
    ptr: NonNull<u8>,
    layout: Layout,
}

// SAFETY: AlignedBuf exclusively owns its heap allocation.
unsafe impl Send for AlignedBuf {}
unsafe impl Sync for AlignedBuf {}

impl AlignedBuf {
    pub fn new_zeroed(size: usize) -> RxResult<Self> {
        let aligned_size = ((size.max(PAGE_ALIGN) - 1) / PAGE_ALIGN + 1) * PAGE_ALIGN;
        let layout = Layout::from_size_align(aligned_size, PAGE_ALIGN)
            .map_err(|e| RxError::Internal(format!("invalid aligned layout: {e}")))?;
        // SAFETY: layout has non-zero size and power-of-two alignment.
        let raw = unsafe { alloc_zeroed(layout) };
        let ptr = NonNull::new(raw)
            .ok_or_else(|| RxError::Internal("OOM allocating aligned chunk".into()))?;
        Ok(Self { ptr, layout })
    }

    #[inline]
    pub fn capacity(&self) -> usize {
        self.layout.size()
    }

    #[inline]
    pub fn as_slice(&self) -> &[u8] {
        // SAFETY: ptr is valid for layout.size() bytes for the lifetime of self.
        unsafe { std::slice::from_raw_parts(self.ptr.as_ptr(), self.layout.size()) }
    }

    #[inline]
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        // SAFETY: unique &mut self guarantees exclusive access.
        unsafe { std::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.layout.size()) }
    }
}

impl Drop for AlignedBuf {
    fn drop(&mut self) {
        // SAFETY: allocated with the same layout in new_zeroed.
        unsafe { dealloc(self.ptr.as_ptr(), self.layout) };
    }
}

pub struct AlignedChunk {
    pub phys_offset: ByteOffset,
    pub len: usize,
    pub overlap_len: usize,
    pub buf: AlignedBuf,
}

/// Bounded SPMC aligned chunk pool with zero-alloc recycling [LAW-7][P-6].
pub struct ChunkPool {
    free_tx: Sender<AlignedChunk>,
    free_rx: Receiver<AlignedChunk>,
    ready_tx: Sender<AlignedChunk>,
    ready_rx: Receiver<AlignedChunk>,
}

impl ChunkPool {
    pub fn new(pool_depth: usize, chunk_bytes: usize) -> RxResult<Self> {
        let depth = pool_depth.max(2);
        let (free_tx, free_rx) = bounded(depth);
        let (ready_tx, ready_rx) = bounded(depth);
        for _ in 0..depth {
            let chunk = AlignedChunk {
                phys_offset: ByteOffset(0),
                len: 0,
                overlap_len: 0,
                buf: AlignedBuf::new_zeroed(chunk_bytes)?,
            };
            free_tx
                .send(chunk)
                .map_err(|_| RxError::Internal("chunk pool init failed".into()))?;
        }
        Ok(Self { free_tx, free_rx, ready_tx, ready_rx })
    }

    pub fn acquire_free(&self, cancel: &CancelToken) -> RxResult<AlignedChunk> {
        loop {
            cancel.check()?;
            match self.free_rx.recv_timeout(Duration::from_millis(50)) {
                Ok(c) => return Ok(c),
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => return Err(RxError::Cancelled),
            }
        }
    }

    pub fn publish_ready(&self, chunk: AlignedChunk, cancel: &CancelToken) -> RxResult<()> {
        let mut pending = Some(chunk);
        while let Some(c) = pending.take() {
            cancel.check()?;
            match self.ready_tx.send_timeout(c, Duration::from_millis(50)) {
                Ok(()) => return Ok(()),
                Err(crossbeam_channel::SendTimeoutError::Timeout(returned)) => {
                    pending = Some(returned);
                }
                Err(crossbeam_channel::SendTimeoutError::Disconnected(_)) => {
                    return Err(RxError::Cancelled);
                }
            }
        }
        Ok(())
    }

    pub fn worker_rx(&self) -> Receiver<AlignedChunk> {
        self.ready_rx.clone()
    }

    pub fn recycler_tx(&self) -> Sender<AlignedChunk> {
        self.free_tx.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn spmc_chunk_pool_recycles_cleanly() {
        let pool = ChunkPool::new(4, 4096).unwrap();
        let cancel = CancelToken::new();
        let rx = pool.worker_rx();
        let recycle = pool.recycler_tx();

        let worker = thread::spawn(move || {
            let mut sum: u64 = 0;
            for _ in 0..16 {
                let chunk = rx.recv().unwrap();
                sum += chunk.buf.as_slice()[0] as u64;
                recycle.send(chunk).unwrap();
            }
            sum
        });

        for i in 0..16u8 {
            let mut c = pool.acquire_free(&cancel).unwrap();
            c.buf.as_mut_slice()[0] = i;
            c.len = 4096;
            pool.publish_ready(c, &cancel).unwrap();
        }

        let sum = worker.join().unwrap();
        assert_eq!(sum, (0..16u64).sum::<u64>());
    }
}
