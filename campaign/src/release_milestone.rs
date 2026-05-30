use soroban_sdk::{Address, Env, token};
use crate::types::{Error, MilestoneStatus};
use crate::storage::{get_campaign, get_milestone, set_milestone};

/// Issue #207 â€“ `release_milestone` function
///
/// Releases funds for an unlocked milestone to the recipient.
/// Requires creator authorization.
/// Validates milestone status is `Unlocked`.
/// Transfers tokens from contract to recipient.
/// Sets milestone status to `Released`.
/// Emits `milestone_released` event.
pub fn release_milestone(env: &Env, milestone_index: u32, recipient: Address) {
    let campaign = get_campaign(env).unwrap_or_else(|| {
        soroban_sdk::panic_with_error!(env, Error::NotInitialized)
    });

    campaign.creator.require_auth();

    let mut milestone = get_milestone(env, milestone_index).unwrap_or_else(|| {
        soroban_sdk::panic_with_error!(env, Error::MilestoneNotFound)
    });

    if milestone.status != MilestoneStatus::Unlocked {
        soroban_sdk::panic_with_error!(env, Error::InvalidMilestoneTransition);
    }

    let release_amount = milestone.target_amount - milestone.released_amount;

    // Transfer each accepted asset proportionally
    for asset in campaign.accepted_assets.iter() {
        if let Some(issuer) = asset.issuer.clone() {
            let token_client = token::Client::new(env, &issuer);
            let asset_balance = token_client.balance(&env.current_contract_address());
            if asset_balance > 0 && release_amount > 0 {
                let transfer_amount = release_amount.min(asset_balance);
                token_client.transfer(&env.current_contract_address(), &recipient, &transfer_amount);
            }
        }
    }

    milestone.released_amount = milestone.target_amount;
    milestone.status = MilestoneStatus::Released;
    set_milestone(env, milestone_index, &milestone);

    env.events().publish(
        ("milestone", "released"),
        (milestone_index, release_amount, recipient),
    );
}
