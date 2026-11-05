#![warn(missing_docs)]

use raep_ports::Clock;

/// Deterministic clock for tests.
#[derive(Clone, Debug)]
pub struct MemoryClock {
    now: i64,
}
impl MemoryClock {
    /// Creates a clock fixed at the supplied timestamp.
    pub fn new(now: i64) -> Self {
        Self { now }
    }
}

impl Clock for MemoryClock {
    fn now_millis(&self) -> i64 {
        self.now
    }
}
