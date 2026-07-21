//! OrbitChain testkit — shared test utilities for the campaign contract.
//!
//! Provides helper functions for setting up test environments, generating
//! mock data, and registering the campaign contract in Soroban test environments.

#![allow(clippy::expect_used)]

use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, BytesN, Env, String, Vec};

use orbitchain_campaign::types::{
    CampaignData, CampaignStatus, MilestoneData, MilestoneStatus, StellarAsset,
};

/// Base timestamp: 1 year in seconds, same convention as other test files.
pub const BASE: u64 = 86_400 * 365;

/// Register the campaign contract and run `f` inside `env.as_contract()`.
///
/// Call `env.mock_all_auths()` **before** this if the body needs auth.
pub fn with_contract<F, R>(env: &Env, f: F) -> R
where
    F: FnOnce() -> R,
{
    #[allow(deprecated)]
    let contract_id = env.register_contract(None, orbitchain_campaign::CampaignContract);
    env.as_contract(&contract_id, f)
}

/// Create a `StellarAsset` with the given code and optional issuer address.
pub fn make_asset(env: &Env, code: &str, issuer: Option<Address>) -> StellarAsset {
    StellarAsset {
        asset_code: String::from_str(env, code),
        issuer,
    }
}

/// Build a `Vec<StellarAsset>` from a list of `(code, issuer)` pairs.
pub fn make_assets(env: &Env, pairs: &[(&str, Option<Address>)]) -> Vec<StellarAsset> {
    let mut assets = Vec::new(env);
    for (code, issuer) in pairs {
        assets.push_back(make_asset(env, code, issuer.clone()));
    }
    assets
}

/// Build milestone data from target amounts. All milestones start as `Locked`.
pub fn make_milestones(env: &Env, targets: &[i128]) -> Vec<MilestoneData> {
    let mut milestones = Vec::new(env);
    for (i, &target) in targets.iter().enumerate() {
        milestones.push_back(MilestoneData {
            index: i as u32,
            target_amount: target,
            released_amount: 0,
            description_hash: BytesN::from_array(env, &[i as u8; 32]),
            status: MilestoneStatus::Locked,
            released_at: None,
            released_at_ledger: None,
            release_tx: None,
            released_to: None,
        });
    }
    milestones
}

/// Build a `CampaignData` with common defaults.
#[allow(clippy::too_many_arguments)]
pub fn make_campaign(
    env: &Env,
    creator: Address,
    goal_amount: i128,
    raised_amount: i128,
    end_time: u64,
    status: CampaignStatus,
    assets: Vec<StellarAsset>,
    milestone_count: u32,
) -> CampaignData {
    CampaignData {
        creator,
        goal_amount,
        raised_amount,
        end_time,
        status,
        accepted_assets: assets,
        milestone_count,
        min_donation_amount: 0,
        created_at_ledger: env.ledger().sequence(),
        created_at_time: env.ledger().timestamp(),
        concluded_at_ledger: None,
    }
}

/// Compute evenly-spaced milestone targets for a given goal and count.
///
/// Returns a `std::vec::Vec` so callers can pass it directly to
/// `make_milestones` as a slice.
pub fn compute_milestone_targets(goal: i128, count: usize) -> std::vec::Vec<i128> {
    if count <= 1 {
        return vec![goal];
    }
    let step = goal / count as i128;
    (1..=count)
        .map(|i| {
            if i == count {
                goal
            } else {
                (step * i as i128).max(1)
            }
        })
        .collect()
}

/// Build the standard XLM asset list accepted by tests.
///
/// Returns `(assets, issuer_address)` so callers can reference the issuer
/// address when constructing `AssetInfo::Native` donations.
pub fn xlm_assets(env: &Env) -> (Vec<StellarAsset>, Address) {
    let issuer = Address::generate(env);
    let assets = make_assets(env, &[("XLM", Some(issuer.clone()))]);
    (assets, issuer)
}
