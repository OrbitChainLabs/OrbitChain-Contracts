//! Tests for the memoised dashboard report (issue #121).
//!
//! The invariant under test: after EVERY state-changing entrypoint, the
//! cached report equals a freshly recomputed one — i.e. the cache can never
//! be observed stale. Covers donate (incl. the GoalReached transition),
//! release_milestone, extend_deadline, end_campaign, cancel_campaign, and
//! claim_refund, plus the fallback path for pre-cache contracts.

#![cfg(test)]

use soroban_sdk::testutils::{Address as AddressTestUtils, Ledger};
use soroban_sdk::token::StellarAssetClient;
use soroban_sdk::{Address, BytesN, Env, String, Vec};

use super::with_contract;
use crate::storage::{get_cached_report_storage, set_campaign, set_milestone};
use crate::types::{
    AssetInfo, CampaignData, CampaignStatus, MilestoneData, MilestoneStatus, StellarAsset,
};
use crate::CampaignContract;

fn make_env() -> Env {
    let env = Env::default();
    // allowing_non_root: the SAC mint and refund paths authorize from
    // nested invocations (client calls inside the campaign frame).
    env.mock_all_auths_allowing_non_root_auth();
    env.ledger().with_mut(|l| {
        l.sequence_number = 1_000;
        l.timestamp = 1_000_000;
        l.max_entry_ttl = 10_000_000;
        l.min_persistent_entry_ttl = 100;
    });
    env
}

/// A real mock SAC — `release_milestone` does cross-contract `balance()` /
/// `transfer()` calls, which panic against a bare generated address.
/// v1 registration is what works inside a `with_contract` frame in SDK 26
/// (same pattern and rationale as release_milestone_tests).
#[allow(deprecated)]
fn asset(env: &Env) -> (Address, AssetInfo) {
    let admin = Address::generate(env);
    let token = env.register_stellar_asset_contract(admin);
    (token.clone(), AssetInfo::Stellar(token))
}

/// Fund the campaign contract so the release path's transfer succeeds.
fn mint_to_contract(env: &Env, token: &Address, amount: i128) {
    StellarAssetClient::new(env, token).mint(&env.current_contract_address(), &amount);
}

fn initialize_campaign(env: &Env, creator: &Address, token: &Address, goal: i128) {
    let assets: Vec<StellarAsset> = Vec::from_array(
        env,
        [StellarAsset {
            asset_code: String::from_str(env, "XLM"),
            issuer: Some(token.clone()),
        }],
    );
    let milestones: Vec<MilestoneData> = Vec::from_array(
        env,
        [MilestoneData {
            index: 0,
            target_amount: goal,
            released_amount: 0,
            description_hash: BytesN::from_array(env, &[9u8; 32]),
            status: MilestoneStatus::Locked,
            released_at: None,
            released_at_ledger: None,
            release_tx: None,
            released_to: None,
        }],
    );
    CampaignContract::initialize(
        env.clone(),
        creator.clone(),
        goal,
        env.ledger().timestamp() + 86_400 * 30,
        assets,
        milestones,
        0,
    )
    .unwrap();
}

/// The invariant: the stored cache must equal a fresh recompute.
fn assert_cache_fresh(env: &Env, context: &str) {
    let cached =
        get_cached_report_storage(env).unwrap_or_else(|| panic!("no cache after {context}"));
    let fresh = CampaignContract::get_campaign_report(env.clone()).unwrap();
    assert_eq!(cached, fresh, "stale cache after {context}");
}

