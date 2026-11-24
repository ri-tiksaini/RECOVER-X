use sha2::{Digest, Sha256};

pub struct Sha256Stream {
    hasher: Sha256,
}

impl Sha256Stream {
    pub fn new() -> Self {
        Self { hasher: Sha256::new() }
    }
    #[inline]
    pub fn update(&mut self, data: &[u8]) {
        self.hasher.update(data);
    }
    pub fn finalize_hex(self) -> String {
        let bytes = self.hasher.finalize();
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }
}

impl Default for Sha256Stream {
    fn default() -> Self {
        Self::new()
    }
}

pub struct Blake3Stream {
    hasher: blake3::Hasher,
}

impl Blake3Stream {
    pub fn new() -> Self {
        Self { hasher: blake3::Hasher::new() }
    }
    #[inline]
    pub fn update(&mut self, data: &[u8]) {
        self.hasher.update(data);
    }
    pub fn finalize_hex(self) -> String {
        self.hasher.finalize().to_hex().to_string()
    }
}

impl Default for Blake3Stream {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streaming_matches_known_sha256() {
        let mut s = Sha256Stream::new();
        s.update(b"hel");
        s.update(b"lo");
        assert_eq!(
            s.finalize_hex(),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }
}
