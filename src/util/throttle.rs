use std::time::{Duration, Instant};

/// Simple throttle to prevent rapid repeated actions
pub struct Throttle {
    last: Option<Instant>,
    interval: Duration,
}

impl Throttle {
    pub fn new(interval: Duration) -> Self {
        Self {
            last: None,
            interval,
        }
    }

    /// Returns true if the action should be allowed (enough time has passed)
    pub fn check(&mut self) -> bool {
        let now = Instant::now();
        if let Some(last) = self.last {
            if now.duration_since(last) < self.interval {
                return false;
            }
        }
        self.last = Some(now);
        true
    }
}