#[test]
fn cache_is_fresh_after_every_transition() {
    let env = make_env();
    let (token, asset_info) = asset(&env);
    // One `as_contract` frame per invocation — mirroring real transactions —
    // sidesteps the "frame is already authorized" collisions that arise when
    // the same address re-authorizes within a single frame.
    let cid = env.register_contract(None, CampaignContract);
    let creator = Address::generate(&env);
    let donor_a = Address::generate(&env);
    let donor_b = Address::generate(&env);
    let recipient = Address::generate(&env);

    env.as_contract(&cid, || initialize_campaign(&env, &creator, &token, 1_000));
    env.as_contract(&cid, || assert_cache_fresh(&env, "initialize"));

    // Partial donation: raised/progress/donation_count change.
    env.as_contract(&cid, || {
        CampaignContract::donate(env.clone(), donor_a.clone(), 400, asset_info.clone())
    });
    env.as_contract(&cid, || {
        assert_cache_fresh(&env, "partial donate");
        assert_eq!(get_cached_report_storage(&env).unwrap().raised_amount, 400);
    });

    // Goal-reaching donation: status flips, milestone unlocks.
    env.as_contract(&cid, || {
        CampaignContract::donate(env.clone(), donor_b.clone(), 600, asset_info.clone())
    });
    env.as_contract(&cid, || {
        assert_cache_fresh(&env, "goal-reaching donate");
        assert_eq!(
            get_cached_report_storage(&env).unwrap().status,
            CampaignStatus::GoalReached
        );
    });

    // Milestone release: release_count changes. Fund the contract first —
    // the release path pays out from the contract's real SAC balance.
    env.as_contract(&cid, || mint_to_contract(&env, &token, 1_000));
    env.as_contract(&cid, || {
        CampaignContract::release_milestone(env.clone(), 0, recipient.clone())
    });
    env.as_contract(&cid, || {
        assert_cache_fresh(&env, "release_milestone");
        assert_eq!(get_cached_report_storage(&env).unwrap().release_count, 1);
    });

    // Deadline extension: end_time changes.
    let new_end = env.ledger().timestamp() + 86_400 * 60;
    env.as_contract(&cid, || {
        CampaignContract::extend_deadline(env.clone(), new_end)
    });
    env.as_contract(&cid, || {
        assert_cache_fresh(&env, "extend_deadline");
        assert_eq!(get_cached_report_storage(&env).unwrap().end_time, new_end);
    });

    // Ending: status changes.
    env.as_contract(&cid, || CampaignContract::end_campaign(env.clone()));
    env.as_contract(&cid, || {
        assert_cache_fresh(&env, "end_campaign");
        assert_eq!(
            get_cached_report_storage(&env).unwrap().status,
            CampaignStatus::Ended
        );
    });
}

#[test]
fn cache_is_fresh_after_cancel_and_refund() {
    let env = make_env();
    let (token, asset_info) = asset(&env);
    let cid = env.register_contract(None, CampaignContract);
    let creator = Address::generate(&env);
    let donor = Address::generate(&env);

    env.as_contract(&cid, || initialize_campaign(&env, &creator, &token, 1_000));
    env.as_contract(&cid, || {
        CampaignContract::donate(env.clone(), donor.clone(), 300, asset_info.clone())
    });

    env.as_contract(&cid, || CampaignContract::cancel_campaign(env.clone()));
    env.as_contract(&cid, || {
        assert_cache_fresh(&env, "cancel_campaign");
        assert_eq!(
            get_cached_report_storage(&env).unwrap().status,
            CampaignStatus::Cancelled
        );
    });

    let eligible = env.as_contract(&cid, || {
        CampaignContract::is_refund_eligible(env.clone(), donor.clone())
    });
    if eligible {
        env.as_contract(&cid, || mint_to_contract(&env, &token, 300));
        env.as_contract(&cid, || {
            CampaignContract::claim_refund(env.clone(), donor.clone())
        });
        env.as_contract(&cid, || assert_cache_fresh(&env, "claim_refund"));
    }
}

#[test]
fn get_cached_report_falls_back_for_pre_cache_state() {
    let env = make_env();
    with_contract(&env, || {
        // Seed campaign state directly (as a contract instance that predates
        // the cache would have it) — no cache entry exists.
        let creator = Address::generate(&env);
        let campaign = CampaignData {
            creator,
            goal_amount: 500,
            raised_amount: 100,
            end_time: env.ledger().timestamp() + 86_400,
            status: CampaignStatus::Active,
            accepted_assets: Vec::new(&env),
            milestone_count: 0,
            min_donation_amount: 0,
            created_at_ledger: env.ledger().sequence(),
            created_at_time: env.ledger().timestamp(),
            concluded_at_ledger: None,
        };
        set_campaign(&env, &campaign);
        let _ = set_milestone; // (helper imported for parity with sibling tests)

        assert!(get_cached_report_storage(&env).is_none());
        let report = CampaignContract::get_cached_report(env.clone()).unwrap();
        assert_eq!(report.raised_amount, 100);
        // The read path stays read-only: it must not have populated the cache.
        assert!(get_cached_report_storage(&env).is_none());
    });
}

#[test]
fn get_cached_report_returns_none_when_uninitialized() {
    let env = make_env();
    with_contract(&env, || {
        assert_eq!(CampaignContract::get_cached_report(env.clone()), None);
    });
}
