use std::time::Duration;

/// Controls how often the worker polls the Stellar Horizon payments endpoint.
#[derive(Debug, Clone)]
pub struct PollConfig {
    /// Base interval between polls.
    pub interval: Duration,
    /// Maximum interval after back-off (used when API errors occur).
    pub max_interval: Duration,
    /// The default interval this config was created with (for reset).
    default_interval: Duration,
}

impl Default for PollConfig {
    fn default() -> Self {
        let interval = Duration::from_secs(10);
        Self {
            interval,
            max_interval: Duration::from_secs(120),
            default_interval: interval,
        }
    }
}

impl PollConfig {
    /// Returns a config suited for high-throughput environments (shorter interval).
    pub fn high_frequency() -> Self {
        let interval = Duration::from_secs(5);
        Self {
            interval,
            max_interval: Duration::from_secs(60),
            default_interval: interval,
        }
    }

    /// Returns a config suited for low-traffic / cost-sensitive deployments.
    pub fn low_frequency() -> Self {
        let interval = Duration::from_secs(30);
        Self {
            interval,
            max_interval: Duration::from_secs(300),
            default_interval: interval,
        }
    }

    /// Doubles the interval up to `max_interval` for exponential back-off on errors.
    pub fn back_off(&mut self) {
        self.interval = (self.interval * 2).min(self.max_interval);
    }

    /// Resets the interval back to its original configured default.
    pub fn reset(&mut self) {
        self.interval = self.default_interval;
    }
}
