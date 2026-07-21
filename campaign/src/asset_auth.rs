//! Defence-in-depth asset whitelist enforcement (issue #89).
//!
//! Provides [`assert_asset_is_accepted`] which must be called at the very top
//! of every mutating entry-point (`donate`, `claim_refund`, etc.) *before* any
//! state mutations.  This ensures a missing or refactored guard deeper in the
//! call stack can never silently accept an unauthorised asset.

use crate::storage::get_campaign;
use crate::types::{AssetInfo, CampaignData, Error};
use soroban_sdk::{panic_with_error, Env};

/// Panic with [`Error::AssetNotAccepted`] if `asset` is not in the campaign's
/// `accepted_assets` list.
///
/// This is the single source of truth for early-exit asset validation.  It
/// reads campaign data from storage and matches against `accepted_assets`
/// without performing any writes.
///
/// # Panics
/// Panics with [`Error::AssetNotAccepted`] when the asset is not whitelisted.
pub fn assert_asset_is_accepted(env: &Env, asset: &AssetInfo, campaign: &CampaignData) {
    match asset {
        AssetInfo::Stellar(addr) => {
            let accepted = campaign
                .accepted_assets
                .iter()
                .any(|a| a.issuer.as_ref() == Some(addr));
            if !accepted {
                panic_with_error!(env, Error::AssetNotAccepted);
            }
        }
        AssetInfo::Native => {
            let xlm_code = soroban_sdk::String::from_str(env, "XLM");
            let has_xlm = campaign
                .accepted_assets
                .iter()
                .any(|a| a.asset_code == xlm_code && a.issuer.is_some());
            if !has_xlm {
                panic_with_error!(env, Error::AssetNotAccepted);
            }
        }
    }
}

/// Returns `true` if `asset` is present in the campaign's accepted assets.
///
/// This is a non-panicking read-only view suitable for off-chain clients and
/// preview queries.
pub fn is_asset_accepted(env: &Env, asset: &AssetInfo) -> bool {
    let campaign = match get_campaign(env) {
        Some(c) => c,
        None => return false,
    };
    is_asset_in_list(env, asset, &campaign)
}

