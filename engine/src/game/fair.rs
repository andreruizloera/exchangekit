//! The hidden fair-value process that drives a game round.
//!
//! A round has a single true price that no participant sees directly. It
//! moves as a random walk with a small drift, per-step Gaussian noise, and
//! occasional jumps that stand in for news shocks. Bots reference noisy
//! estimates of it; the player has to infer it from the book and the tape.

use super::rng::Rng;

/// Parameters of the fair-value walk. All values are in cents.
#[derive(Clone, Debug)]
pub struct FairValueConfig {
    /// Starting price in cents.
    pub start: f64,
    /// Deterministic drift added every step.
    pub drift: f64,
    /// Standard deviation of the per-step Gaussian move.
    pub sigma: f64,
    /// Probability of a jump on any given step.
    pub jump_prob: f64,
    /// Standard deviation of a jump when one happens.
    pub jump_sigma: f64,
    /// Lower and upper clamps (a binary contract lives in 1..=99 cents).
    pub min: f64,
    pub max: f64,
}

/// The evolving hidden fair value.
#[derive(Clone, Debug)]
pub struct FairValue {
    pub value: f64,
    pub cfg: FairValueConfig,
}

impl FairValue {
    pub fn new(cfg: FairValueConfig) -> Self {
        let value = cfg.start.clamp(cfg.min, cfg.max);
        FairValue { value, cfg }
    }

    /// Advance one step and return the new value.
    pub fn step(&mut self, rng: &mut Rng) -> f64 {
        let mut v = self.value + self.cfg.drift + self.cfg.sigma * rng.gaussian();
        if rng.chance(self.cfg.jump_prob) {
            v += self.cfg.jump_sigma * rng.gaussian();
        }
        self.value = v.clamp(self.cfg.min, self.cfg.max);
        self.value
    }

    /// The fair value rounded to a tradeable cent in 1..=99.
    pub fn cents(&self) -> u32 {
        (self.value.round() as i64).clamp(1, 99) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> FairValueConfig {
        FairValueConfig {
            start: 50.0,
            drift: 0.0,
            sigma: 1.0,
            jump_prob: 0.02,
            jump_sigma: 8.0,
            min: 1.0,
            max: 99.0,
        }
    }

    #[test]
    fn same_seed_same_path() {
        let mut r1 = Rng::seed(5);
        let mut r2 = Rng::seed(5);
        let mut f1 = FairValue::new(cfg());
        let mut f2 = FairValue::new(cfg());
        for _ in 0..500 {
            assert_eq!(f1.step(&mut r1), f2.step(&mut r2));
        }
    }

    #[test]
    fn stays_within_clamps() {
        let mut r = Rng::seed(11);
        let mut f = FairValue::new(cfg());
        for _ in 0..100_000 {
            let v = f.step(&mut r);
            assert!((1.0..=99.0).contains(&v), "escaped clamp: {v}");
        }
    }

    #[test]
    fn cents_are_always_tradeable() {
        let mut r = Rng::seed(13);
        let mut f = FairValue::new(FairValueConfig {
            start: 2.0,
            drift: -5.0, // push hard against the floor
            ..cfg()
        });
        for _ in 0..1000 {
            f.step(&mut r);
            let c = f.cents();
            assert!((1..=99).contains(&c), "untradeable cents: {c}");
        }
    }

    #[test]
    fn positive_drift_raises_the_average_path() {
        // With no noise, drift alone must move the value up.
        let mut r = Rng::seed(1);
        let mut f = FairValue::new(FairValueConfig {
            start: 40.0,
            drift: 0.2,
            sigma: 0.0,
            jump_prob: 0.0,
            jump_sigma: 0.0,
            min: 1.0,
            max: 99.0,
        });
        for _ in 0..50 {
            f.step(&mut r);
        }
        assert!((f.value - 50.0).abs() < 1e-9, "expected 40 + 50*0.2 = 50");
    }
}
