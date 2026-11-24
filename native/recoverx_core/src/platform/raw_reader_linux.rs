use crate::core::types::ByteOffset;
use crate::error::{RxError, RxResult};
use crate::platform::raw_reader::RawReader;
use std::ffi::CString;
use std::fs::File;
use std::os::unix::fs::FileExt;
use std::os::unix::io::FromRawFd;
use std::path::Path;

const BLKGETSIZE64: libc::Ioctl = 0x8008_1272u64 as libc::Ioctl;
const BLKSSZGET: libc::Ioctl = 0x1268u64 as libc::Ioctl;

pub struct LinuxRawReader {
    file: File,
    sector_size: u32,
    size_bytes: u64,
    direct_io: bool,
}

impl LinuxRawReader {
    /// Opens a Linux block device or file strictly READ-ONLY [LAW-1].
    /// Tries O_DIRECT first and falls back cleanly to buffered O_RDONLY on EINVAL.
    pub fn open(path: &Path) -> RxResult<Self> {
        let c_path = CString::new(path.to_string_lossy().as_bytes())
            .map_err(|_| RxError::IoFatal("invalid path containing NUL byte".into()))?;

        let mut direct_io = true;
        // SAFETY: read-only open(2) with NUL-terminated path.
        let mut fd = unsafe { libc::open(c_path.as_ptr(), libc::O_RDONLY | libc::O_DIRECT) };
        if fd < 0 {
            let err = std::io::Error::last_os_error();
            if err.raw_os_error() == Some(libc::EINVAL) {
                direct_io = false;
                // SAFETY: read-only open(2) fallback.
                fd = unsafe { libc::open(c_path.as_ptr(), libc::O_RDONLY) };
            }
        }
        if fd < 0 {
            return Err(std::io::Error::last_os_error().into());
        }

        // Verify read-only access mode [LAW-1].
        // SAFETY: fcntl F_GETFL on valid fd.
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if flags < 0 || (flags & libc::O_ACCMODE) != libc::O_RDONLY {
            unsafe { libc::close(fd) };
            return Err(RxError::Internal("LAW-1 violation: fd is not O_RDONLY".into()));
        }

        let mut size_bytes: u64 = 0;
        let mut sector_size: u32 = 512;

        // SAFETY: ioctl queries on valid fd; falls back to fstat length for regular files.
        let rc_sz = unsafe { libc::ioctl(fd, BLKGETSIZE64, &mut size_bytes) };
        let rc_ss = unsafe { libc::ioctl(fd, BLKSSZGET, &mut sector_size) };
        if rc_ss != 0 || sector_size < 512 || !sector_size.is_power_of_two() {
            sector_size = 512;
        }

        // SAFETY: fd is valid and owned by File now.
        let file = unsafe { File::from_raw_fd(fd) };
        if rc_sz != 0 {
            size_bytes = file.metadata()?.len();
        }

        Ok(Self {
            file,
            sector_size,
            size_bytes,
            direct_io,
        })
    }

    pub fn is_direct_io(&self) -> bool {
        self.direct_io
    }
}

impl RawReader for LinuxRawReader {
    fn sector_size(&self) -> u32 {
        self.sector_size
    }

    fn size(&self) -> u64 {
        self.size_bytes
    }

    fn read_at(&self, off: ByteOffset, buf: &mut [u8]) -> RxResult<usize> {
        let ss = self.sector_size as u64;
        if off.0 % ss != 0 || (buf.len() as u64) % ss != 0 {
            return Err(RxError::UnalignedAccess {
                offset: off.0,
                len: buf.len(),
                sector: self.sector_size,
            });
        }
        if off.0 >= self.size_bytes {
            return Ok(0);
        }
        self.file.read_at(buf, off.0).map_err(|e| RxError::IoRetryable {
            offset: off.0,
            msg: e.to_string(),
        })
    }
}
