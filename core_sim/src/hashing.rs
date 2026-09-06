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

// --- splitmix64, the crate's one deterministic 64-bit mixer ---------------------------------
/// splitmix64's increment (the odd 64-bit "golden gamma").
const SPLITMIX_GAMMA: u64 = 0x9E37_79B9_7F4A_7C15;
/// splitmix64's first mix multiplier.
const SPLITMIX_MIX_A: u64 = 0xBF58_476D_1CE4_E5B9;
/// splitmix64's second mix multiplier.
const SPLITMIX_MIX_B: u64 = 0x94D0_49BB_1331_11EB;
/// First xor-shift distance in splitmix64's finalizer.
const SPLITMIX_SHIFT_A: u32 = 30;
/// Second xor-shift distance.
const SPLITMIX_SHIFT_B: u32 = 27;
/// Third xor-shift distance.
const SPLITMIX_SHIFT_C: u32 = 31;

/// splitmix64 — a pure, deterministic 64-bit mixer. No state, no RNG, no allocation: the same
/// input always produces the same output, on every machine and every run.
///
/// **One implementation, because a mixer is a contract about bits.** Three call sites derive
/// reproducible draws from a seed through it — hydrology's flat-tie jitter, flora's per-tile
/// realization, and a faction's band-name permutation — and each one is a world that has to look
/// the same on a reload. A second copy that drifted by one shift would silently regenerate a
/// different world from the same seed.
#[inline]
pub fn splitmix64(x: u64) -> u64 {
    let mut z = x.wrapping_add(SPLITMIX_GAMMA);
    z = (z ^ (z >> SPLITMIX_SHIFT_A)).wrapping_mul(SPLITMIX_MIX_A);
    z = (z ^ (z >> SPLITMIX_SHIFT_B)).wrapping_mul(SPLITMIX_MIX_B);
    z ^ (z >> SPLITMIX_SHIFT_C)
}
