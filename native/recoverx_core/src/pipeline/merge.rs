use crate::fs::common::candidate::{CandidateOrigin, RecoverCandidate};
use std::collections::HashSet;

/// Deduplicates METADATA vs CARVE candidates by physical start offset [LAW-4].
/// Always prefers METADATA (preserves original filename & folder path), marking overlap as Merged.
pub fn merge_candidates(
    mut metadata: Vec<RecoverCandidate>,
    carved: Vec<RecoverCandidate>,
) -> Vec<RecoverCandidate> {
    let carved_offsets: HashSet<u64> = carved.iter().map(|c| c.first_byte_offset).collect();
    let mut meta_offsets: HashSet<u64> = HashSet::with_capacity(metadata.len());

    for m in &mut metadata {
        meta_offsets.insert(m.first_byte_offset);
        if carved_offsets.contains(&m.first_byte_offset) {
            m.origin = CandidateOrigin::Merged;
        }
    }

    for c in carved {
        if !meta_offsets.contains(&c.first_byte_offset) {
            metadata.push(c);
        }
    }
    metadata
}
