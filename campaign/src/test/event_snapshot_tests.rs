//! Snapshot tests for emitted contract events (issue #115).
//!
//! Existing integration tests assert *behaviour*; these assert that events
//! look **exactly** like this — topic order, symbol names, payload shape —
//! because silent event-shape changes break off-chain indexers. Snapshots
//! are recorded once (`cargo insta review` / INSTA_UPDATE=always) and CI
//! fails on any unreviewed change.
//!
//! Determinism: `Address::generate` and contract ids are deterministic per
//! Env construction order, and events are rendered in their XDR form
//! (fully symbolic — no runtime object handles), so snapshots are stable
//! across runs and hosts.

#![cfg(test)]

use soroban_sdk::testutils::Events as _;
use soroban_sdk::testutils::{Address as AddressTestUtils, Ledger};
use soroban_sdk::{Address, BytesN, Env, String, Vec};

use std::format;
use std::string::String as StdString;

use crate::types::{AssetInfo, MilestoneData, MilestoneStatus, StellarAsset};
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

/// Render the last invocation's events in their XDR form — fully symbolic
/// (contract id, topics, payload), stable across runs, and exactly the wire
/// shape off-chain indexers consume.
fn render_events(env: &Env) -> StdString {
    let mut out = StdString::new();
    for ev in env.events().all().events() {
        out.push_str(&format!("{ev:#?}\n---\n"));
    }
    out
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
            description_hash: BytesN::from_array(env, &[3u8; 32]),
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

/// A real mock SAC (see release_milestone_tests for the SDK-26 rationale);
/// registered outside the contract frames so later require_auth calls work.
#[allow(deprecated)]
fn register_sac(env: &Env) -> Address {
    let admin = Address::generate(env);
    env.register_stellar_asset_contract(admin)
}

#[test]
fn donation_lifecycle_events_match_snapshot() {
    let env = make_env();
    let token = register_sac(&env);
    let cid = env.register_contract(None, CampaignContract);
    let creator = Address::generate(&env);
    let donor_a = Address::generate(&env);
    let donor_b = Address::generate(&env);
    let recipient = Address::generate(&env);
    let asset = AssetInfo::Stellar(token.clone());

    let mut log = StdString::new();

    env.as_contract(&cid, || initialize_campaign(&env, &creator, &token, 1_000));
    log.push_str("== initialize ==\n");
    log.push_str(&render_events(&env));

    env.as_contract(&cid, || {
        CampaignContract::donate(env.clone(), donor_a.clone(), 400, asset.clone())
    });
    log.push_str("== donate (partial) ==\n");
    log.push_str(&render_events(&env));

    env.as_contract(&cid, || {
        CampaignContract::donate(env.clone(), donor_b.clone(), 600, asset.clone())
    });
    log.push_str("== donate (reaches goal, unlocks milestone) ==\n");
    log.push_str(&render_events(&env));

    env.as_contract(&cid, || {
        soroban_sdk::token::StellarAssetClient::new(&env, &token)
            .mint(&env.current_contract_address(), &1_000i128)
    });
    env.as_contract(&cid, || {
        CampaignContract::release_milestone(env.clone(), 0, recipient.clone())
    });
    log.push_str("== release_milestone ==\n");
    log.push_str(&render_events(&env));

    env.as_contract(&cid, || CampaignContract::end_campaign(env.clone()));
    log.push_str("== end_campaign ==\n");
    log.push_str(&render_events(&env));

    insta::assert_snapshot!("donation_lifecycle_events", log);
}

#[test]
fn cancel_and_refund_events_match_snapshot() {
    let env = make_env();
    let token = register_sac(&env);
    let cid = env.register_contract(None, CampaignContract);
    let creator = Address::generate(&env);
    let donor = Address::generate(&env);
    let asset = AssetInfo::Stellar(token.clone());

    let mut log = StdString::new();

    env.as_contract(&cid, || initialize_campaign(&env, &creator, &token, 1_000));
    env.as_contract(&cid, || {
        CampaignContract::donate(env.clone(), donor.clone(), 300, asset.clone())
    });

    env.as_contract(&cid, || CampaignContract::cancel_campaign(env.clone()));
    log.push_str("== cancel_campaign ==\n");
    log.push_str(&render_events(&env));

    if env.as_contract(&cid, || {
        CampaignContract::is_refund_eligible(env.clone(), donor.clone())
    }) {
        env.as_contract(&cid, || {
            soroban_sdk::token::StellarAssetClient::new(&env, &token)
                .mint(&env.current_contract_address(), &300i128)
        });
        env.as_contract(&cid, || {
            CampaignContract::claim_refund(env.clone(), donor.clone())
        });
        log.push_str("== claim_refund ==\n");
        log.push_str(&render_events(&env));
    }

    insta::assert_snapshot!("cancel_refund_events", log);
}

#[test]
fn freeze_events_match_snapshot() {
    let env = make_env();
    let token = register_sac(&env);
    let cid = env.register_contract(None, CampaignContract);
    let creator = Address::generate(&env);

    env.as_contract(&cid, || initialize_campaign(&env, &creator, &token, 1_000));

    let mut log = StdString::new();
    env.as_contract(&cid, || CampaignContract::freeze(env.clone()));
    log.push_str("== freeze ==\n");
    log.push_str(&render_events(&env));
    env.as_contract(&cid, || CampaignContract::unfreeze(env.clone()));
    log.push_str("== unfreeze ==\n");
    log.push_str(&render_events(&env));

    insta::assert_snapshot!("freeze_events", log);
}
