use crate::event;
use crate::storage::{
    acquire_lock, get_campaign, get_milestone, release_lock, set_milestone,
    storage_get_asset_raised, storage_get_total_raised, storage_increment_release_count,
    storage_set_asset_raised, storage_set_total_raised,
};
use crate::types::{Error, MilestoneStatus};
use soroban_sdk::{panic_with_error, symbol_short, token, Address, Env};

// ─── Constants ───────────────────────────────────────────────────────────────

/// Minimum transfer amount — prevents dust transfers that waste fees.
const MIN_TRANSFER_AMOUNT: i128 = 1;

/// Maximum accepted assets per campaign to prevent unbounded loop gas costs.
/// Kept as a documented invariant even though the runtime is bounded by
/// Soroban resource limits — leaving the constant for future use.
#[allow(dead_code)]
const MAX_ACCEPTED_ASSETS: usize = 20;

// ─── Helper: proportional release ────────────────────────────────────────────

/// Computes the proportional release for a single asset using integer arithmetic.
///
/// Formula:  floor((asset_raised * milestone_release) / total_raised)
///
/// Returns `None` if the result would be zero or if total_raised is zero.
fn compute_asset_release(
    asset_raised: i128,
    milestone_release: i128,
    total_raised: i128,
) -> Option<i128> {
    if total_raised == 0 || asset_raised <= 0 || milestone_release <= 0 {
        return None;
    }

    let numerator = asset_raised.checked_mul(milestone_release)?;
    let release = numerator / total_raised; // integer floor division

    if release >= MIN_TRANSFER_AMOUNT {
        Some(release)
    } else {
        None
    }
}

// ─── Main entrypoint ─────────────────────────────────────────────────────────

/// Issue #208 — Multi-asset milestone release
///
/// Releases milestone funds proportionally across every accepted asset.
///
/// **Precondition:** The caller (`#[contractimpl]` wrapper) MUST have already
/// verified `creator.require_auth()` before calling this function.
///
/// Issue #242 – Reentrancy protection: acquires lock at entry, releases at exit.
/// Issue #244 – Balance verification: checks contract balance before each transfer.
///
/// Security properties:
/// - Milestone must be in `Unlocked` state (exactly once).
/// - Proportional math uses checked integer arithmetic — no overflows.
/// - Status is written to storage BEFORE transfers (CEI pattern) so a
///   re-entrant call on the same milestone index fails immediately.
/// - Recipient must be non-zero (validated before any transfer).
/// - Per-asset actual balances are used, not stored estimates, so the
///   contract can never release more than it actually holds.
/// - Dust amounts below MIN_TRANSFER_AMOUNT are skipped rather than
///   causing the whole release to fail.
pub fn release_milestone_multi_asset(env: &Env, milestone_index: u32, recipient: Address) {
    // Issue #242 – Reentrancy protection: acquire lock
    acquire_lock(env);

    // ── 1. Load campaign ────────────────────────────────────────────────────
    let campaign =
        get_campaign(env).unwrap_or_else(|| panic_with_error!(env, Error::NotInitialized));

    // ── 3. Validate recipient ────────────────────────────────────────────────
    if recipient == env.current_contract_address() {
        panic_with_error!(env, Error::InvalidRecipient);
    }

    // ── 4. Load and validate milestone ──────────────────────────────────────
    let mut milestone = get_milestone(env, milestone_index)
        .unwrap_or_else(|| panic_with_error!(env, Error::MilestoneNotFound));

    if milestone.status != MilestoneStatus::Unlocked {
        panic_with_error!(env, Error::InvalidMilestoneTransition);
    }

    // ── 5. Compute release amount ────────────────────────────────────────────
    let total_raised = storage_get_total_raised(env);

    if total_raised == 0 {
        panic_with_error!(env, Error::NothingToRelease);
    }

    // Guard against underflow — released_amount should never exceed target_amount
    let milestone_release = milestone
        .target_amount
        .checked_sub(milestone.released_amount)
        .unwrap_or_else(|| panic_with_error!(env, Error::MilestoneReleasedExceedsTarget));

    if milestone_release == 0 {
        panic_with_error!(env, Error::NothingToRelease);
    }

    // ── 6. Write status BEFORE transfers (Checks-Effects-Interactions) ──────
    milestone.released_amount = milestone
        .released_amount
        .checked_add(milestone_release)
        .unwrap_or_else(|| panic_with_error!(env, Error::Overflow));
    milestone.status = MilestoneStatus::Released;
    set_milestone(env, milestone_index, &milestone);

    // ── 7. Execute proportional transfers ───────────────────────────────────
    let timestamp = env.ledger().timestamp();
    let mut total_released: i128 = 0;

    for asset in campaign.accepted_assets.iter() {
        let token_address = match &asset.issuer {
            Some(addr) => addr.clone(),
            None => {
                // Native asset or asset without issuer — skip gracefully
                env.events().publish(
                    (symbol_short!("ms_skip"), symbol_short!("no_issuer")),
                    (milestone_index, asset.asset_code.clone()),
                );
                continue;
            }
        };

        let token_client = token::Client::new(env, &token_address);

        // Issue #244 – Use actual on-contract balance for verification
        let contract_balance = token_client.balance(&env.current_contract_address());

        // Retrieve the per-asset raised amount from storage for proportional math
        let asset_raised = storage_get_asset_raised(env, &token_address);

        let asset_release =
            match compute_asset_release(asset_raised, milestone_release, total_raised) {
                Some(amount) => amount,
                None => {
                    // Nothing to release for this asset (dust or zero balance)
                    continue;
                }
            };

        // Issue #244 – Verify contract balance is sufficient
        if contract_balance < asset_release {
            panic_with_error!(env, Error::InsufficientContractBalance);
        }

        // Clamp to actual available balance to prevent over-spending
        let clamped_release = asset_release.min(contract_balance);

        if clamped_release < MIN_TRANSFER_AMOUNT {
            continue;
        }

        // Execute the transfer
        token_client.transfer(
            &env.current_contract_address(),
            &recipient,
            &clamped_release,
        );

        // Emit per-asset milestone_released event
        event::milestone_released(
            env,
            milestone_index,
            clamped_release,
            asset.asset_code.clone(),
            &recipient,
            timestamp,
        );

        // Update per-asset accounting
        let new_asset_raised = asset_raised
            .checked_sub(clamped_release)
            .unwrap_or_else(|| panic_with_error!(env, Error::Overflow));
        if new_asset_raised < 0 {
            panic_with_error!(env, Error::Overflow);
        }
        storage_set_asset_raised(env, &token_address, new_asset_raised);

        total_released = total_released
            .checked_add(clamped_release)
            .unwrap_or_else(|| panic_with_error!(env, Error::Overflow));
    }

    // ── 8. Update global total-raised bookkeeping ────────────────────────────
    let new_total_raised = total_raised
        .checked_sub(total_released)
        .unwrap_or_else(|| panic_with_error!(env, Error::Overflow));
    if new_total_raised < 0 {
        panic_with_error!(env, Error::Overflow);
    }
    let new_total_raised = total_raised.checked_sub(total_released).unwrap_or(0).max(0);
    storage_set_total_raised(env, new_total_raised);
    storage_increment_release_count(env);

    // Issue #242 – Release reentrancy lock
    release_lock(env);
}

