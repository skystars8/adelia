use std::time::{Duration, Instant};

use dashmap::DashMap;

#[derive(Debug)]
struct Window {
    started: Instant,
    count: u32,
}

#[derive(Debug, Default)]
pub struct RateLimiter {
    windows: DashMap<String, Window>,
}

impl RateLimiter {
    pub fn check(&self, key: String, limit: u32, duration: Duration) -> bool {
        let now = Instant::now();
        let mut window = self.windows.entry(key).or_insert(Window {
            started: now,
            count: 0,
        });
        if now.duration_since(window.started) >= duration {
            window.started = now;
            window.count = 0;
        }
        if window.count >= limit {
            return false;
        }
        window.count += 1;
        drop(window);

        if self.windows.len() > 100_000 {
            self.windows
                .retain(|_, entry| now.duration_since(entry.started) < Duration::from_secs(7200));
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_up_to_limit_without_locking_the_map() {
        let limiter = RateLimiter::default();
        assert!(limiter.check("post-board:general".to_owned(), 2, Duration::from_secs(60)));
        assert!(limiter.check("post-board:general".to_owned(), 2, Duration::from_secs(60)));
        assert!(!limiter.check("post-board:general".to_owned(), 2, Duration::from_secs(60)));
    }
}
