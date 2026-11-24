use crate::core::types::ByteOffset;
use crate::error::{RxError, RxResult};
use crate::platform::raw_reader::RawReader;
use std::fs::{File, OpenOptions};
use std::path::Path;

#[cfg(unix)]
use std::os::unix::fs::FileExt;
#[cfg(windows)]
use std::os::windows::fs::FileExt;

/// Read-only raw image reader (.dd/.raw/.img) using positioned IO (never mmap -> no SIGBUS).
pub struct ImageReader {
    file: File,
    sector_size: u32,
    size_bytes: u64,
}

impl ImageReader {
    pub fn open(path: &Path, sector_size: u32) -> RxResult<Self> {
        let file = OpenOptions::new().read(true).write(false).open(path)?;
        let size_bytes = file.metadata()?.len();
        Ok(Self {
            file,
            sector_size: if sector_size >= 512 && sector_size.is_power_of_two() {
                sector_size
            } else {
                512
            },
            size_bytes,
        })
    }
}

impl RawReader for ImageReader {
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
        #[cfg(unix)]
        let n = self.file.read_at(buf, off.0)?;
        #[cfg(windows)]
        let n = self.file.seek_read(buf, off.0)?;
        Ok(n)
    }
}
