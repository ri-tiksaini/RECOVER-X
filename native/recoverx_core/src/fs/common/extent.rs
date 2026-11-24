use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExtentKind {
    Data,
    SparseHole,
    IoError,
    Resident,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Extent {
    pub logical_off: u64,
    pub physical_off: u64,
    pub len: u64,
    pub kind: ExtentKind,
}

impl Extent {
    pub fn end_physical(&self) -> u64 {
        self.physical_off.saturating_add(self.len)
    }
}
