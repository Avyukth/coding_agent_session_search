//! Circuit breaker pattern for external operations.
//!
//! Prevents repeated slow/failing operations by tracking failures and
//! temporarily blocking requests when a threshold is exceeded.
//!
//! States:
//! - Closed: Normal operation, requests pass through
//! - Open: Requests blocked after threshold failures
//! - HalfOpen: Single test request allowed after timeout
//!
//! Default: Opens after 3 failures, resets after 30 seconds.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU8, AtomicU32, Ordering};
use std::time::{Duration, Instant};

/// Circuit breaker states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CircuitState {
    /// Normal operation - requests pass through
    Closed = 0,
    /// Blocking requests - too many failures
    Open = 1,
    /// Testing - allowing one request through
    HalfOpen = 2,
}

impl From<u8> for CircuitState {
    fn from(v: u8) -> Self {
        match v {
            0 => CircuitState::Closed,
            1 => CircuitState::Open,
            2 => CircuitState::HalfOpen,
            _ => CircuitState::Closed,
        }
    }
}

/// Circuit breaker for protecting external operations.
///
/// Thread-safe and lock-free for the hot path (state checks).
pub struct CircuitBreaker {
    state: AtomicU8,
    failure_count: AtomicU32,
    last_failure: Mutex<Option<Instant>>,
    failure_threshold: u32,
    open_duration: Duration,
    name: String,
}

impl CircuitBreaker {
    /// Create a new circuit breaker with custom settings.
    ///
    /// - `name`: Identifier for logging
    /// - `failure_threshold`: Number of failures before opening
    /// - `open_duration`: How long to stay open before allowing test request
    pub fn new(name: impl Into<String>, failure_threshold: u32, open_duration: Duration) -> Self {
        Self {
            state: AtomicU8::new(CircuitState::Closed as u8),
            failure_count: AtomicU32::new(0),
            last_failure: Mutex::new(None),
            failure_threshold,
            open_duration,
            name: name.into(),
        }
    }

    /// Create with default settings: 3 failures, 30s open duration.
    pub fn default_http() -> Self {
        Self::new("http", 3, Duration::from_secs(30))
    }

    /// Create for file system operations: 5 failures, 60s open.
    pub fn default_fs() -> Self {
        Self::new("fs", 5, Duration::from_secs(60))
    }

    /// Get current state.
    pub fn state(&self) -> CircuitState {
        self.check_half_open();
        CircuitState::from(self.state.load(Ordering::Relaxed))
    }

    /// Check if circuit allows requests through.
    pub fn is_closed(&self) -> bool {
        matches!(self.state(), CircuitState::Closed | CircuitState::HalfOpen)
    }

    /// Record a successful operation - resets failure count.
    pub fn record_success(&self) {
        let prev_state = CircuitState::from(self.state.load(Ordering::Relaxed));
        self.failure_count.store(0, Ordering::Relaxed);
        self.state
            .store(CircuitState::Closed as u8, Ordering::Relaxed);

        if prev_state != CircuitState::Closed {
            tracing::info!(
                circuit = %self.name,
                "Circuit breaker closed after successful operation"
            );
        }
    }

    /// Record a failed operation - may open circuit.
    pub fn record_failure(&self) {
        let count = self.failure_count.fetch_add(1, Ordering::Relaxed) + 1;

        // Update last failure time
        if let Ok(mut last) = self.last_failure.lock() {
            *last = Some(Instant::now());
        }

        if count >= self.failure_threshold {
            let prev = self.state.swap(CircuitState::Open as u8, Ordering::Relaxed);
            if prev != CircuitState::Open as u8 {
                tracing::warn!(
                    circuit = %self.name,
                    failures = count,
                    threshold = self.failure_threshold,
                    open_for_secs = self.open_duration.as_secs(),
                    "Circuit breaker opened"
                );
            }
        }
    }

    /// Check if we should transition from Open to HalfOpen.
    fn check_half_open(&self) {
        if CircuitState::from(self.state.load(Ordering::Relaxed)) != CircuitState::Open {
            return;
        }

        let should_half_open = self
            .last_failure
            .lock()
            .ok()
            .and_then(|guard| *guard)
            .is_some_and(|last| last.elapsed() >= self.open_duration);

        if should_half_open {
            let prev = self
                .state
                .compare_exchange(
                    CircuitState::Open as u8,
                    CircuitState::HalfOpen as u8,
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                )
                .unwrap_or(CircuitState::Open as u8);

            if prev == CircuitState::Open as u8 {
                tracing::debug!(
                    circuit = %self.name,
                    "Circuit breaker half-open, allowing test request"
                );
            }
        }
    }

    /// Execute an operation with circuit breaker protection.
    ///
    /// Returns `Err("circuit open")` if circuit is open.
    /// Otherwise executes the operation and records success/failure.
    pub fn execute<T, E, F>(&self, operation: F) -> Result<T, CircuitError<E>>
    where
        F: FnOnce() -> Result<T, E>,
    {
        if !self.is_closed() {
            return Err(CircuitError::Open {
                circuit: self.name.clone(),
            });
        }

        match operation() {
            Ok(result) => {
                self.record_success();
                Ok(result)
            }
            Err(e) => {
                self.record_failure();
                Err(CircuitError::Inner(e))
            }
        }
    }

    /// Async version of execute for async operations.
    pub async fn execute_async<T, E, F, Fut>(&self, operation: F) -> Result<T, CircuitError<E>>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T, E>>,
    {
        if !self.is_closed() {
            return Err(CircuitError::Open {
                circuit: self.name.clone(),
            });
        }

        match operation().await {
            Ok(result) => {
                self.record_success();
                Ok(result)
            }
            Err(e) => {
                self.record_failure();
                Err(CircuitError::Inner(e))
            }
        }
    }
}

/// Error type for circuit breaker operations.
#[derive(Debug)]
pub enum CircuitError<E> {
    /// Circuit is open, operation not attempted
    Open { circuit: String },
    /// Operation was attempted but failed
    Inner(E),
}

impl<E: std::fmt::Display> std::fmt::Display for CircuitError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CircuitError::Open { circuit } => {
                write!(f, "Circuit breaker '{}' is open", circuit)
            }
            CircuitError::Inner(e) => write!(f, "{}", e),
        }
    }
}

impl<E: std::error::Error + 'static> std::error::Error for CircuitError<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CircuitError::Open { .. } => None,
            CircuitError::Inner(e) => Some(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn circuit_starts_closed() {
        let cb = CircuitBreaker::new("test", 3, Duration::from_secs(1));
        assert_eq!(cb.state(), CircuitState::Closed);
        assert!(cb.is_closed());
    }

    #[test]
    fn circuit_opens_after_threshold_failures() {
        let cb = CircuitBreaker::new("test", 3, Duration::from_secs(1));

        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Closed);

        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Closed);

        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Open);
        assert!(!cb.is_closed());
    }

    #[test]
    fn success_resets_failure_count() {
        let cb = CircuitBreaker::new("test", 3, Duration::from_secs(1));

        cb.record_failure();
        cb.record_failure();
        cb.record_success();

        // Should need 3 more failures to open
        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Closed);

        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Open);
    }

    #[test]
    fn execute_returns_open_error_when_circuit_open() {
        let cb = CircuitBreaker::new("test", 1, Duration::from_secs(60));
        cb.record_failure(); // Opens circuit

        let result: Result<(), CircuitError<()>> = cb.execute(|| Ok(()));

        assert!(matches!(result, Err(CircuitError::Open { .. })));
    }
}
