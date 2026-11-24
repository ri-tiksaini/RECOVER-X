use crate::error::{RxError, RxResult};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Debug, Clone, Default)]
pub struct CancelToken {
    flag: Arc<AtomicBool>,
}

impl CancelToken {
    pub fn new() -> Self {
        Self { flag: Arc::new(AtomicBool::new(false)) }
    }

    #[inline]
    pub fn cancel(&self) {
        self.flag.store(true, Ordering::Release);
    }

    #[inline]
    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::Acquire)
    }

    #[inline]
    pub fn check(&self) -> RxResult<()> {
        if self.is_cancelled() {
            Err(RxError::Cancelled)
        } else {
            Ok(())
        }
    }
}
