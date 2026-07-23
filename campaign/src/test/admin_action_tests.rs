//! Issue #92 – Tests for the timelock + multi-sig admin governance flow.
//!
//! Covers the full acceptance matrix: quorum enforcement (an action cannot
//! execute with <2 approvals under a multi-sig set), timelock enforcement
//! (cannot execute before `execute_after`), replay protection, payload
//! validation, and 1-of-1 backwards compatibility of the direct entrypoints.

#![cfg(test)]

use soroban_sdk::testutils::{Address as AddressTestUtils, Ledger};
use soroban_sdk::{vec, Address, Bytes, Env, Vec};

use crate::storage::{is_frozen, set_campaign};
use crate::types::{ActionKind, CampaignData, CampaignStatus, StellarAsset};
use crate::CampaignContract;

/// Base ledger timestamp; large enough to add deadlines on top of.
const BASE: u64 = 86400 * 365;

/// One-hour timelock used by most tests.
const DELAY: u64 = 3600;

fn make_env() -> Env {
    let env = Env::default();
    env.ledger().set_timestamp(BASE);
    env.mock_all_auths();
    env
}

/// Register the contract and store an Active campaign; returns
/// `(contract_id, creator)`. Every subsequent contract invocation should use
/// its own `as_contract` frame (same-address re-auth inside one frame trips
/// "frame is already authorized").
fn setup(env: &Env) -> (Address, Address) {
    let contract_id = env.register_contract(None, CampaignContract);
    let creator = Address::generate(env);
    let campaign = CampaignData {
        creator: creator.clone(),
        goal_amount: 1000,
        raised_amount: 0,
        end_time: BASE + 30 * 86400,
        status: CampaignStatus::Active,
        accepted_assets: vec![
            env,
            StellarAsset {
                asset_code: soroban_sdk::String::from_str(env, "TST"),
                issuer: Some(Address::generate(env)),
            },
        ],
        milestone_count: 0,
        min_donation_amount: 0,
        created_at_ledger: 0,
        created_at_time: 0,
        concluded_at_ledger: None,
    };
    env.as_contract(&contract_id, || set_campaign(env, &campaign));
    (contract_id, creator)
}

/// Configure a 2-signer multi-sig set (creator + one more); returns the
/// second signer.
fn setup_multisig(env: &Env, contract_id: &Address, creator: &Address) -> Address {
    let second = Address::generate(env);
    let signers: Vec<Address> = vec![env, creator.clone(), second.clone()];
    env.as_contract(contract_id, || {
        CampaignContract::set_admin_signers(env.clone(), signers)
    });
    second
}

fn empty_payload(env: &Env) -> Bytes {
    Bytes::new(env)
}

// ─── Signer set & backwards compatibility ────────────────────────────────────

/// Without a stored signer set the admin set defaults to `[creator]`.
#[test]
fn test_default_signers_is_creator_1_of_1() {
    let env = make_env();
    let (contract_id, creator) = setup(&env);
    let signers = env.as_contract(&contract_id, || {
        CampaignContract::get_admin_signers(env.clone())
    });
    assert_eq!(signers.len(), 1);
    assert_eq!(signers.get(0).unwrap(), creator);
}

/// Backwards compatibility: with the default single signer, the direct
/// `freeze()` / `unfreeze()` entrypoints keep working exactly as before.
#[test]
fn test_direct_freeze_still_works_with_single_signer() {
    let env = make_env();
    let (contract_id, _creator) = setup(&env);

    env.as_contract(&contract_id, || CampaignContract::freeze(env.clone()));
    let frozen = env.as_contract(&contract_id, || is_frozen(&env));
    assert!(frozen);

    env.as_contract(&contract_id, || CampaignContract::unfreeze(env.clone()));
    let frozen = env.as_contract(&contract_id, || is_frozen(&env));
    assert!(!frozen);
}

/// Once a multi-sig signer set is configured, direct admin calls are blocked
/// with `Error::MultisigActive` — the flow is the only path.
#[test]
#[should_panic(expected = "Error(Contract, #100)")]
fn test_direct_freeze_blocked_when_multisig_active() {
    let env = make_env();
    let (contract_id, creator) = setup(&env);
    setup_multisig(&env, &contract_id, &creator);

    env.as_contract(&contract_id, || CampaignContract::freeze(env.clone()));
}

