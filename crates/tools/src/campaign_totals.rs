use anyhow::{anyhow, Result};
use std::collections::HashMap;

/// In-memory store for per-campaign donation totals, grouped by asset.
///
/// Issue #141 – Track Total Volume by Asset
#[derive(Default, Debug, Clone)]
pub struct CampaignTotals {
    /// (campaign_id, asset) → total
    asset_totals: HashMap<(u64, String), i128>,
}

impl CampaignTotals {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds `amount` to the running total for `campaign_id` + `asset` and returns the new total.
    ///
    /// Returns `Err` if `amount` is non-positive or if the addition would overflow.
    #[inline]
    pub fn increment(&mut self, campaign_id: u64, asset: &str, amount: i128) -> Result<i128> {
        if amount <= 0 {
            return Err(anyhow!("CampaignTotals::increment requires positive amount; got {}", amount));
        }
        let entry = self.asset_totals.entry((campaign_id, asset.to_string())).or_insert(0);
        *entry = entry.checked_add(amount).ok_or_else(|| anyhow!("overflow in CampaignTotals"))?;
        Ok(*entry)
    }

    /// Returns the total for a specific `campaign_id` + `asset`, or 0 if none recorded.
    #[must_use]
    pub fn get(&self, campaign_id: u64, asset: &str) -> i128 {
        *self.asset_totals.get(&(campaign_id, asset.to_string())).unwrap_or(&0)
    }

    /// Returns all asset totals for a campaign as a map of asset → total.
    #[must_use]
    pub fn get_all_assets(&self, campaign_id: u64) -> HashMap<String, i128> {
        self.asset_totals
            .iter()
            .filter(|((cid, _), _)| *cid == campaign_id)
            .map(|((_, asset), total)| (asset.clone(), *total))
            .collect()
    }

    /// Returns the aggregate total across all assets for a campaign.
    #[must_use]
    pub fn get_campaign_total(&self, campaign_id: u64) -> i128 {
        self.asset_totals
            .iter()
            .filter(|((cid, _), _)| *cid == campaign_id)
            .map(|(_, total)| total)
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_at_zero() {
        let totals = CampaignTotals::new();
        assert_eq!(totals.get(1, "XLM"), 0);
    }

    #[test]
    fn increments_per_asset() {
        let mut totals = CampaignTotals::new();
        totals.increment(1, "XLM", 500).unwrap();
        totals.increment(1, "XLM", 300).unwrap();
        assert_eq!(totals.get(1, "XLM"), 800);
    }

    #[test]
    fn different_assets_are_independent() {
        let mut totals = CampaignTotals::new();
        totals.increment(1, "XLM", 100).unwrap();
        totals.increment(1, "USDC", 200).unwrap();
        assert_eq!(totals.get(1, "XLM"), 100);
        assert_eq!(totals.get(1, "USDC"), 200);
    }

    #[test]
    fn different_campaigns_are_independent() {
        let mut totals = CampaignTotals::new();
        totals.increment(1, "XLM", 100).unwrap();
        totals.increment(2, "XLM", 200).unwrap();
        assert_eq!(totals.get(1, "XLM"), 100);
        assert_eq!(totals.get(2, "XLM"), 200);
    }

    #[test]
    fn get_all_assets_returns_correct_map() {
        let mut totals = CampaignTotals::new();
        totals.increment(1, "XLM", 500).unwrap();
        totals.increment(1, "USDC", 300).unwrap();
        let map = totals.get_all_assets(1);
        assert_eq!(map.get("XLM"), Some(&500));
        assert_eq!(map.get("USDC"), Some(&300));
    }

    #[test]
    fn campaign_total_aggregates_all_assets() {
        let mut totals = CampaignTotals::new();
        totals.increment(1, "XLM", 500).unwrap();
        totals.increment(1, "USDC", 300).unwrap();
        assert_eq!(totals.get_campaign_total(1), 800);
    }

    #[test]
    fn rejects_negative_amount() {
        let mut totals = CampaignTotals::new();
        let err = totals.increment(1, "XLM", -100).unwrap_err();
        assert!(err.to_string().contains("positive amount"));
    }

    #[test]
    fn rejects_zero_amount() {
        let mut totals = CampaignTotals::new();
        let err = totals.increment(1, "XLM", 0).unwrap_err();
        assert!(err.to_string().contains("positive amount"));
    }

    #[test]
    fn rejects_overflow() {
        let mut totals = CampaignTotals::new();
        totals.increment(1, "XLM", i128::MAX).unwrap();
        let err = totals.increment(1, "XLM", 1).unwrap_err();
        assert!(err.to_string().contains("overflow"));
    }
}
