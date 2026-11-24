use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(transparent)]
pub struct SectorSize(pub u32);

impl SectorSize {
    pub fn new(bytes: u32) -> Option<Self> {
        if bytes >= 512 && bytes.is_power_of_two() {
            Some(Self(bytes))
        } else {
            None
        }
    }
    #[inline]
    pub fn as_u64(self) -> u64 {
        self.0 as u64
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(transparent)]
pub struct ByteOffset(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(transparent)]
pub struct Sector(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(transparent)]
pub struct ClusterId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(transparent)]
pub struct MftRef(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(transparent)]
pub struct InodeRef(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(transparent)]
pub struct CandidateId(pub [u8; 16]);

impl Sector {
    #[inline]
    pub fn to_byte_offset(self, sector_size: SectorSize) -> Option<ByteOffset> {
        self.0.checked_mul(sector_size.as_u64()).map(ByteOffset)
    }
}

impl ByteOffset {
    #[inline]
    pub fn to_sector(self, sector_size: SectorSize) -> Sector {
        Sector(self.0 / sector_size.as_u64())
    }
    #[inline]
    pub fn checked_add(self, delta: u64) -> Option<Self> {
        self.0.checked_add(delta).map(Self)
    }
}

impl CandidateId {
    /// Deterministic candidate ID per [LAW-4].
    pub fn derive(
        device_id: &str,
        origin_tag: u8,
        mft_ref_or_inode: u64,
        physical_byte_offset: ByteOffset,
        logical_size: u64,
    ) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(device_id.as_bytes());
        hasher.update([origin_tag]);
        hasher.update(mft_ref_or_inode.to_le_bytes());
        hasher.update(physical_byte_offset.0.to_le_bytes());
        hasher.update(logical_size.to_le_bytes());
        let digest = hasher.finalize();
        let mut id = [0u8; 16];
        id.copy_from_slice(&digest[..16]);
        Self(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sector_byte_roundtrip() {
        for &sz in &[512u32, 4096u32] {
            let ss = SectorSize::new(sz).unwrap();
            for s in [0u64, 1, 63, 2048, 1_000_000] {
                let off = Sector(s).to_byte_offset(ss).unwrap();
                assert_eq!(off.to_sector(ss), Sector(s));
            }
        }
    }

    #[test]
    fn candidate_id_is_deterministic() {
        let a = CandidateId::derive("dev0", 1, 42, ByteOffset(4096), 1234);
        let b = CandidateId::derive("dev0", 1, 42, ByteOffset(4096), 1234);
        let c = CandidateId::derive("dev0", 2, 0, ByteOffset(4096), 1234);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}
