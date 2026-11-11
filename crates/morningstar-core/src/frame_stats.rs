use std::time::Duration;

/// A fixed-size rolling window of durations (frame intervals, CPU time per
/// frame, ...). Recording never allocates; summarising allocates once.
#[derive(Clone, Debug)]
pub struct FrameStats {
    /// Samples in microseconds. `u32` holds ~71 minutes, far beyond any frame.
    samples: Box<[u32]>,
    next: usize,
    len: usize,
    total_recorded: u64,
}

/// Percentile summary of a [`FrameStats`] window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameSummary {
    /// Number of samples the percentiles were computed from.
    pub samples: usize,
    pub min: Duration,
    pub median: Duration,
    pub p95: Duration,
    pub p99: Duration,
    pub max: Duration,
}

impl FrameStats {
    /// Creates a window holding the most recent `capacity` samples.
    ///
    /// # Panics
    /// Panics if `capacity` is zero.
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "FrameStats capacity must be non-zero");
        Self {
            samples: vec![0; capacity].into_boxed_slice(),
            next: 0,
            len: 0,
            total_recorded: 0,
        }
    }

    pub fn record(&mut self, sample: Duration) {
        self.samples[self.next] = u32::try_from(sample.as_micros()).unwrap_or(u32::MAX);
        self.next = (self.next + 1) % self.samples.len();
        self.len = (self.len + 1).min(self.samples.len());
        self.total_recorded += 1;
    }

    /// Number of samples currently in the window.
    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Number of samples ever recorded, including those that have rolled out
    /// of the window.
    pub fn total_recorded(&self) -> u64 {
        self.total_recorded
    }

    pub fn clear(&mut self) {
        self.next = 0;
        self.len = 0;
    }

    /// Nearest-rank percentiles over the current window, or `None` if empty.
    pub fn summary(&self) -> Option<FrameSummary> {
        if self.len == 0 {
            return None;
        }
        // Until the window wraps, valid samples are the first `len` slots;
        // afterwards every slot is valid. Order is irrelevant for percentiles.
        let mut sorted = self.samples[..self.len].to_vec();
        sorted.sort_unstable();

        let at = |percentile: usize| {
            let rank = (percentile * sorted.len()).div_ceil(100).max(1);
            Duration::from_micros(u64::from(sorted[rank - 1]))
        };

        Some(FrameSummary {
            samples: sorted.len(),
            min: Duration::from_micros(u64::from(sorted[0])),
            median: at(50),
            p95: at(95),
            p99: at(99),
            max: Duration::from_micros(u64::from(sorted[sorted.len() - 1])),
        })
    }
}

impl std::fmt::Display for FrameSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ms = |d: Duration| d.as_secs_f64() * 1000.0;
        write!(
            f,
            "n={} min={:.2}ms p50={:.2}ms p95={:.2}ms p99={:.2}ms max={:.2}ms",
            self.samples,
            ms(self.min),
            ms(self.median),
            ms(self.p95),
            ms(self.p99),
            ms(self.max),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn micros(n: u64) -> Duration {
        Duration::from_micros(n)
    }

    #[test]
    fn empty_has_no_summary() {
        assert_eq!(FrameStats::new(8).summary(), None);
    }

    #[test]
    fn percentiles_use_nearest_rank() {
        let mut stats = FrameStats::new(100);
        for n in 1..=100 {
            stats.record(micros(n));
        }
        let s = stats.summary().unwrap();
        assert_eq!(s.samples, 100);
        assert_eq!(s.min, micros(1));
        assert_eq!(s.median, micros(50));
        assert_eq!(s.p95, micros(95));
        assert_eq!(s.p99, micros(99));
        assert_eq!(s.max, micros(100));
    }

    #[test]
    fn single_sample() {
        let mut stats = FrameStats::new(4);
        stats.record(micros(16_667));
        let s = stats.summary().unwrap();
        assert_eq!(
            (s.min, s.median, s.p99, s.max),
            (
                micros(16_667),
                micros(16_667),
                micros(16_667),
                micros(16_667)
            )
        );
    }

    #[test]
    fn window_keeps_only_most_recent() {
        let mut stats = FrameStats::new(3);
        for n in [1_000, 2, 3, 4] {
            stats.record(micros(n));
        }
        let s = stats.summary().unwrap();
        assert_eq!(s.samples, 3);
        assert_eq!(s.max, micros(4), "the 1000us outlier rolled out");
        assert_eq!(stats.total_recorded(), 4);
    }

    #[test]
    fn oversized_sample_saturates() {
        let mut stats = FrameStats::new(1);
        stats.record(Duration::from_secs(100_000));
        assert_eq!(stats.summary().unwrap().max, micros(u64::from(u32::MAX)));
    }
}
