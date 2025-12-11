//! Simple token bucket rate limiter for search operations.
//!
//! Prevents resource exhaustion from rapid search queries.
//! Default: 10 requests/second with burst capacity of 20.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

/// Token bucket rate limiter for search queries.
///
/// Allows bursts up to `max_tokens` but refills at `refill_rate` tokens/second.
/// Blocking: waits until a token is available (up to timeout).
pub struct RateLimiter {
    tokens: AtomicU32,
    max_tokens: u32,
    last_refill: Mutex<Instant>,
    refill_rate: u32, // tokens per second
}

impl RateLimiter {
    /// Create a new rate limiter.
    ///
    /// - `max_tokens`: Burst capacity (max tokens to accumulate)
    /// - `refill_rate`: Tokens added per second
    pub fn new(max_tokens: u32, refill_rate: u32) -> Self {
        Self {
            tokens: AtomicU32::new(max_tokens),
            max_tokens,
            last_refill: Mutex::new(Instant::now()),
            refill_rate,
        }
    }

    /// Create with default settings: 20 burst, 10/sec refill
    pub fn default_search() -> Self {
        Self::new(20, 10)
    }

    /// Try to acquire a token, blocking if necessary (up to 1 second).
    ///
    /// Returns `true` if token acquired, `false` if timed out.
    pub fn acquire(&self) -> bool {
        self.acquire_timeout(Duration::from_secs(1))
    }

    /// Try to acquire a token with custom timeout.
    pub fn acquire_timeout(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;

        loop {
            self.refill();

            // Try to decrement tokens
            let current = self.tokens.load(Ordering::Relaxed);
            if current > 0
                && self
                    .tokens
                    .compare_exchange(current, current - 1, Ordering::Relaxed, Ordering::Relaxed)
                    .is_ok()
            {
                return true;
            }

            // No tokens available, check deadline
            if Instant::now() >= deadline {
                return false;
            }

            // Sleep briefly and retry
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Try to acquire without blocking.
    ///
    /// Returns `true` if token acquired, `false` if rate limited.
    pub fn try_acquire(&self) -> bool {
        self.refill();

        let current = self.tokens.load(Ordering::Relaxed);
        if current > 0 {
            self.tokens
                .compare_exchange(current, current - 1, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
        } else {
            false
        }
    }

    /// Refill tokens based on elapsed time since last refill.
    fn refill(&self) {
        let mut last = match self.last_refill.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };

        let elapsed = last.elapsed();
        let new_tokens = (elapsed.as_secs_f64() * self.refill_rate as f64) as u32;

        if new_tokens > 0 {
            *last = Instant::now();
            let current = self.tokens.load(Ordering::Relaxed);
            let new_total = (current + new_tokens).min(self.max_tokens);
            self.tokens.store(new_total, Ordering::Relaxed);
        }
    }

    /// Get current token count (for metrics/debugging).
    pub fn available_tokens(&self) -> u32 {
        self.refill();
        self.tokens.load(Ordering::Relaxed)
    }
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::default_search()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_limiter_allows_burst() {
        let limiter = RateLimiter::new(5, 10);
        // Should allow burst of 5
        for _ in 0..5 {
            assert!(limiter.try_acquire());
        }
        // 6th should fail immediately
        assert!(!limiter.try_acquire());
    }

    #[test]
    fn rate_limiter_refills_over_time() {
        let limiter = RateLimiter::new(2, 100); // 100/sec = 1 per 10ms
        // Exhaust tokens
        assert!(limiter.try_acquire());
        assert!(limiter.try_acquire());
        assert!(!limiter.try_acquire());

        // Wait for refill
        std::thread::sleep(Duration::from_millis(15));

        // Should have refilled at least 1 token
        assert!(limiter.try_acquire());
    }
}
