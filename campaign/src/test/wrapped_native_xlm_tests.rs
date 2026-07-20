//! Tests for issue #105 — sender-side Soroban-native (wrapped) XLM handling.
//!
//! Verifies that Native → wrapped XLM is resolved once at initialize, cached
//! in storage, and reused on the donate hot path without scanning asset codes.

#![cfg(test)]

use soroban_sdk::testutils::Address as AddressTestUtils;
use soroban_sdk::{Address, BytesN, Env, String, Vec};

use super::with_contract;
use crate::contract::{resolve_and_cache_wrapped_native_xlm, resolve_wrapped_native_xlm};
use crate::storage::{get_donor_asset_donation, get_wrapped_native_xlm};
use crate::types::{AssetInfo, MilestoneData, MilestoneStatus, StellarAsset};
use crate::CampaignContract;

fn xlm_code(env: &Env) -> String {
    String::from_str(env, "XLM")
}

fn milestone(env: &Env, target: i128) -> Vec<MilestoneData> {
    let mut milestones = Vec::new(env);
    milestones.push_back(MilestoneData {
        index: 0,
        target_amount: target,
        released_amount: 0,
        description_hash: BytesN::from_array(env, &[0u8; 32]),
        status: MilestoneStatus::Locked,
        released_at: None,
        released_at_ledger: None,
        release_tx: None,
        released_to: None,
    });
    milestones
}

/// Legacy linear scan used only for before/after CPU comparison.
fn legacy_scan_xlm_issuer(env: &Env, assets: &Vec<StellarAsset>) -> Option<Address> {
    let code = xlm_code(env);
    assets
        .iter()
        .find(|a| a.asset_code == code)
        .and_then(|a| a.issuer.clone())
}

fn filler_asset_code(env: &Env, index: u32) -> String {
    // Fixed codes so we never depend on `format!` under `#![no_std]`.
    let codes: [&str; 32] = [
        "A00", "A01", "A02", "A03", "A04", "A05", "A06", "A07", "A08", "A09", "A10", "A11", "A12",
        "A13", "A14", "A15", "A16", "A17", "A18", "A19", "A20", "A21", "A22", "A23", "A24", "A25",
        "A26", "A27", "A28", "A29", "A30", "A31",
    ];
    String::from_str(env, codes[index as usize])
}

#[test]
fn test_initialize_caches_wrapped_xlm_from_issuer() {
    let env = Env::default();
    env.mock_all_auths();
    with_contract(&env, || {
        let creator = Address::generate(&env);
        let wrapped_xlm = Address::generate(&env);
        let mut assets = Vec::new(&env);
        assets.push_back(StellarAsset {
            asset_code: xlm_code(&env),
            issuer: Some(wrapped_xlm.clone()),
        });

        CampaignContract::initialize(
            env.clone(),
            creator,
            1000,
            env.ledger().timestamp() + 86_400,
            assets,
            milestone(&env, 1000),
            0,
        )
        .unwrap();

        let cached = get_wrapped_native_xlm(&env).expect("wrapped XLM should be cached");
        assert_eq!(cached, wrapped_xlm);
    });
}

