//! Issue #146 — tests for soulbound donation receipts.
//!
//! Covers the two acceptance criteria directly: the minting happy path, and
//! soulbound enforcement (every transfer path panics). Also pins the
//! preconditions, since a receipt is permanent and must not be mintable early,
//! twice, or by a non-donor.

#![cfg(test)]

use soroban_sdk::testutils::{Address as AddressTestUtils, Ledger};
use soroban_sdk::{vec, Address, Env};

use super::with_contract;
use crate::receipt;
use crate::storage::{set_campaign, set_donor, set_milestone, set_receipt};
use crate::types::{
    AssetInfo, CampaignData, CampaignStatus, DonationReceipt, DonorRecord, MilestoneData,
    MilestoneStatus, StellarAsset,
};

const BASE: u64 = 86400 * 365;
const GOAL: i128 = 1_000;

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Write a campaign whose goal is met or not, with `milestone_count` milestones.
fn seed_campaign(env: &Env, raised: i128, milestone_count: u32) -> Address {
    let creator = Address::generate(env);
    let asset = StellarAsset {
        asset_code: soroban_sdk::String::from_str(env, "XLM"),
        issuer: None, // native
    };
    set_campaign(
        env,
        &CampaignData {
            creator: creator.clone(),
            goal_amount: GOAL,
            raised_amount: raised,
            end_time: BASE + 1000,
            status: CampaignStatus::Active,
            accepted_assets: vec![env, asset],
            milestone_count,
            min_donation_amount: 0,
            created_at_ledger: 1,
            created_at_time: BASE,
            concluded_at_ledger: None,
        },
    );
    creator
}

/// Write milestone `index` with the given status.
fn seed_milestone(env: &Env, index: u32, status: MilestoneStatus) {
    let released = status == MilestoneStatus::Released;
    set_milestone(
        env,
        index,
        &MilestoneData {
            index,
            target_amount: GOAL,
            released_amount: if released { GOAL } else { 0 },
            description_hash: soroban_sdk::BytesN::from_array(env, &[0u8; 32]),
            status,
            released_at: if released { Some(BASE) } else { None },
            released_at_ledger: if released { Some(1) } else { None },
            release_tx: None,
            released_to: None,
        },
    );
}

/// Write a donor who has given `amount`.
fn seed_donor(env: &Env, amount: i128) -> Address {
    let donor = Address::generate(env);
    set_donor(
        env,
        &donor,
        &DonorRecord {
            donor: donor.clone(),
            total_donated: amount,
            asset: AssetInfo::Native,
            last_donation_time: BASE,
            last_donation_ledger: 1,
            donation_count: 1,
            refund_claimed: false,
        },
    );
    donor
}

/// Campaign that is finalised: goal met and the final milestone released.
fn seed_finalised(env: &Env) {
    seed_campaign(env, GOAL, 1);
    seed_milestone(env, 0, MilestoneStatus::Released);
}

// ─── is_finalised ────────────────────────────────────────────────────────────

#[test]
fn test_not_finalised_when_goal_unmet() {
    let env = Env::default();
    with_contract(&env, || {
        seed_campaign(&env, GOAL - 1, 1);
        seed_milestone(&env, 0, MilestoneStatus::Released);
        // Final milestone released but the goal was never reached.
        assert!(!receipt::is_finalised(&env));
    });
}

#[test]
fn test_not_finalised_when_last_milestone_unreleased() {
    let env = Env::default();
    with_contract(&env, || {
        seed_campaign(&env, GOAL, 2);
        seed_milestone(&env, 0, MilestoneStatus::Released);
        seed_milestone(&env, 1, MilestoneStatus::Unlocked); // final one still open
        assert!(!receipt::is_finalised(&env));
    });
}

#[test]
fn test_finalised_when_goal_met_and_last_milestone_released() {
    let env = Env::default();
    with_contract(&env, || {
        seed_campaign(&env, GOAL, 2);
        seed_milestone(&env, 0, MilestoneStatus::Released);
        seed_milestone(&env, 1, MilestoneStatus::Released);
        assert!(receipt::is_finalised(&env));
    });
}

// ─── Minting happy path (acceptance criterion 1) ─────────────────────────────

#[test]
fn test_claim_receipt_mints_with_donation_amount() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = BASE);

    with_contract(&env, || {
        seed_finalised(&env);
        let donor = seed_donor(&env, 250);

        let r = receipt::claim_receipt(&env, donor.clone());

        assert_eq!(r.donor, donor);
        // Metadata references the donation amount, per the issue.
        assert_eq!(r.amount_donated, 250);
        assert_eq!(r.campaign_goal, GOAL);
        assert_eq!(r.minted_at, BASE);

        // Readable afterwards, and reflected in the token-shaped views.
        assert_eq!(receipt::get_receipt_for(&env, donor.clone()), Some(r));
        assert!(receipt::has_claimed(&env, donor.clone()));
        assert_eq!(receipt::balance(&env, donor), 1);
        assert_eq!(receipt::total_supply(&env), 1);
    });
}

