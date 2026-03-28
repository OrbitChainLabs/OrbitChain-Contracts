//! Fee estimation service for transaction fees
//!
//! This module provides utilities for estimating Stellar transaction fees,
//! detecting surge pricing, converting fees to various currencies, and
//! tracking fee history.

pub mod cache;
pub mod calculator;
pub mod currency;
pub mod error;
pub mod history;
pub mod horizon_fetcher;
pub mod service;
pub mod surge_pricing;

// Re-export frequently used types
pub use cache::{CacheMetadata, CachedFeeData, FeeCache};
pub use calculator::{
    calculate_fee, stroops_to_xlm, xlm_to_stroops, FeeConfig, FeeInfo,
    // Utility functions
    estimate_payment_fee, estimate_donation_fee, estimate_multisig_setup_fee,
    estimate_contract_invocation_fee, estimate_fee_xlm, format_fee_xlm,
    format_fee_stroops, percentage_above_base, is_fee_acceptable,
    estimate_time_to_fee_reduction, calculate_surge_percent,
};
pub use currency::{Currency, CurrencyConverter, ExchangeRate, FormattedAmount};
pub use error::{FeeError, FeeResult};
pub use history::{FeeHistory, FeeRecord, FeeStats};
pub use horizon_fetcher::HorizonFeeFetcher;
pub use service::{FeeEstimationService, FeeServiceConfig};
pub use surge_pricing::{FeeTrend, SurgePricingAnalyzer, SurgePricingConfig, SurgePricingLevel};

/// Stellar fee constants
pub mod constants {
    /// Base fee in stroops (smallest unit)
    pub const BASE_FEE_STROOPS: i64 = 100;
    /// Base fee in XLM
    pub const BASE_FEE_XLM: f64 = 0.00001;
    /// Conversion factor: 1 XLM = 10,000,000 stroops
    pub const STROOPS_PER_XLM: i64 = 10_000_000;
    /// Default cache TTL in seconds (5 minutes)
    pub const DEFAULT_CACHE_TTL_SECS: i64 = 300;
}
