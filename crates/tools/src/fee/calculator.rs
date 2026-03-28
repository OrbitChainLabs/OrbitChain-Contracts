use serde::{Deserialize, Serialize};

use super::error::{FeeError, FeeResult};

/// Base fee in stroops (1 XLM = 10,000,000 stroops)
/// Standard base fee is 100 stroops per operation
pub const BASE_FEE_STROOPS: i64 = 100;
pub const BASE_FEE_XLM: f64 = 0.00001;
pub const STROOPS_PER_XLM: i64 = 10_000_000;

/// Fee calculation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeeConfig {
    /// Base fee in stroops (typically 100)
    pub base_fee_stroops: i64,
    /// Minimum fee in XLM
    pub min_fee_xlm: f64,
    /// Maximum fee in XLM before warning
    pub max_fee_xlm: f64,
    /// Warning threshold percentage (e.g., 150% = 50% above normal)
    pub surge_threshold_percent: f64,
}

impl Default for FeeConfig {
    fn default() -> Self {
        Self {
            base_fee_stroops: BASE_FEE_STROOPS,
            min_fee_xlm: 0.00001,
            max_fee_xlm: 100.0,
            surge_threshold_percent: 150.0,
        }
    }
}

/// Fee information for a transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeeInfo {
    /// Base fee in stroops from Horizon
    pub base_fee_stroops: i64,
    /// Number of operations in transaction
    pub operation_count: u32,
    /// Total fee in stroops
    pub total_fee_stroops: i64,
    /// Total fee in XLM
    pub total_fee_xlm: f64,
    /// Is this a surge pricing scenario
    pub is_surge_pricing: bool,
    /// Surge pricing percentage (100 = normal, 200 = 2x)
    pub surge_percent: f64,
    /// Timestamp when fee was fetched
    pub fetched_at: chrono::DateTime<chrono::Utc>,
}

impl FeeInfo {
    /// Create new fee info
    pub fn new(
        base_fee_stroops: i64,
        operation_count: u32,
        is_surge_pricing: bool,
        surge_percent: f64,
    ) -> FeeResult<Self> {
        if operation_count == 0 {
            return Err(FeeError::InvalidOperationCount(
                "operation_count must be at least 1".to_string(),
            ));
        }

        if base_fee_stroops < 0 {
            return Err(FeeError::InvalidFeeValue(
                "base_fee_stroops cannot be negative".to_string(),
            ));
        }

        let total_fee_stroops = base_fee_stroops
            .checked_mul(operation_count as i64)
            .ok_or_else(|| FeeError::InvalidFeeValue("fee calculation overflow".to_string()))?;

        let total_fee_xlm = stroops_to_xlm(total_fee_stroops);

        Ok(Self {
            base_fee_stroops,
            operation_count,
            total_fee_stroops,
            total_fee_xlm,
            is_surge_pricing,
            surge_percent,
            fetched_at: chrono::Utc::now(),
        })
    }

    /// Check if fee exceeds threshold
    pub fn exceeds_threshold(&self, threshold_xlm: f64) -> bool {
        self.total_fee_xlm > threshold_xlm
    }

    /// Get fee in stroops
    pub fn fee_stroops(&self) -> i64 {
        self.total_fee_stroops
    }

    /// Get fee in XLM
    pub fn fee_xlm(&self) -> f64 {
        self.total_fee_xlm
    }

    /// Get age of fee data in seconds
    pub fn age_seconds(&self) -> i64 {
        (chrono::Utc::now() - self.fetched_at).num_seconds()
    }

    /// Check if fee data is fresh (within cache TTL)
    pub fn is_fresh(&self, cache_ttl_seconds: i64) -> bool {
        self.age_seconds() < cache_ttl_seconds
    }
}

/// Calculate fee for given stroops and operation count
pub fn calculate_fee(base_fee_stroops: i64, operation_count: u32) -> FeeResult<i64> {
    if operation_count == 0 {
        return Err(FeeError::InvalidOperationCount(
            "operation_count must be at least 1".to_string(),
        ));
    }

    if base_fee_stroops < 0 {
        return Err(FeeError::InvalidFeeValue(
            "base_fee_stroops cannot be negative".to_string(),
        ));
    }

    base_fee_stroops
        .checked_mul(operation_count as i64)
        .ok_or_else(|| FeeError::InvalidFeeValue("fee calculation overflow".to_string()))
}

/// Convert stroops to XLM
pub fn stroops_to_xlm(stroops: i64) -> f64 {
    stroops as f64 / STROOPS_PER_XLM as f64
}

/// Convert XLM to stroops
pub fn xlm_to_stroops(xlm: f64) -> i64 {
    (xlm * STROOPS_PER_XLM as f64) as i64
}

/// Calculate surge pricing percentage
/// Normal base fee is 100 stroops, surge pricing increases this
pub fn calculate_surge_percent(current_fee: i64, normal_fee: i64) -> f64 {
    if normal_fee == 0 {
        return 100.0;
    }
    (current_fee as f64 / normal_fee as f64) * 100.0
}