#[test]
fn test_multi_asset_campaign_exact_xlm_still_works() {
    let env = Env::default();
    env.mock_all_auths();
    with_contract(&env, || {
        let creator = Address::generate(&env);
        let wrapped_xlm = Address::generate(&env);
        let usdc = Address::generate(&env);
        // Prefix-like codes must not steal Native routing.
        let xlmusd = Address::generate(&env);

        let mut assets = Vec::new(&env);
        assets.push_back(StellarAsset {
            asset_code: String::from_str(&env, "USDC"),
            issuer: Some(usdc.clone()),
        });
        assets.push_back(StellarAsset {
            asset_code: String::from_str(&env, "XLMUSD"),
            issuer: Some(xlmusd.clone()),
        });
        assets.push_back(StellarAsset {
            asset_code: xlm_code(&env),
            issuer: Some(wrapped_xlm.clone()),
        });

        CampaignContract::initialize(
            env.clone(),
            creator,
            1000,
            env.ledger().timestamp() + 86_400,
            assets,
            milestone(&env, 1000),
            0,
        )
        .unwrap();

        assert_eq!(
            get_wrapped_native_xlm(&env),
            Some(wrapped_xlm.clone()),
            "exact XLM entry must win over XLMUSD / USDC"
        );

        let donor = Address::generate(&env);
        CampaignContract::donate(env.clone(), donor.clone(), 250, AssetInfo::Native);

        assert_eq!(
            get_donor_asset_donation(&env, &donor, &wrapped_xlm),
            250,
            "Native donation must credit the exact XLM wrapped address"
        );
        assert_eq!(get_donor_asset_donation(&env, &donor, &usdc), 0);
        assert_eq!(get_donor_asset_donation(&env, &donor, &xlmusd), 0);
    });
}

#[test]
#[should_panic(expected = "HostError")]
fn test_native_without_xlm_panics() {
    let env = Env::default();
    env.mock_all_auths();
    with_contract(&env, || {
        let creator = Address::generate(&env);
        let mut assets = Vec::new(&env);
        assets.push_back(StellarAsset {
            asset_code: String::from_str(&env, "USDC"),
            issuer: Some(Address::generate(&env)),
        });

        CampaignContract::initialize(
            env.clone(),
            creator,
            1000,
            env.ledger().timestamp() + 86_400,
            assets,
            milestone(&env, 1000),
            0,
        )
        .unwrap();

        assert!(get_wrapped_native_xlm(&env).is_none());

        let donor = Address::generate(&env);
        CampaignContract::donate(env.clone(), donor, 100, AssetInfo::Native);
    });
}

#[test]
fn test_issuer_none_resolves_via_deployer_sac() {
    let env = Env::default();
    env.mock_all_auths();
    with_contract(&env, || {
        let mut assets = Vec::new(&env);
        assets.push_back(StellarAsset {
            asset_code: xlm_code(&env),
            issuer: None,
        });

        let resolved = resolve_wrapped_native_xlm(&env, &assets)
            .expect("issuer:None XLM should resolve via deployer");
        resolve_and_cache_wrapped_native_xlm(&env, &assets);

        assert_eq!(get_wrapped_native_xlm(&env), Some(resolved));
    });
}

/// Hot-path benchmark: cached lookup should use fewer CPU instructions than
/// a linear scan over a large accepted_assets list (issue #105 acceptance).
#[test]
fn test_hot_path_cache_beats_linear_scan() {
    let env = Env::default();
    env.mock_all_auths();
    with_contract(&env, || {
        let wrapped_xlm = Address::generate(&env);
        let mut assets = Vec::new(&env);
        // Put XLM last so a linear scan does maximal work.
        for i in 0..32u32 {
            assets.push_back(StellarAsset {
                asset_code: filler_asset_code(&env, i),
                issuer: Some(Address::generate(&env)),
            });
        }
        assets.push_back(StellarAsset {
            asset_code: xlm_code(&env),
            issuer: Some(wrapped_xlm.clone()),
        });

        resolve_and_cache_wrapped_native_xlm(&env, &assets);
        assert_eq!(get_wrapped_native_xlm(&env), Some(wrapped_xlm.clone()));

        env.cost_estimate().budget().reset_default();
        let cached = get_wrapped_native_xlm(&env).unwrap();
        let cache_cpu = env.cost_estimate().budget().cpu_instruction_cost();

        env.cost_estimate().budget().reset_default();
        let scanned = legacy_scan_xlm_issuer(&env, &assets).unwrap();
        let scan_cpu = env.cost_estimate().budget().cpu_instruction_cost();

        assert_eq!(cached, scanned);
        assert_eq!(cached, wrapped_xlm);
        assert!(
            cache_cpu < scan_cpu,
            "cached lookup ({cache_cpu}) should beat linear scan ({scan_cpu})"
        );
    });
}
