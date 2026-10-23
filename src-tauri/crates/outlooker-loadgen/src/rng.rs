//! Deterministic PRNG (xorshift64*; Part 01 has no `rand` crate). Same seed → same data on every OS.

#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        // splitmix64 of the seed, so nearby seeds give unrelated streams; never 0
        let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        Rng((z ^ (z >> 31)) | 1)
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Uniform in [0, 1).
    pub fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Uniform in [0, n); n > 0.
    pub fn below(&mut self, n: u64) -> u64 {
        ((self.next_u64() as u128 * n as u128) >> 64) as u64
    }

    /// Uniform in [lo, hi].
    pub fn range(&mut self, lo: u64, hi: u64) -> u64 {
        lo + self.below(hi - lo + 1)
    }

    pub fn chance(&mut self, p: f64) -> bool {
        self.unit() < p
    }

    /// Standard normal (Box–Muller).
    pub fn normal(&mut self) -> f64 {
        let u1 = self.unit().max(f64::MIN_POSITIVE);
        let u2 = self.unit();
        (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
    }
}

/// Zipf(s = 1) sampler over ranks 0..n (rank 0 most frequent): binary search in the cumulative weights.
#[derive(Debug, Clone)]
pub struct Zipf {
    cdf: Vec<f64>,
}

impl Zipf {
    pub fn new(n: usize) -> Zipf {
        let mut acc = 0.0;
        let mut cdf: Vec<f64> = (1..=n)
            .map(|k| {
                acc += 1.0 / k as f64;
                acc
            })
            .collect();
        for c in &mut cdf {
            *c /= acc;
        }
        Zipf { cdf }
    }

    pub fn sample(&self, rng: &mut Rng) -> usize {
        let u = rng.unit();
        self.cdf.partition_point(|&c| c <= u).min(self.cdf.len() - 1)
    }
}