/// Internal check against a campaign's accepted assets list without panicking.
fn is_asset_in_list(_env: &Env, asset: &AssetInfo, campaign: &CampaignData) -> bool {
    match asset {
        AssetInfo::Stellar(addr) => campaign
            .accepted_assets
            .iter()
            .any(|a| a.issuer.as_ref() == Some(addr)),
        AssetInfo::Native => {
            let xlm_code = soroban_sdk::String::from_str(_env, "XLM");
            campaign
                .accepted_assets
                .iter()
                .any(|a| a.asset_code == xlm_code && a.issuer.is_some())
        }
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{CampaignStatus, StellarAsset};
    use crate::test::with_contract;
    use soroban_sdk::{testutils::Address as _, Address, Env, String, Vec};

    fn make_campaign(env: &Env, accepted_assets: Vec<StellarAsset>) -> CampaignData {
        CampaignData {
            creator: Address::generate(env),
            goal_amount: 10_000,
            raised_amount: 0,
            end_time: env.ledger().timestamp() + 86_400,
            status: CampaignStatus::Active,
            accepted_assets,
            milestone_count: 1,
            min_donation_amount: 0,
            created_at_ledger: 0,
            created_at_time: 0,
            concluded_at_ledger: None,
        }
    }

    fn usdc_asset(env: &Env) -> StellarAsset {
        let issuer = Address::generate(env);
        StellarAsset {
            asset_code: String::from_str(env, "USDC"),
            issuer: Some(issuer),
        }
    }

    fn native_xlm_asset(env: &Env) -> StellarAsset {
        let issuer = Address::generate(env);
        StellarAsset {
            asset_code: String::from_str(env, "XLM"),
            issuer: Some(issuer),
        }
    }

    // ── assert_asset_is_accepted ────────────────────────────────────────────

    #[test]
    fn stellar_accepted_asset_passes() {
        let env = Env::default();
        let usdc = usdc_asset(&env);
        let issuer_addr = usdc.issuer.clone().unwrap();
        let campaign = make_campaign(&env, {
            let mut v = Vec::new(&env);
            v.push_back(usdc.clone());
            v
        });

        with_contract(&env, || {
            assert_asset_is_accepted(&env, &AssetInfo::Stellar(issuer_addr), &campaign);
        });
    }

    #[test]
    #[should_panic(expected = "HostError")]
    fn stellar_unaccepted_asset_panics() {
        let env = Env::default();
        let campaign = make_campaign(&env, {
            let mut v = Vec::new(&env);
            v.push_back(usdc_asset(&env));
            v
        });
        let unknown = Address::generate(&env);

        with_contract(&env, || {
            assert_asset_is_accepted(&env, &AssetInfo::Stellar(unknown), &campaign);
        });
    }

    #[test]
    fn native_xlm_accepted_passes() {
        let env = Env::default();
        let xlm = native_xlm_asset(&env);
        let campaign = make_campaign(&env, {
            let mut v = Vec::new(&env);
            v.push_back(xlm);
            v
        });

        with_contract(&env, || {
            assert_asset_is_accepted(&env, &AssetInfo::Native, &campaign);
        });
    }

    #[test]
    #[should_panic(expected = "HostError")]
    fn native_xlm_not_in_list_panics() {
        let env = Env::default();
        let campaign = make_campaign(&env, {
            let mut v = Vec::new(&env);
            v.push_back(usdc_asset(&env));
            v
        });

        with_contract(&env, || {
            assert_asset_is_accepted(&env, &AssetInfo::Native, &campaign);
        });
    }

    #[test]
    #[should_panic(expected = "HostError")]
    fn empty_accepted_assets_panics() {
        let env = Env::default();
        let campaign = make_campaign(&env, Vec::new(&env));

        with_contract(&env, || {
            let addr = Address::generate(&env);
            assert_asset_is_accepted(&env, &AssetInfo::Stellar(addr), &campaign);
        });
    }

    // ── is_asset_accepted (view) ────────────────────────────────────────────

    #[test]
    fn view_returns_true_for_accepted() {
        let env = Env::default();
        let usdc = usdc_asset(&env);
        let issuer_addr = usdc.issuer.clone().unwrap();

        with_contract(&env, || {
            crate::storage::set_campaign(
                &env,
                &make_campaign(&env, {
                    let mut v = Vec::new(&env);
                    v.push_back(usdc);
                    v
                }),
            );
            assert!(is_asset_accepted(&env, &AssetInfo::Stellar(issuer_addr)));
        });
    }

    #[test]
    fn view_returns_false_for_unaccepted() {
        let env = Env::default();
        let usdc = usdc_asset(&env);

        with_contract(&env, || {
            crate::storage::set_campaign(
                &env,
                &make_campaign(&env, {
                    let mut v = Vec::new(&env);
                    v.push_back(usdc);
                    v
                }),
            );
            let unknown = Address::generate(&env);
            assert!(!is_asset_accepted(&env, &AssetInfo::Stellar(unknown)));
        });
    }

    #[test]
    fn view_returns_false_when_not_initialized() {
        let env = Env::default();
        with_contract(&env, || {
            let addr = Address::generate(&env);
            assert!(!is_asset_accepted(&env, &AssetInfo::Stellar(addr)));
        });
    }

    #[test]
    fn view_native_xlm_returns_true_when_accepted() {
        let env = Env::default();
        let xlm = native_xlm_asset(&env);

        with_contract(&env, || {
            crate::storage::set_campaign(
                &env,
                &make_campaign(&env, {
                    let mut v = Vec::new(&env);
                    v.push_back(xlm);
                    v
                }),
            );
            assert!(is_asset_accepted(&env, &AssetInfo::Native));
        });
    }

    // ── Storage manipulation cannot bypass ───────────────────────────────────

    #[test]
    #[should_panic(expected = "HostError")]
    fn direct_storage_write_does_not_bypass_early_guard() {
        let env = Env::default();
        let usdc = usdc_asset(&env);
        let unknown = Address::generate(&env);
        let donor = Address::generate(&env);

        with_contract(&env, || {
            crate::storage::set_campaign(
                &env,
                &make_campaign(&env, {
                    let mut v = Vec::new(&env);
                    v.push_back(usdc);
                    v
                }),
            );

            // Attempt to inject a fake DonorAssetDonation for the unknown asset
            crate::storage::increment_donor_asset_donation(
                &env,
                &donor,
                &unknown,
                500,
            );

            // The early guard must still reject the unknown asset
            let campaign = get_campaign(&env).unwrap();
            assert_asset_is_accepted(
                &env,
                &AssetInfo::Stellar(unknown),
                &campaign,
            );
        });
    }
}