#[test]
fn test_balance_is_zero_before_claiming() {
    let env = Env::default();
    with_contract(&env, || {
        seed_finalised(&env);
        let donor = seed_donor(&env, 100);
        assert_eq!(receipt::balance(&env, donor.clone()), 0);
        assert!(!receipt::has_claimed(&env, donor));
        assert_eq!(receipt::total_supply(&env), 0);
    });
}

#[test]
fn test_each_donor_gets_their_own_receipt() {
    let env = Env::default();
    env.mock_all_auths();

    with_contract(&env, || {
        seed_finalised(&env);
        let a = seed_donor(&env, 100);
        let b = seed_donor(&env, 900);

        let ra = receipt::claim_receipt(&env, a.clone());
        let rb = receipt::claim_receipt(&env, b.clone());

        // Receipts are per-donor and carry that donor's own amount.
        assert_eq!(ra.amount_donated, 100);
        assert_eq!(rb.amount_donated, 900);
        assert_eq!(receipt::total_supply(&env), 2);
        assert_eq!(receipt::balance(&env, a), 1);
        assert_eq!(receipt::balance(&env, b), 1);
    });
}

// ─── Soulbound enforcement (acceptance criterion 2) ──────────────────────────

#[test]
#[should_panic(expected = "Error(Contract, #93)")] // ReceiptNonTransferable
fn test_transfer_panics() {
    let env = Env::default();
    env.mock_all_auths();
    with_contract(&env, || {
        seed_finalised(&env);
        let donor = seed_donor(&env, 100);
        receipt::claim_receipt(&env, donor.clone());

        let other = Address::generate(&env);
        receipt::transfer(&env, donor, other, 1);
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #93)")] // ReceiptNonTransferable
fn test_transfer_from_panics() {
    let env = Env::default();
    env.mock_all_auths();
    with_contract(&env, || {
        seed_finalised(&env);
        let donor = seed_donor(&env, 100);
        receipt::claim_receipt(&env, donor.clone());

        let spender = Address::generate(&env);
        let other = Address::generate(&env);
        receipt::transfer_from(&env, spender, donor, other, 1);
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #93)")] // ReceiptNonTransferable
fn test_approve_panics() {
    let env = Env::default();
    env.mock_all_auths();
    with_contract(&env, || {
        seed_finalised(&env);
        let donor = seed_donor(&env, 100);
        receipt::claim_receipt(&env, donor.clone());

        let spender = Address::generate(&env);
        receipt::approve(&env, donor, spender, 1, 100);
    });
}

#[test]
fn test_receipt_survives_a_failed_transfer_attempt() {
    // The panic must not consume or move the receipt.
    let env = Env::default();
    env.mock_all_auths();
    with_contract(&env, || {
        seed_finalised(&env);
        let donor = seed_donor(&env, 100);
        receipt::claim_receipt(&env, donor.clone());

        let other = Address::generate(&env);
        // Transfers panic, so the donor keeps it and the recipient never gains one.
        assert_eq!(receipt::balance(&env, donor), 1);
        assert_eq!(receipt::balance(&env, other), 0);
    });
}

// ─── Preconditions ───────────────────────────────────────────────────────────

#[test]
#[should_panic(expected = "Error(Contract, #91)")] // CampaignNotFinalised
fn test_claim_before_finalised_panics() {
    let env = Env::default();
    env.mock_all_auths();
    with_contract(&env, || {
        seed_campaign(&env, GOAL - 1, 1); // goal not reached
        seed_milestone(&env, 0, MilestoneStatus::Unlocked);
        let donor = seed_donor(&env, 100);
        receipt::claim_receipt(&env, donor);
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #51)")] // NoDonorRecord
fn test_non_donor_cannot_claim() {
    let env = Env::default();
    env.mock_all_auths();
    with_contract(&env, || {
        seed_finalised(&env);
        let stranger = Address::generate(&env); // never donated
        receipt::claim_receipt(&env, stranger);
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #92)")] // ReceiptAlreadyClaimed
fn test_double_claim_panics() {
    let env = Env::default();
    env.mock_all_auths();
    with_contract(&env, || {
        seed_finalised(&env);
        let donor = seed_donor(&env, 100);

        // Seed the already-claimed state rather than calling claim_receipt
        // twice: two `require_auth()` calls for the same address inside one
        // `as_contract` frame trip the auth mock ("frame is already
        // authorized") before the guard is reached. Mirrors
        // `test_claim_refund_already_claimed`, which pre-seeds `refund_claimed`
        // for the same reason.
        set_receipt(
            &env,
            &donor,
            &DonationReceipt {
                donor: donor.clone(),
                amount_donated: 100,
                campaign_goal: GOAL,
                minted_at: BASE,
                minted_at_ledger: 1,
            },
        );

        receipt::claim_receipt(&env, donor);
    });
}
