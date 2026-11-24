use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivilegeInfo {
    pub is_elevated: bool,
    pub guidance: String,
}

pub fn check_privilege() -> PrivilegeInfo {
    #[cfg(unix)]
    {
        let elevated = unsafe { libc::geteuid() == 0 };
        let guidance = if elevated {
            "Read-only raw disk access enabled.".into()
        } else {
            "Run with sudo for raw disk access, or open a disk image (.dd/.raw).".into()
        };
        PrivilegeInfo { is_elevated: elevated, guidance }
    }
    #[cfg(windows)]
    {
        // Probing \\.\PhysicalDrive0 read-only succeeds only when elevated as Administrator.
        let elevated = std::fs::OpenOptions::new()
            .read(true)
            .write(false)
            .open(r"\\.\PhysicalDrive0")
            .is_ok();
        let guidance = if elevated {
            "Administrator read-only drive access enabled.".into()
        } else {
            "Right-click RECOVER-X.exe -> Run as administrator to scan drives.".into()
        };
        PrivilegeInfo { is_elevated: elevated, guidance }
    }
    #[cfg(not(any(unix, windows)))]
    {
        PrivilegeInfo { is_elevated: false, guidance: "Unsupported OS.".into() }
    }
}
