use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Like `Range` but for time
///
/// This just represents the bare minimum data for a timer, but doesn't deal with any of the practical functionality.
/// To see an example of something that uses `Timer`, go to `TimerWidget`.
#[derive(Serialize, Deserialize, Clone, Copy)]
pub struct Timer {
    pub total: Duration,
    pub elapsed: Duration,
}
impl Timer {
    #[must_use]
    pub const fn new_clean(total: Duration) -> Self {
        Self {
            total,
            elapsed: Duration::ZERO,
        }
    }

    #[must_use]
    pub fn is_done(&self) -> bool {
        self.elapsed >= self.total
    }

    pub fn increment(&mut self, increment_amount: Duration) {
        self.elapsed += increment_amount;

        // clamp
        if self.is_done() {
            self.elapsed = self.total;
        }
    }
}