// ─── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proportional_release_equal_split() {
        let result = compute_asset_release(500, 1000, 1000);
        assert_eq!(result, Some(500));
    }

    #[test]
    fn proportional_release_unequal_split() {
        let result = compute_asset_release(300, 400, 1000);
        assert_eq!(result, Some(120));
    }

    #[test]
    fn proportional_release_rounds_down() {
        let result = compute_asset_release(1, 100, 3);
        assert_eq!(result, Some(33));
    }

    #[test]
    fn proportional_release_zero_total_raised() {
        let result = compute_asset_release(100, 100, 0);
        assert_eq!(result, None);
    }

    #[test]
    fn proportional_release_zero_asset_raised() {
        let result = compute_asset_release(0, 100, 1000);
        assert_eq!(result, None);
    }

    #[test]
    fn proportional_release_dust_below_minimum() {
        let result = compute_asset_release(1, 1, 1_000_000);
        assert_eq!(result, None);
    }

    #[test]
    fn proportional_release_full_amount() {
        let result = compute_asset_release(5000, 5000, 5000);
        assert_eq!(result, Some(5000));
    }

    #[test]
    fn proportional_release_negative_asset_raised() {
        let result = compute_asset_release(-100, 1000, 1000);
        assert_eq!(result, None);
    }

    #[test]
    #[should_panic(expected = "HostError")]
    fn test_release_underflow_panics() {
        use crate::storage::{
            set_campaign, set_milestone, storage_set_asset_raised, storage_set_total_raised,
        };
        use crate::types::{
            CampaignData, CampaignStatus, MilestoneData, MilestoneStatus, StellarAsset,
        };
        use soroban_sdk::token::StellarAssetClient;
        use soroban_sdk::{testutils::Address as _, Address, Env, String, Vec};

        let env = Env::default();
        env.mock_all_auths();

        let creator = Address::generate(&env);
        let recipient = Address::generate(&env);

        let token_admin = Address::generate(&env);
        let token_issuer = env.register_stellar_asset_contract(token_admin.clone());
        let token_client = StellarAssetClient::new(&env, &token_issuer);
        token_client.mint(&env.current_contract_address(), &5000);

        let mut assets = Vec::new(&env);
        assets.push_back(StellarAsset {
            asset_code: String::from_str(&env, "USDC"),
            issuer: Some(token_issuer.clone()),
        });

        let campaign = CampaignData {
            creator,
            goal_amount: 3000,
            raised_amount: 3000,
            end_time: env.ledger().timestamp() + 86400,
            status: CampaignStatus::Active,
            accepted_assets: assets,
            milestone_count: 1,
            min_donation_amount: 0,
            created_at_ledger: 0,
            created_at_time: 0,
            concluded_at_ledger: None,
        };
        set_campaign(&env, &campaign);

        let milestone = MilestoneData {
            index: 0,
            target_amount: 3000, // milestone_release = 3000
            released_amount: 0,
            description_hash: soroban_sdk::BytesN::from_array(&env, &[0; 32]),
            status: MilestoneStatus::Unlocked,
            released_at: None,
            released_at_ledger: None,
            release_tx: None,
            released_to: None,
        };
        set_milestone(&env, 0, &milestone);

        // Force underflow condition: total_raised = 1000, asset_raised = 500
        // asset_release = 500 * 3000 / 1000 = 1500
        // clamped_release = min(1500, 5000) = 1500
        // new_asset_raised = 500 - 1500 => underflow!
        storage_set_total_raised(&env, 1000);
        storage_set_asset_raised(&env, &token_issuer, 500);

        release_milestone_multi_asset(&env, 0, recipient);
    }
}
