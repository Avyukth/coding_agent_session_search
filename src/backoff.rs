//! Exponential backoff with jitter for retryable operations.
//!
//! Provides configurable retry logic with exponential delay growth
//! and random jitter to prevent thundering herd problems.
//!
//! Default: 100ms base, 5s max, 3 attempts, ±25% jitter.

use std::time::Duration;

/// Exponential backoff configuration.
#[derive(Debug, Clone)]
pub struct Backoff {
    /// Initial delay between retries
    pub base_delay: Duration,
    /// Maximum delay cap
    pub max_delay: Duration,
    /// Maximum number of attempts (including initial)
    pub max_attempts: u32,
    /// Whether to apply random jitter (±25%)
    pub jitter: bool,
}

impl Default for Backoff {
    fn default() -> Self {
        Self {
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(5),
            max_attempts: 3,
            jitter: true,
        }
    }
}

impl Backoff {
    /// Create with custom settings.
    pub fn new(base_delay: Duration, max_delay: Duration, max_attempts: u32) -> Self {
        Self {
            base_delay,
            max_delay,
            max_attempts,
            jitter: true,
        }
    }

    /// Create for network operations: 200ms base, 10s max, 4 attempts.
    pub fn network() -> Self {
        Self {
            base_delay: Duration::from_millis(200),
            max_delay: Duration::from_secs(10),
            max_attempts: 4,
            jitter: true,
        }
    }

    /// Create for fast retries: 50ms base, 1s max, 3 attempts.
    pub fn fast() -> Self {
        Self {
            base_delay: Duration::from_millis(50),
            max_delay: Duration::from_secs(1),
            max_attempts: 3,
            jitter: true,
        }
    }

    /// Disable jitter for deterministic delays.
    pub fn without_jitter(mut self) -> Self {
        self.jitter = false;
        self
    }

    /// Calculate delay for attempt N (0-indexed).
    fn delay_for_attempt(&self, attempt: u32) -> Duration {
        // Exponential: base * 2^attempt
        let multiplier = 2u64.saturating_pow(attempt);
        let base_ms = self.base_delay.as_millis() as u64;
        let delay_ms = base_ms.saturating_mul(multiplier);

        // Cap at max
        let delay = Duration::from_millis(delay_ms.min(self.max_delay.as_millis() as u64));

        // Apply jitter if enabled (±25%)
        if self.jitter {
            let jitter_factor = 0.75 + (random_float() * 0.5); // 0.75 to 1.25
            let jittered_ms = (delay.as_millis() as f64 * jitter_factor) as u64;
            Duration::from_millis(jittered_ms)
        } else {
            delay
        }
    }

    /// Execute operation with exponential backoff.
    ///
    /// Retries `max_attempts` times if the operation returns an error
    /// that satisfies `should_retry`. Returns the first success or
    /// the final error.
    pub fn execute<T, E, F, R>(&self, mut operation: F, should_retry: R) -> Result<T, E>
    where
        F: FnMut() -> Result<T, E>,
        R: Fn(&E) -> bool,
    {
        let mut attempt = 0;
        loop {
            match operation() {
                Ok(result) => return Ok(result),
                Err(e) => {
                    attempt += 1;
                    if attempt >= self.max_attempts || !should_retry(&e) {
                        return Err(e);
                    }

                    let delay = self.delay_for_attempt(attempt - 1);
                    tracing::debug!(
                        attempt = attempt,
                        max = self.max_attempts,
                        delay_ms = delay.as_millis(),
                        "Backoff: retrying after error"
                    );
                    std::thread::sleep(delay);
                }
            }
        }
    }

    /// Execute operation with exponential backoff, always retrying on error.
    pub fn execute_always<T, E, F>(&self, operation: F) -> Result<T, E>
    where
        F: FnMut() -> Result<T, E>,
    {
        self.execute(operation, |_| true)
    }

    /// Async version - execute with exponential backoff.
    pub async fn execute_async<T, E, F, Fut, R>(
        &self,
        mut operation: F,
        should_retry: R,
    ) -> Result<T, E>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = Result<T, E>>,
        R: Fn(&E) -> bool,
    {
        let mut attempt = 0;
        loop {
            match operation().await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    attempt += 1;
                    if attempt >= self.max_attempts || !should_retry(&e) {
                        return Err(e);
                    }

                    let delay = self.delay_for_attempt(attempt - 1);
                    tracing::debug!(
                        attempt = attempt,
                        max = self.max_attempts,
                        delay_ms = delay.as_millis(),
                        "Backoff: retrying after async error"
                    );
                    tokio::time::sleep(delay).await;
                }
            }
        }
    }
}

/// Simple pseudo-random float [0.0, 1.0) using time-based seed.
fn random_float() -> f64 {
    use std::time::SystemTime;
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    // Use lower bits for some variation
    let mixed = nanos.wrapping_mul(6364136223846793005).wrapping_add(1);
    (mixed as u64 % 10000) as f64 / 10000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_delays_grow_exponentially() {
        let backoff =
            Backoff::new(Duration::from_millis(100), Duration::from_secs(10), 5).without_jitter();

        let d0 = backoff.delay_for_attempt(0);
        let d1 = backoff.delay_for_attempt(1);
        let d2 = backoff.delay_for_attempt(2);

        assert_eq!(d0, Duration::from_millis(100));
        assert_eq!(d1, Duration::from_millis(200));
        assert_eq!(d2, Duration::from_millis(400));
    }

    #[test]
    fn backoff_respects_max_delay() {
        let backoff = Backoff::new(Duration::from_millis(100), Duration::from_millis(500), 10)
            .without_jitter();

        let d5 = backoff.delay_for_attempt(5); // Would be 3200ms without cap
        assert_eq!(d5, Duration::from_millis(500));
    }

    #[test]
    fn backoff_execute_retries_on_error() {
        let backoff = Backoff::fast().without_jitter();
        let mut attempts = 0;

        let result: Result<i32, &str> = backoff.execute(
            || {
                attempts += 1;
                if attempts < 3 {
                    Err("transient")
                } else {
                    Ok(42)
                }
            },
            |_| true,
        );

        assert_eq!(result, Ok(42));
        assert_eq!(attempts, 3);
    }

    #[test]
    fn backoff_stops_after_max_attempts() {
        let backoff =
            Backoff::new(Duration::from_millis(1), Duration::from_millis(10), 3).without_jitter();
        let mut attempts = 0;

        let result: Result<i32, &str> = backoff.execute(
            || {
                attempts += 1;
                Err("always fails")
            },
            |_| true,
        );

        assert!(result.is_err());
        assert_eq!(attempts, 3);
    }

    #[test]
    fn backoff_stops_when_not_retryable() {
        let backoff = Backoff::fast();
        let mut attempts = 0;

        let result: Result<i32, &str> = backoff.execute(
            || {
                attempts += 1;
                Err("fatal")
            },
            |e| *e != "fatal",
        );

        assert!(result.is_err());
        assert_eq!(attempts, 1); // No retries for non-retryable
    }
}
