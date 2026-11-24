use crate::core::types::ByteOffset;
use crate::error::{RxError, RxResult};
use crate::platform::raw_reader::RawReader;
use std::fs::{File, OpenOptions};
use std::os::windows::fs::{FileExt, OpenOptionsExt};
use std::os::windows::io::AsRawHandle;
use std::path::Path;
use windows_sys::Win32::System::IO::DeviceIoControl;
use windows_sys::Win32::System::Ioctl::IOCTL_DISK_GET_LENGTH_INFO;

const FILE_SHARE_READ_WRITE: u32 = 0x0000_0001 | 0x0000_0002;

#[repr(C)]
struct GetLengthInformation {
    length: i64,
}

pub struct WindowsRawReader {
    file: File,
    sector_size: u32,
    size_bytes: u64,
}

impl WindowsRawReader {
    /// Opens a Windows volume (\\.\D:) or physical disk (\\.\PhysicalDrive0) strictly READ-ONLY [LAW-1].
    pub fn open(path: &Path) -> RxResult<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(false)
            .share_mode(FILE_SHARE_READ_WRITE)
            .open(path)?;

        let mut size_bytes = file.metadata().map(|m| m.len()).unwrap_or(0);
        if size_bytes == 0 {
            let mut info = GetLengthInformation { length: 0 };
            let mut returned: u32 = 0;
            // SAFETY: valid open file handle and properly sized output struct.
            let ok = unsafe {
                DeviceIoControl(
                    file.as_raw_handle(),
                    IOCTL_DISK_GET_LENGTH_INFO,
                    std::ptr::null(),
                    0,
                    (&mut info as *mut GetLengthInformation).cast(),
                    std::mem::size_of::<GetLengthInformation>() as u32,
                    &mut returned,
                    std::ptr::null_mut(),
                )
            };
            if ok != 0 && info.length > 0 {
                size_bytes = info.length as u64;
            }
        }

        Ok(Self {
            file,
            sector_size: 512,
            size_bytes,
        })
    }
}

impl RawReader for WindowsRawReader {
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
        self.file.seek_read(buf, off.0).map_err(|e| RxError::IoRetryable {
            offset: off.0,
            msg: e.to_string(),
        })
    }
}