/// The signer set rejects duplicates.
#[test]
#[should_panic(expected = "Error(Contract, #102)")]
fn test_set_admin_signers_rejects_duplicates() {
    let env = make_env();
    let (contract_id, creator) = setup(&env);
    let signers: Vec<Address> = vec![&env, creator.clone(), creator.clone()];
    env.as_contract(&contract_id, || {
        CampaignContract::set_admin_signers(env.clone(), signers)
    });
}

/// The signer set rejects an empty list.
#[test]
#[should_panic(expected = "Error(Contract, #102)")]
fn test_set_admin_signers_rejects_empty() {
    let env = make_env();
    let (contract_id, _creator) = setup(&env);
    env.as_contract(&contract_id, || {
        CampaignContract::set_admin_signers(env.clone(), Vec::new(&env))
    });
}

// ─── Propose ─────────────────────────────────────────────────────────────────

/// A non-signer cannot propose an action.
#[test]
#[should_panic(expected = "Error(Contract, #94)")]
fn test_propose_by_non_signer_rejected() {
    let env = make_env();
    let (contract_id, _creator) = setup(&env);
    let outsider = Address::generate(&env);
    env.as_contract(&contract_id, || {
        CampaignContract::propose_admin_action(
            env.clone(),
            outsider.clone(),
            ActionKind::Freeze,
            empty_payload(&env),
            BASE + DELAY,
        )
    });
}

/// An `Upgrade` proposal whose payload is not exactly 32 bytes is rejected.
#[test]
#[should_panic(expected = "Error(Contract, #99)")]
fn test_propose_upgrade_with_bad_payload_rejected() {
    let env = make_env();
    let (contract_id, creator) = setup(&env);
    env.as_contract(&contract_id, || {
        CampaignContract::propose_admin_action(
            env.clone(),
            creator.clone(),
            ActionKind::Upgrade,
            Bytes::from_array(&env, &[1, 2, 3]),
            BASE + DELAY,
        )
    });
}

/// An `execute_after` in the past is rejected at proposal time.
#[test]
#[should_panic(expected = "Error(Contract, #101)")]
fn test_propose_with_past_execute_after_rejected() {
    let env = make_env();
    let (contract_id, creator) = setup(&env);
    env.as_contract(&contract_id, || {
        CampaignContract::propose_admin_action(
            env.clone(),
            creator.clone(),
            ActionKind::Freeze,
            empty_payload(&env),
            BASE - 1,
        )
    });
}

/// Proposing stores the action, counts as the proposer's approval, and
/// increments the action counter.
#[test]
fn test_propose_records_action_and_first_approval() {
    let env = make_env();
    let (contract_id, creator) = setup(&env);

    let id = env.as_contract(&contract_id, || {
        CampaignContract::propose_admin_action(
            env.clone(),
            creator.clone(),
            ActionKind::Freeze,
            empty_payload(&env),
            BASE + DELAY,
        )
    });
    assert_eq!(id, 0);

    let action = env.as_contract(&contract_id, || {
        CampaignContract::get_admin_action(env.clone(), id)
    });
    assert_eq!(action.kind, ActionKind::Freeze);
    assert_eq!(action.execute_after, BASE + DELAY);
    assert_eq!(action.voters.len(), 1);
    assert!(action.voters.contains_key(creator.clone()));

    let count = env.as_contract(&contract_id, || {
        CampaignContract::get_admin_action_count(env.clone())
    });
    assert_eq!(count, 1);
}

// ─── Approve ─────────────────────────────────────────────────────────────────

/// A signer cannot approve the same action twice (proposing counts as the
/// proposer's approval).
#[test]
#[should_panic(expected = "Error(Contract, #98)")]
fn test_double_approval_rejected() {
    let env = make_env();
    let (contract_id, creator) = setup(&env);
    setup_multisig(&env, &contract_id, &creator);

    let id = env.as_contract(&contract_id, || {
        CampaignContract::propose_admin_action(
            env.clone(),
            creator.clone(),
            ActionKind::Freeze,
            empty_payload(&env),
            BASE + DELAY,
        )
    });

    env.as_contract(&contract_id, || {
        CampaignContract::approve_admin_action(env.clone(), creator.clone(), id)
    });
}

