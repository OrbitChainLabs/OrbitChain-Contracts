//! Issue #92 – Timelock + multi-sig admin governance.
//!
//! Privileged operations (`upgrade`, `freeze`, `unfreeze`, `extend_deadline`)
//! previously required only `creator.require_auth()` — a single point of
//! compromise: one stolen key could instantly replace the contract WASM.
//!
//! This module introduces the propose → approve → execute flow modelled on
//! Compound / OpenZeppelin's `TimelockController`:
//!
//! - Any admin signer may **propose** an [`AdminAction`] with an
//!   `execute_after` timestamp (the timelock, giving the community time to
//!   detect a malicious upgrade before it lands on-chain). Proposing counts
//!   as the proposer's approval.
//! - Other signers **approve** the action by id.
//! - Once the quorum is met **and** `execute_after` has passed, any signer
//!   may **execute**. The action is deleted before its effect is applied, so
//!   it can never be replayed.
//!
//! Quorum: `min(2, signer_count)`. With a multi-sig signer set (≥2 signers)
//! two approvals are required and the **direct** admin entrypoints are
//! disabled ([`Error::MultisigActive`]). With a single signer — the
//! backwards-compatible default of `[creator]` — the flow degrades to a
//! 1-of-1 quorum and the existing direct entrypoints keep working unchanged.

use soroban_sdk::{Address, Bytes, Env, Map, Vec};

use crate::event;
use crate::storage::{
    get_admin_action, get_admin_signers_storage, get_campaign, increment_admin_action_count,
    is_frozen, remove_admin_action, set_admin_action, set_admin_signers_storage, set_frozen,
};
use crate::types::{ActionKind, AdminAction, Error};

/// The current admin signer set: the stored set if configured, otherwise the
/// backwards-compatible default `[creator]`.
///
/// # Panics
/// - `Error::NotInitialized` if the campaign is not initialized
pub fn get_admin_signers(env: &Env) -> Vec<Address> {
    if let Some(signers) = get_admin_signers_storage(env) {
        return signers;
    }
    let campaign = get_campaign(env).unwrap_or_else(|| env.panic_with_error(Error::NotInitialized));
    let mut signers = Vec::new(env);
    signers.push_back(campaign.creator);
    signers
}

/// Approvals required to execute an action: 2 with a multi-sig set, 1 with
/// the single-signer (legacy) configuration.
fn required_approvals(signer_count: u32) -> u32 {
    if signer_count >= 2 {
        2
    } else {
        1
    }
}

/// Panic with `Error::NotAdminSigner` unless `who` is in the signer set.
fn assert_signer(env: &Env, signers: &Vec<Address>, who: &Address) {
    if !signers.contains(who) {
        env.panic_with_error(Error::NotAdminSigner);
    }
}

/// Guard for the legacy direct entrypoints (`upgrade`, `freeze`, `unfreeze`,
/// `extend_deadline`): allowed only while a single signer is configured
/// (1-of-1 quorum). Returns that signer so the caller can `require_auth()` it.
///
/// # Panics
/// - `Error::MultisigActive` if ≥2 signers are configured
pub fn require_direct_admin(env: &Env) -> Address {
    let signers = get_admin_signers(env);
    if signers.len() >= 2 {
        env.panic_with_error(Error::MultisigActive);
    }
    signers.get(0).unwrap()
}

/// Validate that `payload` matches the encoding its `kind` demands.
fn validate_payload(env: &Env, kind: &ActionKind, payload: &Bytes) {
    let valid = match kind {
        ActionKind::Upgrade => payload.len() == 32,
        ActionKind::Freeze | ActionKind::Unfreeze => payload.is_empty(),
        ActionKind::ExtendDeadline => payload.len() == 8,
    };
    if !valid {
        env.panic_with_error(Error::InvalidActionPayload);
    }
}

/// Propose an admin action. The proposer must be an admin signer and is
/// recorded as the first approval. Returns the new action's id.
///
/// # Panics
/// - `Error::NotAdminSigner` if `proposer` is not in the signer set
/// - `Error::InvalidActionPayload` if the payload doesn't match the kind
/// - `Error::InvalidExecuteAfter` if `execute_after` is in the past
pub fn propose_admin_action(
    env: &Env,
    proposer: Address,
    kind: ActionKind,
    payload: Bytes,
    execute_after: u64,
) -> u64 {
    proposer.require_auth();

    let signers = get_admin_signers(env);
    assert_signer(env, &signers, &proposer);
    validate_payload(env, &kind, &payload);

    if execute_after < env.ledger().timestamp() {
        env.panic_with_error(Error::InvalidExecuteAfter);
    }

    let mut voters: Map<Address, bool> = Map::new(env);
    voters.set(proposer.clone(), true);

    let action = AdminAction {
        kind,
        payload,
        execute_after,
        voters,
    };

    let action_id = increment_admin_action_count(env);
    set_admin_action(env, action_id, &action);

    event::admin_action_proposed(env, action_id, &proposer, execute_after);
    action_id
}

