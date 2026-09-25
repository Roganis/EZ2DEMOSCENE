//! Tiny deterministic RNG / hash helpers (no external dependency so that the
//! same seed gives the same scene on every platform and version).

/// PCG-XSH-RR 32 bit generator.
#[derive(Clone, Debug)]
pub struct Rng {
    state: u64,
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        let mut r = Rng { state: 0 };
        r.next_u32();
        r.state = r.state.wrapping_add(seed ^ 0x853c_49e6_748f_ea9b);
        r.next_u32();
        r
    }

    pub fn next_u32(&mut self) -> u32 {
        let old = self.state;
        self.state = old
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let xorshifted = (((old >> 18) ^ old) >> 27) as u32;
        let rot = (old >> 59) as u32;
        xorshifted.rotate_right(rot)
    }

    /// Uniform float in [0, 1).
    pub fn f32(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / (1u32 << 24) as f32
    }

    /// Uniform float in [lo, hi).
    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.f32()
    }

    /// Signed float in [-1, 1).
    pub fn signed(&mut self) -> f32 {
        self.f32() * 2.0 - 1.0
    }

    pub fn range_u32(&mut self, lo: u32, hi_inclusive: u32) -> u32 {
        if hi_inclusive <= lo {
            return lo;
        }
        lo + self.next_u32() % (hi_inclusive - lo + 1)
    }

    pub fn chance(&mut self, p: f32) -> bool {
        self.f32() < p
    }

    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.next_u32() as usize % items.len()]
    }
}

/// Stateless integer hash (lowbias32), matches `hash_u32` in the shaders.
pub fn hash_u32(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    x
}

/// Hash to float in [0, 1).
pub fn hash_f32(x: u32) -> f32 {
    (hash_u32(x) >> 8) as f32 / (1u32 << 24) as f32
}

/// Hash of two values to float in [0, 1).
pub fn hash2(a: u32, b: u32) -> f32 {
    hash_f32(a.wrapping_mul(0x9e37_79b9) ^ hash_u32(b))
}