/// Approving a non-existent action panics with `ActionNotFound`.
#[test]
#[should_panic(expected = "Error(Contract, #95)")]
fn test_approve_unknown_action_rejected() {
    let env = make_env();
    let (contract_id, creator) = setup(&env);
    env.as_contract(&contract_id, || {
        CampaignContract::approve_admin_action(env.clone(), creator.clone(), 7)
    });
}

// ─── Execute: quorum & timelock (acceptance criteria) ────────────────────────

/// Acceptance: with a 2-signer set, an action with only the proposer's
/// approval cannot execute — `InsufficientApprovals`.
#[test]
#[should_panic(expected = "Error(Contract, #97)")]
fn test_execute_with_one_of_two_approvals_rejected() {
    let env = make_env();
    let (contract_id, creator) = setup(&env);
    setup_multisig(&env, &contract_id, &creator);

    let id = env.as_contract(&contract_id, || {
        CampaignContract::propose_admin_action(
            env.clone(),
            creator.clone(),
            ActionKind::Freeze,
            empty_payload(&env),
            BASE + DELAY,
        )
    });

    // Timelock elapsed, but quorum (2) not met: only the proposer approved.
    env.ledger().set_timestamp(BASE + DELAY + 1);
    env.as_contract(&contract_id, || {
        CampaignContract::execute_admin_action(env.clone(), creator.clone(), id)
    });
}

/// Acceptance: a fully-approved action cannot execute before `execute_after`
/// — `TimelockNotElapsed`.
#[test]
#[should_panic(expected = "Error(Contract, #96)")]
fn test_execute_before_timelock_rejected() {
    let env = make_env();
    let (contract_id, creator) = setup(&env);
    let second = setup_multisig(&env, &contract_id, &creator);

    let id = env.as_contract(&contract_id, || {
        CampaignContract::propose_admin_action(
            env.clone(),
            creator.clone(),
            ActionKind::Freeze,
            empty_payload(&env),
            BASE + DELAY,
        )
    });
    env.as_contract(&contract_id, || {
        CampaignContract::approve_admin_action(env.clone(), second.clone(), id)
    });

    // Quorum met, but the timelock has not elapsed (now == BASE < BASE+DELAY).
    env.as_contract(&contract_id, || {
        CampaignContract::execute_admin_action(env.clone(), creator.clone(), id)
    });
}

/// Happy path: propose freeze → second approval → warp past the timelock →
/// execute. The contract freezes; the flow then unfreezes it the same way.
#[test]
fn test_full_flow_freeze_then_unfreeze() {
    let env = make_env();
    let (contract_id, creator) = setup(&env);
    let second = setup_multisig(&env, &contract_id, &creator);

    // Freeze via the flow.
    let id = env.as_contract(&contract_id, || {
        CampaignContract::propose_admin_action(
            env.clone(),
            creator.clone(),
            ActionKind::Freeze,
            empty_payload(&env),
            BASE + DELAY,
        )
    });
    env.as_contract(&contract_id, || {
        CampaignContract::approve_admin_action(env.clone(), second.clone(), id)
    });
    env.ledger().set_timestamp(BASE + DELAY + 1);
    env.as_contract(&contract_id, || {
        CampaignContract::execute_admin_action(env.clone(), second.clone(), id)
    });
    assert!(env.as_contract(&contract_id, || is_frozen(&env)));

    // Unfreeze via the flow.
    let id2 = env.as_contract(&contract_id, || {
        CampaignContract::propose_admin_action(
            env.clone(),
            creator.clone(),
            ActionKind::Unfreeze,
            empty_payload(&env),
            BASE + 2 * DELAY,
        )
    });
    env.as_contract(&contract_id, || {
        CampaignContract::approve_admin_action(env.clone(), second.clone(), id2)
    });
    env.ledger().set_timestamp(BASE + 2 * DELAY + 1);
    env.as_contract(&contract_id, || {
        CampaignContract::execute_admin_action(env.clone(), creator.clone(), id2)
    });
    assert!(!env.as_contract(&contract_id, || is_frozen(&env)));
}

