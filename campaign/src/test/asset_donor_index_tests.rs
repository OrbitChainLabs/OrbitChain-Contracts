//! Tests for the asset→donors inverse index (issue #119).
//!
//! Covers index maintenance on donate (first-donation dedup), isolation
//! between assets, the paginated read API (windows, tail, out-of-range), and
//! the empty case for pre-index contract state.

#![cfg(test)]

use soroban_sdk::testutils::{Address as AddressTestUtils, Ledger};
use soroban_sdk::{Address, Env, String, Vec};

use crate::types::{AssetInfo, CampaignData, CampaignStatus, StellarAsset};
use crate::CampaignContract;

fn make_env() -> Env {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    env.ledger().with_mut(|l| {
        l.sequence_number = 1_000;
        l.timestamp = 1_000_000;
        l.max_entry_ttl = 10_000_000;
        l.min_persistent_entry_ttl = 100;
    });
    env
}

/// Campaign accepting two assets, seeded directly (index behaviour is what's
/// under test, not initialize's validation).
fn setup_two_asset_campaign(env: &Env, token_a: &Address, token_b: &Address) {
    let creator = Address::generate(env);
    let campaign = CampaignData {
        creator,
        goal_amount: 100_000,
        raised_amount: 0,
        end_time: env.ledger().timestamp() + 86_400 * 30,
        status: CampaignStatus::Active,
        accepted_assets: Vec::from_array(
            env,
            [
                StellarAsset {
                    asset_code: String::from_str(env, "XLM"),
                    issuer: Some(token_a.clone()),
                },
                StellarAsset {
                    asset_code: String::from_str(env, "USDC"),
                    issuer: Some(token_b.clone()),
                },
            ],
        ),
        milestone_count: 0,
        min_donation_amount: 0,
        created_at_ledger: env.ledger().sequence(),
        created_at_time: env.ledger().timestamp(),
        concluded_at_ledger: None,
    };
    crate::storage::set_campaign(env, &campaign);
}

#[test]
fn donate_indexes_each_donor_once_per_asset() {
    let env = make_env();
    let token_a = Address::generate(&env);
    let token_b = Address::generate(&env);
    let cid = env.register_contract(None, CampaignContract);

    let donors: [Address; 3] = [
        Address::generate(&env),
        Address::generate(&env),
        Address::generate(&env),
    ];

    env.as_contract(&cid, || setup_two_asset_campaign(&env, &token_a, &token_b));

    // donor0 gives twice in asset A (must index once), donor1 once in A,
    // donor2 once in B only.
    for (donor, token, amount) in [
        (&donors[0], &token_a, 100i128),
        (&donors[1], &token_a, 200),
        (&donors[0], &token_a, 300),
        (&donors[2], &token_b, 400),
    ] {
        let asset = AssetInfo::Stellar(token.clone());
        env.as_contract(&cid, || {
            CampaignContract::donate(env.clone(), donor.clone(), amount, asset.clone())
        });
    }

    env.as_contract(&cid, || {
        // Asset A: donor0 and donor1 exactly once each, in first-donation order.
        assert_eq!(
            CampaignContract::get_asset_donor_count(env.clone(), token_a.clone()),
            2
        );
        let a_donors = CampaignContract::get_asset_donors(env.clone(), token_a.clone(), 0, 10);
        assert_eq!(a_donors.len(), 2);
        assert_eq!(a_donors.get(0).unwrap(), donors[0]);
        assert_eq!(a_donors.get(1).unwrap(), donors[1]);

        // Asset B is isolated: only donor2.
        assert_eq!(
            CampaignContract::get_asset_donor_count(env.clone(), token_b.clone()),
            1
        );
        let b_donors = CampaignContract::get_asset_donors(env.clone(), token_b.clone(), 0, 10);
        assert_eq!(b_donors.len(), 1);
        assert_eq!(b_donors.get(0).unwrap(), donors[2]);
    });
}

#[test]
fn pagination_windows_slice_the_index() {
    let env = make_env();
    let token = Address::generate(&env);
    let other = Address::generate(&env);
    let cid = env.register_contract(None, CampaignContract);

    env.as_contract(&cid, || setup_two_asset_campaign(&env, &token, &other));

    let mut donors: [Option<Address>; 5] = [None, None, None, None, None];
    for slot in donors.iter_mut() {
        let donor = Address::generate(&env);
        *slot = Some(donor.clone());
        let asset = AssetInfo::Stellar(token.clone());
        env.as_contract(&cid, || {
            CampaignContract::donate(env.clone(), donor.clone(), 50, asset.clone())
        });
    }

    env.as_contract(&cid, || {
        assert_eq!(
            CampaignContract::get_asset_donor_count(env.clone(), token.clone()),
            5
        );

        // Full window.
        assert_eq!(
            CampaignContract::get_asset_donors(env.clone(), token.clone(), 0, 5).len(),
            5
        );
        // Middle window preserves order.
        let mid = CampaignContract::get_asset_donors(env.clone(), token.clone(), 1, 2);
        assert_eq!(mid.len(), 2);
        assert_eq!(mid.get(0).unwrap(), donors[1].clone().unwrap());
        assert_eq!(mid.get(1).unwrap(), donors[2].clone().unwrap());
        // Tail window clamps to the end.
        assert_eq!(
            CampaignContract::get_asset_donors(env.clone(), token.clone(), 4, 10).len(),
            1
        );
        // Out-of-range start → empty.
        assert_eq!(
            CampaignContract::get_asset_donors(env.clone(), token.clone(), 5, 10).len(),
            0
        );
        // Zero limit → empty.
        assert_eq!(
            CampaignContract::get_asset_donors(env.clone(), token.clone(), 0, 0).len(),
            0
        );
    });
}

#[test]
fn unindexed_asset_reads_as_empty() {
    let env = make_env();
    let token = Address::generate(&env);
    let cid = env.register_contract(None, CampaignContract);
    env.as_contract(&cid, || {
        assert_eq!(
            CampaignContract::get_asset_donor_count(env.clone(), token.clone()),
            0
        );
        assert_eq!(
            CampaignContract::get_asset_donors(env.clone(), token.clone(), 0, 10).len(),
            0
        );
    });
}
