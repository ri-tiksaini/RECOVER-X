use crate::core::types::CandidateId;
use crate::fs::common::extent::Extent;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum FsKind {
    Unknown = 0,
    Ntfs = 1,
    Fat32 = 2,
    ExFat = 3,
    Ext = 4,
    Apfs = 5,
    HfsPlus = 6,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum CandidateOrigin {
    Metadata = 1,
    Carve = 2,
    Merged = 3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum RecoveryStatus {
    Intact = 1,
    Partial = 2,
    Unrecoverable = 3,
    LowConfidence = 4,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum ReasonCode {
    None = 0,
    TrimOrWipe = 1,
    Zeroed = 2,
    Overwritten = 3,
    FragmentedLost = 4,
    HeaderMismatch = 5,
    EncryptedHighEntropy = 6,
    ShortReadIo = 7,
    NameLost = 8,
    SparseHole = 9,
    UnknownFs = 10,
    CompressedNtfs = 11,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoverCandidate {
    pub id_hex: String,
    pub id_bytes: [u8; 16],
    pub name_bytes: Vec<u8>,
    pub name_display: String,
    pub path_display: String,
    pub logical_size: u64,
    pub first_byte_offset: u64,
    pub fs_kind: FsKind,
    pub origin: CandidateOrigin,
    pub status: RecoveryStatus,
    pub reason_code: ReasonCode,
    pub confidence: u8,
    pub mtime_utc_ns: i64,
    pub ctime_utc_ns: i64,
    pub extents: Vec<Extent>,
    pub resident_data: Option<Vec<u8>>,
    pub is_compressed: bool,
    pub is_encrypted: bool,
}

impl RecoverCandidate {
    pub fn id_to_hex(id: CandidateId) -> String {
        id.0.iter().map(|b| format!("{:02x}", b)).collect()
    }
}
