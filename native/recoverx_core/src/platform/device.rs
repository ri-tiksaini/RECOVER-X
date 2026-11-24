use crate::error::RxResult;
use crate::platform::trim::{MediaKind, TrimStatus};
use serde::{Deserialize, Serialize};

#[cfg(unix)]
use crate::platform::trim::classify_linux_block;
#[cfg(unix)]
use std::fs;
#[cfg(unix)]
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionInfo {
    pub os_path: String,
    pub start_lba: u64,
    pub len_bytes: u64,
    pub mount_point: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub id: String,
    pub os_path: String,
    pub model: String,
    pub serial: String,
    pub size_bytes: u64,
    pub sector_size: u32,
    pub media_kind: MediaKind,
    pub trim_status: TrimStatus,
    pub mount_state: String,
    pub partitions: Vec<PartitionInfo>,
}

pub fn enumerate_devices() -> RxResult<Vec<DeviceInfo>> {
    #[cfg(unix)]
    {
        enumerate_linux()
    }
    #[cfg(windows)]
    {
        enumerate_windows()
    }
    #[cfg(not(any(unix, windows)))]
    {
        Ok(Vec::new())
    }
}

#[cfg(windows)]
fn enumerate_windows() -> RxResult<Vec<DeviceInfo>> {
    use crate::platform::raw_reader::RawReader;
    use crate::platform::raw_reader_windows::WindowsRawReader;
    use std::path::Path;
    use windows_sys::Win32::Storage::FileSystem::{GetDriveTypeW, GetLogicalDrives};

    let mask = unsafe { GetLogicalDrives() };
    let mut devices = Vec::new();

    for bit in 0..26u32 {
        if (mask & (1 << bit)) == 0 {
            continue;
        }
        let letter = (b'A' + bit as u8) as char;
        let root_w: Vec<u16> = format!("{letter}:\\\0").encode_utf16().collect();
        let dtype = unsafe { GetDriveTypeW(root_w.as_ptr()) };
        // 2 = DRIVE_REMOVABLE (USB), 3 = DRIVE_FIXED (HDD/SSD)
        if dtype != 2 && dtype != 3 {
            continue;
        }
        let vol_path = format!(r"\\.\{letter}:");
        let size_bytes = WindowsRawReader::open(Path::new(&vol_path))
            .map(|r| r.size())
            .unwrap_or(0);
        let label = if dtype == 2 {
            format!("Removable USB ({letter}:)")
        } else {
            format!("Local Disk ({letter}:)")
        };

        devices.push(DeviceInfo {
            id: format!("win_{letter}"),
            os_path: vol_path.clone(),
            model: label,
            serial: String::new(),
            size_bytes,
            sector_size: 512,
            media_kind: MediaKind::Unknown,
            trim_status: TrimStatus::Unknown,
            mount_state: "LiveMounted".into(),
            partitions: vec![PartitionInfo {
                os_path: vol_path,
                start_lba: 0,
                len_bytes: size_bytes,
                mount_point: Some(format!("{letter}:\\")),
            }],
        });
    }
    Ok(devices)
}

#[cfg(unix)]
fn enumerate_linux() -> RxResult<Vec<DeviceInfo>> {
    let mounts = fs::read_to_string("/proc/mounts").unwrap_or_default();
    let mut devices = Vec::new();
    let entries = match fs::read_dir("/sys/block") {
        Ok(e) => e,
        Err(_) => return Ok(devices),
    };

    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with("loop") || name.starts_with("ram") || name.starts_with("zram") {
            continue;
        }
        let sys = entry.path();
        let sectors = read_u64(&sys.join("size")).unwrap_or(0);
        if sectors == 0 {
            continue;
        }
        let sector_size = read_u64(&sys.join("queue/logical_block_size")).unwrap_or(512) as u32;
        let size_bytes = sectors * 512;
        let model = read_trimmed(&sys.join("device/model")).unwrap_or_else(|| name.clone());
        let serial = read_trimmed(&sys.join("device/serial")).unwrap_or_default();
        let (media_kind, trim_status) = classify_linux_block(&name);
        let os_path = format!("/dev/{name}");

        let mut partitions = Vec::new();
        let mut any_mounted = false;
        if let Ok(sub) = fs::read_dir(&sys) {
            for s in sub.flatten() {
                let sname = s.file_name().to_string_lossy().into_owned();
                if sname.starts_with(&name) && s.path().join("partition").exists() {
                    let start_lba = read_u64(&s.path().join("start")).unwrap_or(0);
                    let plen = read_u64(&s.path().join("size")).unwrap_or(0) * 512;
                    let ppath = format!("/dev/{sname}");
                    let mp = find_mount_point(&mounts, &ppath);
                    if mp.is_some() {
                        any_mounted = true;
                    }
                    partitions.push(PartitionInfo {
                        os_path: ppath,
                        start_lba,
                        len_bytes: plen,
                        mount_point: mp,
                    });
                }
            }
        }
        partitions.sort_by_key(|p| p.start_lba);

        devices.push(DeviceInfo {
            id: if serial.is_empty() { name.clone() } else { serial.clone() },
            os_path,
            model,
            serial,
            size_bytes,
            sector_size,
            media_kind,
            trim_status,
            mount_state: if any_mounted { "LiveMounted".into() } else { "Unmounted".into() },
            partitions,
        });
    }
    Ok(devices)
}

#[cfg(unix)]
fn read_trimmed(p: &Path) -> Option<String> {
    fs::read_to_string(p).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

#[cfg(unix)]
fn read_u64(p: &Path) -> Option<u64> {
    read_trimmed(p)?.parse::<u64>().ok()
}

#[cfg(unix)]
fn find_mount_point(mounts: &str, dev_path: &str) -> Option<String> {
    for line in mounts.lines() {
        let mut parts = line.split_whitespace();
        if let (Some(src), Some(dst)) = (parts.next(), parts.next()) {
            if src == dev_path {
                return Some(dst.to_string());
            }
        }
    }
    None
}