/// Approve a pending admin action. Each signer may approve once.
///
/// # Panics
/// - `Error::NotAdminSigner` if `approver` is not in the signer set
/// - `Error::ActionNotFound` if no action exists under `action_id`
/// - `Error::AlreadyApproved` if this signer already approved
pub fn approve_admin_action(env: &Env, approver: Address, action_id: u64) {
    approver.require_auth();

    let signers = get_admin_signers(env);
    assert_signer(env, &signers, &approver);

    let mut action = get_admin_action(env, action_id)
        .unwrap_or_else(|| env.panic_with_error(Error::ActionNotFound));

    if action.voters.contains_key(approver.clone()) {
        env.panic_with_error(Error::AlreadyApproved);
    }

    action.voters.set(approver.clone(), true);
    set_admin_action(env, action_id, &action);

    event::admin_action_approved(env, action_id, &approver, action.voters.len());
}

/// Execute an approved admin action once its timelock has elapsed.
///
/// The action is **deleted before** its effect is applied, so execution can
/// never replay and a re-entrant call finds no action.
///
/// # Panics
/// - `Error::NotAdminSigner` if `executor` is not in the signer set
/// - `Error::ActionNotFound` if no action exists under `action_id`
/// - `Error::TimelockNotElapsed` if `execute_after` has not passed
/// - `Error::InsufficientApprovals` if approvals < quorum
/// - `Error::ContractFrozen` for an `Upgrade` or `ExtendDeadline` action
///   while the contract is frozen (matching the direct entrypoints)
pub fn execute_admin_action(env: &Env, executor: Address, action_id: u64) {
    executor.require_auth();

    let signers = get_admin_signers(env);
    assert_signer(env, &signers, &executor);

    let action = get_admin_action(env, action_id)
        .unwrap_or_else(|| env.panic_with_error(Error::ActionNotFound));

    if env.ledger().timestamp() < action.execute_after {
        env.panic_with_error(Error::TimelockNotElapsed);
    }

    if action.voters.len() < required_approvals(signers.len()) {
        env.panic_with_error(Error::InsufficientApprovals);
    }

    // Delete before applying: no replay, and a re-entrant call finds nothing.
    remove_admin_action(env, action_id);

    let timestamp = env.ledger().timestamp();
    match action.kind {
        ActionKind::Upgrade => {
            // Freeze invariant matches the direct upgrade() entrypoint.
            if is_frozen(env) {
                env.panic_with_error(Error::ContractFrozen);
            }
            let mut hash = [0u8; 32];
            action.payload.copy_into_slice(&mut hash);
            let wasm_hash = soroban_sdk::BytesN::from_array(env, &hash);
            env.deployer()
                .update_current_contract_wasm(wasm_hash.clone());
            event::contract_upgraded(env, &executor, wasm_hash, timestamp);
        }
        ActionKind::Freeze => {
            set_frozen(env, true);
            event::contract_frozen(env, &executor, timestamp);
        }
        ActionKind::Unfreeze => {
            set_frozen(env, false);
            event::contract_unfrozen(env, &executor, timestamp);
        }
        ActionKind::ExtendDeadline => {
            let mut buf = [0u8; 8];
            action.payload.copy_into_slice(&mut buf);
            let new_end_time = u64::from_be_bytes(buf);
            // Shares the direct entrypoint's validation (status, bounds,
            // freeze check) without re-authing the creator.
            crate::contract::apply_extend_deadline(env, new_end_time, &executor);
        }
    }

    event::admin_action_executed(env, action_id, &executor);
}

/// Replace the admin signer set.
///
/// Requires authorization from **every current signer** — rotating a
/// multi-sig set is itself a multi-sig operation, so a single compromised
/// key can neither expand nor collapse the quorum. With the default
/// single-signer set this degrades to the familiar creator-only auth.
///
/// # Panics
/// - `Error::InvalidSigners` if `new_signers` is empty or has duplicates
pub fn set_admin_signers(env: &Env, new_signers: Vec<Address>) {
    if new_signers.is_empty() {
        env.panic_with_error(Error::InvalidSigners);
    }
    // Reject duplicates: each signer must count once toward the quorum.
    for i in 0..new_signers.len() {
        let a = new_signers.get(i).unwrap();
        for j in (i + 1)..new_signers.len() {
            if a == new_signers.get(j).unwrap() {
                env.panic_with_error(Error::InvalidSigners);
            }
        }
    }

    let current = get_admin_signers(env);
    for signer in current.iter() {
        signer.require_auth();
    }

    set_admin_signers_storage(env, &new_signers);
    event::admin_signers_updated(env, new_signers.len());
}
