use crate::fs::common::candidate::{ReasonCode, RecoveryStatus};
use crate::fs::common::extent::Extent;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportAuditRecord {
    pub id_hex: String,
    pub name_display: String,
    pub path_display: String,
    pub exported_path: String,
    pub logical_size: u64,
    pub bytes_written: u64,
    pub status: RecoveryStatus,
    pub reason_code: ReasonCode,
    pub confidence: u8,
    pub sha256_source_region: String,
    pub sha256_exported: String,
    pub extents: Vec<Extent>,
    pub io_error_ranges: Vec<(u64, u64)>,
    pub sparse_ranges: Vec<(u64, u64)>,
}
