use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MediaKind {
    Hdd,
    Ssd,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrimStatus {
    Enabled,
    Disabled,
    NotApplicable,
    Unknown,
}

pub fn classify_linux_block(dev_name: &str) -> (MediaKind, TrimStatus) {
    let sys = Path::new("/sys/block").join(dev_name);
    let rotational = fs::read_to_string(sys.join("queue/rotational"))
        .ok()
        .and_then(|s| s.trim().parse::<u8>().ok());

    let media = match rotational {
        Some(1) => MediaKind::Hdd,
        Some(0) => MediaKind::Ssd,
        _ => MediaKind::Unknown,
    };

    if media == MediaKind::Hdd {
        return (media, TrimStatus::NotApplicable);
    }

    let disc_max = fs::read_to_string(sys.join("queue/discard_max_bytes"))
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok());

    let trim = match disc_max {
        Some(1..) => TrimStatus::Enabled,
        Some(0) => TrimStatus::Unknown,
        None => TrimStatus::Unknown,
    };
    (media, trim)
}
