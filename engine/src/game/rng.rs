//! A small, dependency-free, seedable PRNG (SplitMix64) so the fair-value
//! process and every bot decision are reproducible from a seed. This is
//! not cryptographic and is not meant to be; it exists so tests can pin
//! behavior and so a game round can be replayed.

/// Deterministic pseudo-random source. Same seed, same sequence, forever.
#[derive(Clone, Debug)]
pub struct Rng {
    state: u64,
    /// Cached second value from the last Box-Muller pair.
    spare: Option<f64>,
}

impl Rng {
    pub fn seed(seed: u64) -> Self {
        Rng {
            // Mix the seed so nearby seeds do not produce nearby streams.
            state: seed ^ 0x9E37_79B9_7F4A_7C15,
            spare: None,
        }
    }

    /// Next raw 64-bit value (SplitMix64).
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform double in [0, 1).
    pub fn next_f64(&mut self) -> f64 {
        // Use the top 53 bits for a full-mantissa double.
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Uniform double in [lo, hi).
    pub fn uniform(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.next_f64()
    }

    /// Uniform integer in [lo, hi] inclusive.
    pub fn int(&mut self, lo: i64, hi: i64) -> i64 {
        if hi <= lo {
            return lo;
        }
        let span = (hi - lo + 1) as u64;
        lo + (self.next_u64() % span) as i64
    }

    /// True with probability `p`.
    pub fn chance(&mut self, p: f64) -> bool {
        self.next_f64() < p
    }

    /// A draw from the standard normal distribution (Box-Muller).
    pub fn gaussian(&mut self) -> f64 {
        if let Some(s) = self.spare.take() {
            return s;
        }
        let u1 = self.next_f64().max(1e-12);
        let u2 = self.next_f64();
        let r = (-2.0 * u1.ln()).sqrt();
        let theta = 2.0 * std::f64::consts::PI * u2;
        self.spare = Some(r * theta.sin());
        r * theta.cos()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_sequence() {
        let mut a = Rng::seed(42);
        let mut b = Rng::seed(42);
        for _ in 0..1000 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn different_seeds_diverge() {
        let mut a = Rng::seed(1);
        let mut b = Rng::seed(2);
        assert_ne!(a.next_u64(), b.next_u64());
    }

    #[test]
    fn next_f64_is_in_unit_interval() {
        let mut r = Rng::seed(7);
        for _ in 0..10_000 {
            let x = r.next_f64();
            assert!((0.0..1.0).contains(&x), "out of range: {x}");
        }
    }

    #[test]
    fn int_respects_inclusive_bounds() {
        let mut r = Rng::seed(9);
        for _ in 0..10_000 {
            let x = r.int(3, 7);
            assert!((3..=7).contains(&x), "out of range: {x}");
        }
        assert_eq!(r.int(5, 5), 5);
        assert_eq!(r.int(9, 2), 9); // degenerate range returns lo
    }

    #[test]
    fn gaussian_has_roughly_zero_mean_and_unit_variance() {
        let mut r = Rng::seed(123);
        let n = 50_000;
        let xs: Vec<f64> = (0..n).map(|_| r.gaussian()).collect();
        let mean = xs.iter().sum::<f64>() / n as f64;
        let var = xs.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n as f64;
        assert!(mean.abs() < 0.05, "mean too far from 0: {mean}");
        assert!((var - 1.0).abs() < 0.1, "variance too far from 1: {var}");
    }
}
