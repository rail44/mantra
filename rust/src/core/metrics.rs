use std::time::{Duration, Instant};
use tracing::{debug, info, Level};

/// Performance timer for measuring operation durations
pub struct Timer {
    operation: String,
    start: Instant,
    log_level: Level,
}

impl Timer {
    /// Start a new timer with info level logging
    pub fn start(operation: impl Into<String>) -> Self {
        Self {
            operation: operation.into(),
            start: Instant::now(),
            log_level: Level::INFO,
        }
    }

    /// Start a new timer with debug level logging
    pub fn start_debug(operation: impl Into<String>) -> Self {
        Self {
            operation: operation.into(),
            start: Instant::now(),
            log_level: Level::DEBUG,
        }
    }

    /// Get elapsed time without stopping the timer
    pub fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }

    /// Stop the timer and log the duration
    pub fn stop(self) {
        let duration = self.start.elapsed();
        match self.log_level {
            Level::INFO => {
                info!(
                    operation = %self.operation,
                    duration_ms = duration.as_millis(),
                    "Operation completed"
                );
            }
            Level::DEBUG => {
                debug!(
                    operation = %self.operation,
                    duration_ms = duration.as_millis(),
                    "Operation completed"
                );
            }
            _ => {}
        }
    }

    /// Stop the timer with a custom message
    pub fn stop_with_message(self, message: &str) {
        let duration = self.start.elapsed();
        match self.log_level {
            Level::INFO => {
                info!(
                    operation = %self.operation,
                    duration_ms = duration.as_millis(),
                    message = %message,
                    "Operation completed"
                );
            }
            Level::DEBUG => {
                debug!(
                    operation = %self.operation,
                    duration_ms = duration.as_millis(),
                    message = %message,
                    "Operation completed"
                );
            }
            _ => {}
        }
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        // Automatically log if not explicitly stopped
        // This ensures we always get timing information
        if self.start.elapsed().as_secs() > 1 {
            // Only auto-log if operation took more than 1 second
            let duration = self.start.elapsed();
            debug!(
                operation = %self.operation,
                duration_ms = duration.as_millis(),
                "Timer dropped (operation may have been interrupted)"
            );
        }
    }
}

/// Macro for timing a block of code
#[macro_export]
macro_rules! time_operation {
    ($operation:expr, $block:block) => {{
        let _timer = $crate::core::metrics::Timer::start($operation);
        let result = $block;
        _timer.stop();
        result
    }};
}

/// Macro for timing a block of code with debug level
#[macro_export]
macro_rules! time_debug {
    ($operation:expr, $block:block) => {{
        let _timer = $crate::core::metrics::Timer::start_debug($operation);
        let result = $block;
        _timer.stop();
        result
    }};
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_timer_basic() {
        let timer = Timer::start("test_operation");
        thread::sleep(Duration::from_millis(10));
        assert!(timer.elapsed().as_millis() >= 10);
        timer.stop();
    }

    #[test]
    fn test_timer_with_message() {
        let timer = Timer::start("test_operation");
        thread::sleep(Duration::from_millis(10));
        timer.stop_with_message("Test completed successfully");
    }

    #[test]
    fn test_timer_debug_level() {
        let timer = Timer::start_debug("debug_operation");
        thread::sleep(Duration::from_millis(10));
        timer.stop();
    }

    #[test]
    fn test_time_operation_macro() {
        let result = time_operation!("macro_test", {
            thread::sleep(Duration::from_millis(10));
            42
        });
        assert_eq!(result, 42);
    }
}
