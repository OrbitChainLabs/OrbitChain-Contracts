//! Tests for concluded_at_ledger field.
//!
//! Verifies that `concluded_at_ledger` is set correctly when a campaign ends
//! or is cancelled.

#![cfg(test)]

use soroban_sdk::{Env, String};
use crate::{
    contract::{end_campaign, cancel_campaign},
    storage::{get_campaign, init_campaign},
    types::{CampaignData, CampaignStatus, StellarAsset},
};

fn setup_test_campaign(env: &Env) -> CampaignData {
    let creator = Address::generate(env);
    let asset_code = String::from_str(env, "XLM");
    let accepted_assets = vec![
        env,
        StellarAsset {
            asset_code,
            issuer: None,
        },
    ];

    let campaign = CampaignData {
        creator: creator.clone(),
        goal_amount: 100_000_000,
        raised_amount: 0,
        end_time: env.ledger().timestamp() + 86400 * 30,
        status: CampaignStatus::Active,
        accepted_assets,
        milestone_count: 1,
        min_donation_amount: 0,
        created_at_ledger: env.ledger().sequence(),
        created_at_time: env.ledger().timestamp(),
        concluded_at_ledger: None,
    };

    init_campaign(env, &campaign);
    campaign
}

#[test]
fn test_concluded_at_ledger_set_on_end() {
    let env = Env::default();
    env.ledger().with_mock(|ledger| {
        ledger.sequence = 1000;
        ledger.timestamp = 1234567890;
    });

    setup_test_campaign(&env);

    end_campaign(&env);

    let campaign = get_campaign(&env).unwrap();
    assert_eq!(campaign.concluded_at_ledger, Some(1000));
    assert_eq!(campaign.status, CampaignStatus::Ended);
}

#[test]
fn test_concluded_at_ledger_set_on_cancel() {
    let env = Env::default();
    env.ledger().with_mock(|ledger| {
        ledger.sequence = 2000;
        ledger.timestamp = 1234567890;
    });

    setup_test_campaign(&env);

    cancel_campaign(&env);

    let campaign = get_campaign(&env).unwrap();
    assert_eq!(campaign.concluded_at_ledger, Some(2000));
    assert_eq!(campaign.status, CampaignStatus::Cancelled);
}

#[test]
fn test_concluded_at_ledger_none_for_active() {
    let env = Env::default();
    env.ledger().with_mock(|ledger| {
        ledger.sequence = 500;
        ledger.timestamp = 1234567890;
    });

    setup_test_campaign(&env);

    let campaign = get_campaign(&env).unwrap();
    assert_eq!(campaign.concluded_at_ledger, None);
    assert_eq!(campaign.status, CampaignStatus::Active);
}