// --- Utility Functions ---

/// Estimate fee for a simple payment (1 operation)
pub fn estimate_payment_fee(base_fee_stroops: i64) -> FeeResult<i64> {
    calculate_fee(base_fee_stroops, 1)
}

/// Estimate fee for a payment with a memo (1 operation)
pub fn estimate_payment_with_memo_fee(base_fee_stroops: i64) -> FeeResult<i64> {
    // Memo doesn't add operations, just data
    calculate_fee(base_fee_stroops, 1)
}

/// Estimate fee for a typical donation (payment + contract call)
pub fn estimate_donation_fee(base_fee_stroops: i64) -> FeeResult<i64> {
    // Donation requires: payment operation + contract invocation
    calculate_fee(base_fee_stroops, 2)
}

/// Estimate fee for a complex transaction (payment + multi-sig setup)
pub fn estimate_multisig_setup_fee(base_fee_stroops: i64) -> FeeResult<i64> {
    // Multi-sig: multiple set_options operations
    calculate_fee(base_fee_stroops, 3)
}

/// Estimate fee for contract invocation
pub fn estimate_contract_invocation_fee(base_fee_stroops: i64, num_operations: u32) -> FeeResult<i64> {
    if num_operations == 0 {
        return Err(FeeError::InvalidOperationCount(
            "contract invocations require at least 1 operation".to_string(),
        ));
    }
    calculate_fee(base_fee_stroops, num_operations)
}

/// Estimate fee in XLM for a transaction
pub fn estimate_fee_xlm(base_fee_stroops: i64, operation_count: u32) -> FeeResult<f64> {
    let stroops = calculate_fee(base_fee_stroops, operation_count)?;
    Ok(stroops_to_xlm(stroops))
}

/// Format fee in XLM with specified decimal places
pub fn format_fee_xlm(fee_stroops: i64, decimals: usize) -> String {
    let xlm = stroops_to_xlm(fee_stroops);
    format!("{:.prec$} XLM", xlm, prec = decimals)
}

/// Format fee in stroops with thousands separator
pub fn format_fee_stroops(fee_stroops: i64) -> String {
    format!("{:,} stroops", fee_stroops)
}

/// Calculate percentage increase from base fee
pub fn percentage_above_base(current_fee: i64) -> f64 {
    let base = BASE_FEE_STROOPS;
    if base == 0 {
        return 0.0;
    }
    ((current_fee - base) as f64 / base as f64) * 100.0
}

/// Check if fee is within acceptable range
pub fn is_fee_acceptable(fee_stroops: i64, max_acceptable_xlm: f64) -> bool {
    let fee_xlm = stroops_to_xlm(fee_stroops);
    fee_xlm <= max_acceptable_xlm
}

