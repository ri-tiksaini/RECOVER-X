use thiserror::Error;

pub type RxResult<T> = Result<T, RxError>;

#[derive(Debug, Error)]
pub enum RxError {
    #[error("retryable I/O error at offset {offset}: {msg}")]
    IoRetryable { offset: u64, msg: String },

    #[error("fatal I/O error: {0}")]
    IoFatal(String),

    #[error("short read at offset {offset}: expected {expected}, got {got}")]
    IoShortRead { offset: u64, expected: usize, got: usize },

    #[error("unaligned access: offset={offset}, len={len}, sector={sector}")]
    UnalignedAccess { offset: u64, len: usize, sector: u32 },

    #[error("short buffer: needed {needed} bytes at offset {offset}, len={buf_len}")]
    ShortBuffer { offset: usize, needed: usize, buf_len: usize },

    #[error("unknown or unsupported filesystem: {0}")]
    FsUnknown(String),

    #[error("corrupt filesystem or partition structure: {0}")]
    FsCorrupt(String),

    #[error("permission denied: {0}")]
    PermissionDenied(String),

    #[error("destination path is on the same physical device as source [LAW-3]")]
    DestOnSource,

    #[error("invalid destination path: {0}")]
    DestInvalid(String),

    #[error("operation cancelled [LAW-10]")]
    Cancelled,

    #[error("internal invariant violation: {0}")]
    Internal(String),
}

impl RxError {
    /// Stable integer error codes crossing language boundaries.
    pub fn code(&self) -> i32 {
        match self {
            RxError::IoRetryable { .. } => 1001,
            RxError::IoFatal(_) => 1002,
            RxError::IoShortRead { .. } => 1003,
            RxError::UnalignedAccess { .. } => 1004,
            RxError::ShortBuffer { .. } => 1005,
            RxError::FsUnknown(_) => 2001,
            RxError::FsCorrupt(_) => 2002,
            RxError::PermissionDenied(_) => 3001,
            RxError::DestOnSource => 4001,
            RxError::DestInvalid(_) => 4002,
            RxError::Cancelled => 5001,
            RxError::Internal(_) => 9001,
        }
    }

    pub fn is_retryable(&self) -> bool {
        matches!(self, RxError::IoRetryable { .. })
    }
}

impl From<std::io::Error> for RxError {
    fn from(err: std::io::Error) -> Self {
        use std::io::ErrorKind;
        match err.kind() {
            ErrorKind::PermissionDenied => RxError::PermissionDenied(err.to_string()),
            ErrorKind::Interrupted | ErrorKind::TimedOut | ErrorKind::WouldBlock => {
                RxError::IoRetryable { offset: 0, msg: err.to_string() }
            }
            _ => RxError::IoFatal(err.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn error_codes_are_unique() {
        let samples = vec![
            RxError::IoRetryable { offset: 0, msg: String::new() },
            RxError::IoFatal(String::new()),
            RxError::IoShortRead { offset: 0, expected: 1, got: 0 },
            RxError::UnalignedAccess { offset: 1, len: 1, sector: 512 },
            RxError::ShortBuffer { offset: 0, needed: 4, buf_len: 2 },
            RxError::FsUnknown(String::new()),
            RxError::FsCorrupt(String::new()),
            RxError::PermissionDenied(String::new()),
            RxError::DestOnSource,
            RxError::DestInvalid(String::new()),
            RxError::Cancelled,
            RxError::Internal(String::new()),
        ];
        let codes: HashSet<i32> = samples.iter().map(|e| e.code()).collect();
        assert_eq!(codes.len(), samples.len());
    }
}
