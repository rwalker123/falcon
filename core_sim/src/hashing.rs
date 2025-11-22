use std::hash::Hasher;

/// A deterministic FNV-1a 64-bit hasher.
///
/// Used to replace `DefaultHasher` (which is randomized) for generating
/// deterministic seeds from string identifiers in the simulation.
#[derive(Debug, Default)]
pub struct FnvHasher {
    state: u64,
}

impl FnvHasher {
    const OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x100000001b3;

    pub fn new() -> Self {
        Self {
            state: Self::OFFSET_BASIS,
        }
    }
}

impl Hasher for FnvHasher {
    fn finish(&self) -> u64 {
        self.state
    }

    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.state ^= byte as u64;
            self.state = self.state.wrapping_mul(Self::PRIME);
        }
    }
}