/// Estimate time to next fee reduction (simple heuristic)
/// Returns minutes estimated until fees drop
pub fn estimate_time_to_fee_reduction(current_fee: i64, trend_analysis: bool) -> u32 {
    if trend_analysis && current_fee > BASE_FEE_STROOPS * 2 {
        // Heavy congestion: estimate 15-30 minutes
        (15 + (current_fee / 1000) as u32).min(30)
    } else if current_fee > BASE_FEE_STROOPS {
        // Moderate congestion: estimate 5-15 minutes
        (5 + (current_fee / 500) as u32).min(15)
    } else {
        // Normal: no wait needed
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fee_config_default() {
        let config = FeeConfig::default();
        assert_eq!(config.base_fee_stroops, BASE_FEE_STROOPS);
        assert_eq!(config.min_fee_xlm, 0.00001);
    }

    #[test]
    fn test_calculate_fee_single_operation() {
        let fee = calculate_fee(100, 1).unwrap();
        assert_eq!(fee, 100);
    }

    #[test]
    fn test_calculate_fee_multiple_operations() {
        let fee = calculate_fee(100, 5).unwrap();
        assert_eq!(fee, 500);
    }

    #[test]
    fn test_calculate_fee_invalid_operation_count() {
        let result = calculate_fee(100, 0);
        assert!(result.is_err());
    }

    #[test]
    fn test_calculate_fee_negative_base_fee() {
        let result = calculate_fee(-100, 1);
        assert!(result.is_err());
    }

    #[test]
    fn test_stroops_to_xlm() {
        let xlm = stroops_to_xlm(10_000_000);
        assert_eq!(xlm, 1.0);

        let xlm = stroops_to_xlm(100);
        assert_eq!(xlm, 0.00001);

        let xlm = stroops_to_xlm(1_000_000);
        assert_eq!(xlm, 0.1);
    }

    #[test]
    fn test_xlm_to_stroops() {
        let stroops = xlm_to_stroops(1.0);
        assert_eq!(stroops, 10_000_000);

        let stroops = xlm_to_stroops(0.00001);
        assert_eq!(stroops, 100);

        let stroops = xlm_to_stroops(0.1);
        assert_eq!(stroops, 1_000_000);
    }

    #[test]
    fn test_surge_pricing_calculation() {
        let surge = calculate_surge_percent(200, 100);
        assert_eq!(surge, 200.0); // 2x normal fee

        let surge = calculate_surge_percent(150, 100);
        assert_eq!(surge, 150.0); // 1.5x normal fee

        let surge = calculate_surge_percent(100, 100);
        assert_eq!(surge, 100.0); // normal fee
    }

    #[test]
    fn test_fee_info_creation() {
        let fee_info = FeeInfo::new(100, 3, false, 100.0).unwrap();
        assert_eq!(fee_info.base_fee_stroops, 100);
        assert_eq!(fee_info.operation_count, 3);
        assert_eq!(fee_info.total_fee_stroops, 300);
        assert_eq!(fee_info.total_fee_xlm, 0.00003);
        assert!(!fee_info.is_surge_pricing);
    }

    #[test]
    fn test_fee_info_surge_pricing() {
        let fee_info = FeeInfo::new(200, 2, true, 200.0).unwrap();
        assert!(fee_info.is_surge_pricing);
        assert_eq!(fee_info.surge_percent, 200.0);
    }

    #[test]
    fn test_fee_info_freshness() {
        let fee_info = FeeInfo::new(100, 1, false, 100.0).unwrap();
        assert!(fee_info.is_fresh(60)); // within 1 minute
    }

    #[test]
    fn test_fee_info_exceeds_threshold() {
        let fee_info = FeeInfo::new(100, 10, false, 100.0).unwrap();
        assert!(!fee_info.exceeds_threshold(0.001)); // 0.001 XLM threshold
        // total_fee_xlm == 0.0001; threshold uses strict `>`
        assert!(!fee_info.exceeds_threshold(0.0001));
        assert!(fee_info.exceeds_threshold(0.00005));
    }

    // Utility function tests

    #[test]
    fn test_estimate_payment_fee() {
        let fee = estimate_payment_fee(100).unwrap();
        assert_eq!(fee, 100);
    }

    #[test]
    fn test_estimate_donation_fee() {
        let fee = estimate_donation_fee(100).unwrap();
        assert_eq!(fee, 200); // 2 operations
    }

    #[test]
    fn test_estimate_multisig_setup_fee() {
        let fee = estimate_multisig_setup_fee(100).unwrap();
        assert_eq!(fee, 300); // 3 operations
    }

    #[test]
    fn test_estimate_contract_invocation_fee() {
        let fee = estimate_contract_invocation_fee(100, 1).unwrap();
        assert_eq!(fee, 100);

        let fee = estimate_contract_invocation_fee(100, 5).unwrap();
        assert_eq!(fee, 500);
    }

    #[test]
    fn test_estimate_fee_xlm() {
        let fee_xlm = estimate_fee_xlm(100, 1).unwrap();
        assert_eq!(fee_xlm, 0.00001);

        let fee_xlm = estimate_fee_xlm(100, 2).unwrap();
        assert_eq!(fee_xlm, 0.00002);
    }

    #[test]
    fn test_format_fee_xlm() {
        let formatted = format_fee_xlm(100, 8);
        assert_eq!(formatted, "0.00001000 XLM");

        let formatted = format_fee_xlm(10_000_000, 2);
        assert_eq!(formatted, "1.00 XLM");
    }

    #[test]
    fn test_format_fee_stroops() {
        let formatted = format_fee_stroops(1_000);
        assert_eq!(formatted, "1,000 stroops");

        let formatted = format_fee_stroops(100);
        assert_eq!(formatted, "100 stroops");
    }

    #[test]
    fn test_percentage_above_base() {
        let percent = percentage_above_base(100);
        assert_eq!(percent, 0.0); // Normal fee

        let percent = percentage_above_base(200);
        assert_eq!(percent, 100.0); // 100% above normal

        let percent = percentage_above_base(150);
        assert_eq!(percent, 50.0); // 50% above normal
    }

    #[test]
    fn test_is_fee_acceptable() {
        assert!(is_fee_acceptable(100, 0.0001)); // 0.00001 XLM
        assert!(!is_fee_acceptable(100_000, 0.00001)); // 0.01 XLM

        let acceptable_fee = xlm_to_stroops(0.001);
        assert!(is_fee_acceptable(acceptable_fee - 1, 0.001));
        assert!(!is_fee_acceptable(acceptable_fee + 1, 0.001));
    }

    #[test]
    fn test_estimate_time_to_fee_reduction() {
        let time = estimate_time_to_fee_reduction(100, false);
        assert_eq!(time, 0); // Normal fee, no wait

        let time = estimate_time_to_fee_reduction(200, false);
        assert!(time > 0); // Elevated fee, some wait expected

        let time = estimate_time_to_fee_reduction(500, true);
        assert!(time > 10); // Severe congestion with trend, significant wait
    }
}
