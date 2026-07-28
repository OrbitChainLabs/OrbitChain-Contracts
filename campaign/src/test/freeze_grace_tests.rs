//! Issue #95 – Tests for the freeze / un-freeze grace window.
//!
//! `freeze()` and `unfreeze()` used to be immediate toggles, so a compromised
//! admin key could undo an emergency freeze in the very next transaction.
//! `unfreeze()` now requires the freeze to have stood for at least the
//! configured delay. These tests pin the boundary exactly, cover the
//! configurable delay, and confirm the pre-existing freeze behaviour is intact.

#![cfg(test)]

use soroban_sdk::testutils::{Address as AddressTestUtils, Ledger};
use soroban_sdk::{vec, Address, Env};

use crate::storage::{is_frozen, set_campaign};
use crate::types::{CampaignData, CampaignStatus, StellarAsset};
use crate::{CampaignContract, DEFAULT_MIN_UNFREEZE_DELAY, MAX_UNFREEZE_DELAY};

/// Base ledger timestamp, well clear of zero so the window can be measured.
const BASE: u64 = 86400 * 365;

fn make_env() -> Env {
    let env = Env::default();
    env.ledger().set_timestamp(BASE);
    env.mock_all_auths();
    env
}

/// Register the contract with an Active campaign. Each contract invocation
/// gets its own `as_contract` frame — re-authing the same address twice inside
/// one frame trips "frame is already authorized".
fn setup(env: &Env) -> Address {
    let contract_id = env.register_contract(None, CampaignContract);
    let campaign = CampaignData {
        creator: Address::generate(env),
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
    contract_id
}

// ─── Acceptance criterion: unfreeze inside the window panics ─────────────────

/// Acceptance: `unfreeze()` inside the grace window panics `UnfreezeTooEarly`.
#[test]
#[should_panic(expected = "Error(Contract, #103)")]
fn test_unfreeze_within_grace_window_rejected() {
    let env = make_env();
    let contract_id = setup(&env);

    env.as_contract(&contract_id, || CampaignContract::freeze(env.clone()));

    // Immediately afterwards — the attacker's race.
    env.as_contract(&contract_id, || CampaignContract::unfreeze(env.clone()));
}

/// One second before the window closes is still too early.
#[test]
#[should_panic(expected = "Error(Contract, #103)")]
fn test_unfreeze_one_second_before_window_rejected() {
    let env = make_env();
    let contract_id = setup(&env);

    env.as_contract(&contract_id, || CampaignContract::freeze(env.clone()));
    env.ledger()
        .set_timestamp(BASE + DEFAULT_MIN_UNFREEZE_DELAY - 1);
    env.as_contract(&contract_id, || CampaignContract::unfreeze(env.clone()));
}

/// Exactly at the boundary the un-freeze is permitted (`>=`, not `>`).
#[test]
fn test_unfreeze_exactly_at_window_boundary_succeeds() {
    let env = make_env();
    let contract_id = setup(&env);

    env.as_contract(&contract_id, || CampaignContract::freeze(env.clone()));
    assert!(env.as_contract(&contract_id, || is_frozen(&env)));

    env.ledger()
        .set_timestamp(BASE + DEFAULT_MIN_UNFREEZE_DELAY);
    env.as_contract(&contract_id, || CampaignContract::unfreeze(env.clone()));
    assert!(!env.as_contract(&contract_id, || is_frozen(&env)));
}

/// Well past the window, un-freeze behaves exactly as it always did.
#[test]
fn test_unfreeze_after_window_succeeds() {
    let env = make_env();
    let contract_id = setup(&env);

    env.as_contract(&contract_id, || CampaignContract::freeze(env.clone()));
    env.ledger()
        .set_timestamp(BASE + DEFAULT_MIN_UNFREEZE_DELAY + 5000);
    env.as_contract(&contract_id, || CampaignContract::unfreeze(env.clone()));
    assert!(!env.as_contract(&contract_id, || is_frozen(&env)));
}

// ─── Timestamp bookkeeping ───────────────────────────────────────────────────

/// `freeze()` stamps `FrozenAt` with the current ledger time.
#[test]
fn test_freeze_stamps_frozen_at() {
    let env = make_env();
    let contract_id = setup(&env);

    let before = env.as_contract(&contract_id, || {
        CampaignContract::get_frozen_at(env.clone())
    });
    assert_eq!(before, 0, "never frozen → 0");

    env.as_contract(&contract_id, || CampaignContract::freeze(env.clone()));
    let after = env.as_contract(&contract_id, || {
        CampaignContract::get_frozen_at(env.clone())
    });
    assert_eq!(after, BASE);
}

/// A re-freeze restarts the window: the stamp moves to the later freeze, so
/// the delay is measured from the most recent freeze-state change.
#[test]
#[should_panic(expected = "Error(Contract, #103)")]
fn test_refreeze_restarts_the_window() {
    let env = make_env();
    let contract_id = setup(&env);

    env.as_contract(&contract_id, || CampaignContract::freeze(env.clone()));
    env.ledger()
        .set_timestamp(BASE + DEFAULT_MIN_UNFREEZE_DELAY);
    env.as_contract(&contract_id, || CampaignContract::unfreeze(env.clone()));

    // Freeze again much later; the window restarts from here.
    let second_freeze = BASE + DEFAULT_MIN_UNFREEZE_DELAY + 10_000;
    env.ledger().set_timestamp(second_freeze);
    env.as_contract(&contract_id, || CampaignContract::freeze(env.clone()));

    // Enough time has passed since the FIRST freeze, but not the second.
    env.ledger().set_timestamp(second_freeze + 10);
    env.as_contract(&contract_id, || CampaignContract::unfreeze(env.clone()));
}

// ─── Configurable delay ──────────────────────────────────────────────────────

/// The delay defaults to `DEFAULT_MIN_UNFREEZE_DELAY` when never configured.
#[test]
fn test_delay_defaults_when_unset() {
    let env = make_env();
    let contract_id = setup(&env);
    let d = env.as_contract(&contract_id, || {
        CampaignContract::get_unfreeze_delay(env.clone())
    });
    assert_eq!(d, DEFAULT_MIN_UNFREEZE_DELAY);
}

/// A configured delay replaces the default and is what `unfreeze()` enforces.
#[test]
fn test_configured_delay_is_enforced() {
    let env = make_env();
    let contract_id = setup(&env);

    let custom = 7200u64; // two hours, longer than the one-hour default
    env.as_contract(&contract_id, || {
        CampaignContract::set_unfreeze_delay(env.clone(), custom)
    });
    let d = env.as_contract(&contract_id, || {
        CampaignContract::get_unfreeze_delay(env.clone())
    });
    assert_eq!(d, custom);

    env.as_contract(&contract_id, || CampaignContract::freeze(env.clone()));

    // At the configured boundary the un-freeze goes through.
    env.ledger().set_timestamp(BASE + custom);
    env.as_contract(&contract_id, || CampaignContract::unfreeze(env.clone()));
    assert!(!env.as_contract(&contract_id, || is_frozen(&env)));
}

/// The configured delay genuinely replaces the default: a window longer than
/// `DEFAULT_MIN_UNFREEZE_DELAY` still blocks once the default has elapsed.
/// Without this, a `set_unfreeze_delay` that silently failed to take effect
/// would go unnoticed.
#[test]
#[should_panic(expected = "Error(Contract, #103)")]
fn test_longer_configured_delay_blocks_past_default_window() {
    let env = make_env();
    let contract_id = setup(&env);

    let custom = DEFAULT_MIN_UNFREEZE_DELAY * 2;
    env.as_contract(&contract_id, || {
        CampaignContract::set_unfreeze_delay(env.clone(), custom)
    });
    env.as_contract(&contract_id, || CampaignContract::freeze(env.clone()));

    // The default window has passed, the configured one has not.
    env.ledger()
        .set_timestamp(BASE + DEFAULT_MIN_UNFREEZE_DELAY + 1);
    env.as_contract(&contract_id, || CampaignContract::unfreeze(env.clone()));
}

/// A zero delay restores the old immediate-toggle behaviour, for deployments
/// that do not want the window.
#[test]
fn test_zero_delay_allows_immediate_unfreeze() {
    let env = make_env();
    let contract_id = setup(&env);

    env.as_contract(&contract_id, || {
        CampaignContract::set_unfreeze_delay(env.clone(), 0)
    });
    env.as_contract(&contract_id, || CampaignContract::freeze(env.clone()));
    env.as_contract(&contract_id, || CampaignContract::unfreeze(env.clone()));
    assert!(!env.as_contract(&contract_id, || is_frozen(&env)));
}

/// A delay beyond `MAX_UNFREEZE_DELAY` is rejected, so the contract cannot be
/// bricked in the frozen state by a mis-set parameter.
#[test]
#[should_panic(expected = "Error(Contract, #104)")]
fn test_delay_above_maximum_rejected() {
    let env = make_env();
    let contract_id = setup(&env);
    env.as_contract(&contract_id, || {
        CampaignContract::set_unfreeze_delay(env.clone(), MAX_UNFREEZE_DELAY + 1)
    });
}

/// The maximum itself is accepted (boundary is inclusive).
#[test]
fn test_delay_at_maximum_accepted() {
    let env = make_env();
    let contract_id = setup(&env);
    env.as_contract(&contract_id, || {
        CampaignContract::set_unfreeze_delay(env.clone(), MAX_UNFREEZE_DELAY)
    });
    let d = env.as_contract(&contract_id, || {
        CampaignContract::get_unfreeze_delay(env.clone())
    });
    assert_eq!(d, MAX_UNFREEZE_DELAY);
}

// ─── Pre-existing behaviour intact ───────────────────────────────────────────

/// `freeze()` itself is unchanged: still immediate, still blocks mutations.
#[test]
fn test_freeze_still_immediate_and_blocking() {
    let env = make_env();
    let contract_id = setup(&env);

    assert!(!env.as_contract(&contract_id, || is_frozen(&env)));
    env.as_contract(&contract_id, || CampaignContract::freeze(env.clone()));
    assert!(env.as_contract(&contract_id, || is_frozen(&env)));
}

/// A frozen contract still rejects mutating operations with `ContractFrozen`
/// — the grace window does not weaken the freeze itself.
#[test]
#[should_panic(expected = "Error(Contract, #80)")]
fn test_frozen_contract_still_blocks_mutations() {
    let env = make_env();
    let contract_id = setup(&env);

    env.as_contract(&contract_id, || CampaignContract::freeze(env.clone()));
    env.as_contract(&contract_id, || CampaignContract::end_campaign(env.clone()));
}