/// Replay protection: an executed action is deleted, so executing it again
/// panics with `ActionNotFound`.
#[test]
#[should_panic(expected = "Error(Contract, #95)")]
fn test_executed_action_cannot_replay() {
    let env = make_env();
    let (contract_id, creator) = setup(&env);
    let second = setup_multisig(&env, &contract_id, &creator);

    let id = env.as_contract(&contract_id, || {
        CampaignContract::propose_admin_action(
            env.clone(),
            creator.clone(),
            ActionKind::Freeze,
            empty_payload(&env),
            BASE + DELAY,
        )
    });
    env.as_contract(&contract_id, || {
        CampaignContract::approve_admin_action(env.clone(), second.clone(), id)
    });
    env.ledger().set_timestamp(BASE + DELAY + 1);
    env.as_contract(&contract_id, || {
        CampaignContract::execute_admin_action(env.clone(), creator.clone(), id)
    });

    // Second execution must find nothing.
    env.as_contract(&contract_id, || {
        CampaignContract::execute_admin_action(env.clone(), second.clone(), id)
    });
}

/// 1-of-1 backwards compatibility inside the flow itself: with the default
/// single signer, the proposer's implicit approval alone meets the quorum.
#[test]
fn test_flow_works_with_single_signer_quorum_of_one() {
    let env = make_env();
    let (contract_id, creator) = setup(&env);

    let id = env.as_contract(&contract_id, || {
        CampaignContract::propose_admin_action(
            env.clone(),
            creator.clone(),
            ActionKind::Freeze,
            empty_payload(&env),
            BASE + DELAY,
        )
    });
    env.ledger().set_timestamp(BASE + DELAY + 1);
    env.as_contract(&contract_id, || {
        CampaignContract::execute_admin_action(env.clone(), creator.clone(), id)
    });
    assert!(env.as_contract(&contract_id, || is_frozen(&env)));
}

/// ExtendDeadline flows end-to-end: the payload's big-endian `u64` becomes
/// the campaign's new `end_time` after quorum + timelock.
#[test]
fn test_extend_deadline_via_flow() {
    let env = make_env();
    let (contract_id, creator) = setup(&env);
    let second = setup_multisig(&env, &contract_id, &creator);

    let new_end = BASE + 60 * 86400;
    let payload = Bytes::from_array(&env, &new_end.to_be_bytes());

    let id = env.as_contract(&contract_id, || {
        CampaignContract::propose_admin_action(
            env.clone(),
            creator.clone(),
            ActionKind::ExtendDeadline,
            payload,
            BASE + DELAY,
        )
    });
    env.as_contract(&contract_id, || {
        CampaignContract::approve_admin_action(env.clone(), second.clone(), id)
    });
    env.ledger().set_timestamp(BASE + DELAY + 1);
    env.as_contract(&contract_id, || {
        CampaignContract::execute_admin_action(env.clone(), second.clone(), id)
    });

    let campaign = env.as_contract(&contract_id, || crate::storage::get_campaign(&env).unwrap());
    assert_eq!(campaign.end_time, new_end);
}

/// A non-signer cannot execute even a fully-approved, matured action.
#[test]
#[should_panic(expected = "Error(Contract, #94)")]
fn test_execute_by_non_signer_rejected() {
    let env = make_env();
    let (contract_id, creator) = setup(&env);
    let second = setup_multisig(&env, &contract_id, &creator);

    let id = env.as_contract(&contract_id, || {
        CampaignContract::propose_admin_action(
            env.clone(),
            creator.clone(),
            ActionKind::Freeze,
            empty_payload(&env),
            BASE + DELAY,
        )
    });
    env.as_contract(&contract_id, || {
        CampaignContract::approve_admin_action(env.clone(), second.clone(), id)
    });
    env.ledger().set_timestamp(BASE + DELAY + 1);

    let outsider = Address::generate(&env);
    env.as_contract(&contract_id, || {
        CampaignContract::execute_admin_action(env.clone(), outsider.clone(), id)
    });
}
