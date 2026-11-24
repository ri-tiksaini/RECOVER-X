use crate::fs::common::candidate::{ReasonCode, RecoveryStatus};
use crate::platform::trim::MediaKind;

#[derive(Debug, Clone, Copy)]
pub struct ConfidenceInput {
    pub has_name: bool,
    pub has_parent_path: bool,
    pub runlist_intact: bool,
    pub overlap_ratio: f64,
    pub zero_ratio: f64,
    pub header_agreement: Option<bool>,
    pub structural_valid: bool,
    pub has_io_errors: bool,
    pub is_compressed: bool,
    pub is_encrypted: bool,
    pub high_entropy_no_header: bool,
    pub media_kind: MediaKind,
}

/// Fail-closed status & confidence decision ladder (§9.6).
pub fn evaluate(inp: &ConfidenceInput) -> (RecoveryStatus, ReasonCode, u8) {
    if inp.is_encrypted {
        return (RecoveryStatus::Unrecoverable, ReasonCode::EncryptedHighEntropy, 10);
    }
    if inp.is_compressed {
        return (RecoveryStatus::Partial, ReasonCode::CompressedNtfs, 40);
    }
    if inp.zero_ratio >= 0.99 {
        let reason = if inp.media_kind == MediaKind::Ssd {
            ReasonCode::TrimOrWipe
        } else {
            ReasonCode::Zeroed
        };
        return (RecoveryStatus::Unrecoverable, reason, 0);
    }
    if inp.overlap_ratio >= 0.50 {
        return (RecoveryStatus::Unrecoverable, ReasonCode::Overwritten, 5);
    }
    if inp.high_entropy_no_header {
        return (RecoveryStatus::LowConfidence, ReasonCode::EncryptedHighEntropy, 20);
    }

    let header_ok = inp.header_agreement.unwrap_or(true);
    let mut score: i32 = 0;
    if inp.has_name { score += 20; }
    if inp.has_parent_path { score += 15; }
    if inp.runlist_intact { score += 25; }
    if header_ok { score += 20; }
    if inp.structural_valid { score += 20; }
    score -= (inp.overlap_ratio * 60.0) as i32;
    score -= (inp.zero_ratio * 50.0) as i32;
    if inp.has_io_errors { score -= 25; }
    let conf = score.clamp(0, 100) as u8;

    if !header_ok && !inp.structural_valid {
        return (RecoveryStatus::LowConfidence, ReasonCode::HeaderMismatch, conf.min(35));
    }
    if inp.overlap_ratio > 0.0 {
        return (RecoveryStatus::Partial, ReasonCode::Overwritten, conf.min(75));
    }
    if inp.zero_ratio > 0.0 {
        let reason = if inp.media_kind == MediaKind::Ssd {
            ReasonCode::TrimOrWipe
        } else {
            ReasonCode::Zeroed
        };
        return (RecoveryStatus::Partial, reason, conf.min(75));
    }
    if inp.has_io_errors {
        return (RecoveryStatus::Partial, ReasonCode::ShortReadIo, conf.min(70));
    }
    if !header_ok {
        return (RecoveryStatus::Partial, ReasonCode::HeaderMismatch, conf.min(60));
    }
    if !inp.structural_valid {
        return (RecoveryStatus::Partial, ReasonCode::FragmentedLost, conf.min(65));
    }
    (RecoveryStatus::Intact, ReasonCode::None, conf)
}
