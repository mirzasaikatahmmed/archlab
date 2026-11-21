//! Seeded random streams.
//!
//! ChaCha8 gives the same sequence on every platform and compiler version.
//! Transcendental functions come from `libm` (pure Rust) because the system
//! math library may round differently between operating systems.

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::{Rng, SeedableRng};

/// One independent random stream; streams of the same seed never overlap.
pub struct Stream(ChaCha8Rng);

impl Stream {
    pub fn new(seed: u64, stream: u64) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        rng.set_stream(stream);
        Self(rng)
    }

    /// Uniform in `[0, 1)` with 53 bits of precision.
    pub fn unit(&mut self) -> f64 {
        const SCALE: f64 = 1.0 / 9_007_199_254_740_992.0; // 2^-53
        #[allow(clippy::cast_precision_loss)]
        {
            (self.0.next_u64() >> 11) as f64 * SCALE
        }
    }

    /// Exponential with the given mean.
    pub fn exponential(&mut self, mean: f64) -> f64 {
        -mean * libm::log(1.0 - self.unit())
    }

    /// Standard normal via Box-Muller.
    pub fn normal(&mut self) -> f64 {
        let u1 = 1.0 - self.unit();
        let u2 = self.unit();
        libm::sqrt(-2.0 * libm::log(u1)) * libm::cos(std::f64::consts::TAU * u2)
    }

    /// Lognormal with the given mean and shape `sigma`.
    pub fn lognormal(&mut self, mean: f64, sigma: f64) -> f64 {
        if sigma <= 0.0 {
            return mean;
        }
        let mu = libm::log(mean) - sigma * sigma / 2.0;
        libm::exp(mu + sigma * self.normal())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_and_stream_repeat() {
        let mut a = Stream::new(7, 3);
        let mut b = Stream::new(7, 3);
        for _ in 0..100 {
            assert_eq!(a.unit().to_bits(), b.unit().to_bits());
        }
    }

    #[test]
    fn streams_and_seeds_differ() {
        let first = |seed, stream| Stream::new(seed, stream).unit().to_bits();
        assert_ne!(first(7, 0), first(7, 1));
        assert_ne!(first(7, 0), first(8, 0));
    }

    /// Pins the sequence so an accidental change of algorithm is caught.
    #[test]
    fn sequence_is_stable() {
        let mut s = Stream::new(42, 0);
        let v: Vec<f64> = (0..3).map(|_| s.unit()).collect();
        assert!(v.iter().all(|x| (0.0..1.0).contains(x)));
        assert_eq!(
            v,
            [0.6818961923066714, 0.950275407672484, 0.4275164028565197]
        );
    }

    #[test]
    fn exponential_and_lognormal_have_expected_means() {
        let mut s = Stream::new(1, 0);
        let n = 200_000;
        let exp: f64 = (0..n).map(|_| s.exponential(2.0)).sum::<f64>() / f64::from(n);
        let logn: f64 = (0..n).map(|_| s.lognormal(5.0, 0.5)).sum::<f64>() / f64::from(n);
        assert!((exp - 2.0).abs() < 0.05, "exp mean {exp}");
        assert!((logn - 5.0).abs() < 0.1, "lognormal mean {logn}");
    }
}
