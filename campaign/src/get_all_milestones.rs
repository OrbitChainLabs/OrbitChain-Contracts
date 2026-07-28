use soroban_sdk::{panic_with_error, Env, Vec};

use crate::storage::{get_campaign, get_milestone};
use crate::types::Error;
use crate::views::{find_next_pending_index, MilestoneView};

/// Issue #200 – Returns enriched views for ALL milestones in the campaign.
///
/// Returns an empty vec if the campaign is not initialised (though the caller
/// should guard against that).  No authentication required (read-only view).
///
/// # Panics
/// - `Error::NotInitialized` — contract not yet initialised.
#[must_use]
pub fn get_all_milestones_view(env: &Env) -> Vec<MilestoneView> {
    let campaign =
        get_campaign(env).unwrap_or_else(|| panic_with_error!(env, Error::NotInitialized));

    let next_pending = find_next_pending_index(env);

    let mut result: Vec<MilestoneView> = Vec::new(env);
    for i in 0..campaign.milestone_count {
        let data = get_milestone(env, i)
            .unwrap_or_else(|| panic_with_error!(env, Error::MilestoneNotFound));
        let pending_release = data.pending_release();
        let is_fully_released = data.is_fully_released();
        let is_next_pending = next_pending == i;
        result.push_back(MilestoneView {
            data,
            pending_release,
            is_fully_released,
            is_next_pending,
        });
    }
    result
}

// ─── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Address, Env};

    use crate::test::with_contract;
    use crate::types::{CampaignData, CampaignStatus, DataKey, MilestoneStatus};

    fn make_env() -> Env {
        Env::default()
    }

    fn seed_campaign(env: &Env, milestone_count: u32) {
        let creator = Address::generate(env);
        let campaign = CampaignData {
            creator,
            goal_amount: 10_000,
            raised_amount: 0,
            end_time: 9_999_999,
            status: CampaignStatus::Active,
            milestone_count,
            accepted_assets: soroban_sdk::Vec::new(env),
            min_donation_amount: 0,
            created_at_ledger: env.ledger().sequence(),
            created_at_time: env.ledger().timestamp(),
            concluded_at_ledger: None,
        };
        env.storage()
            .persistent()
            .set(&DataKey::CampaignData, &campaign);
    }

    fn seed_milestone(env: &Env, index: u32, status: MilestoneStatus) {
        let m = crate::types::MilestoneData {
            index,
            target_amount: (index as i128 + 1) * 1_000,
            released_amount: if status == MilestoneStatus::Released {
                (index as i128 + 1) * 1_000
            } else {
                0
            },
            description_hash: soroban_sdk::BytesN::from_array(env, &[0u8; 32]),
            status,
            released_at: None,
            released_at_ledger: None,
            release_tx: None,
            released_to: None,
        };
        env.storage()
            .persistent()
            .set(&DataKey::MilestoneData(index), &m);
    }

    #[test]
    fn returns_all_milestones_when_empty() {
        let env = make_env();
        with_contract(&env, || {
            seed_campaign(&env, 0);
            let result = get_all_milestones_view(&env);
            assert_eq!(result.len(), 0);
        });
    }

    #[test]
    fn returns_all_milestones_for_single() {
        let env = make_env();
        with_contract(&env, || {
            seed_campaign(&env, 1);
            seed_milestone(&env, 0, MilestoneStatus::Locked);
            let result = get_all_milestones_view(&env);
            assert_eq!(result.len(), 1);
            assert_eq!(result.get(0).unwrap().data.status, MilestoneStatus::Locked);
        });
    }

    #[test]
    fn returns_all_milestones_for_multiple() {
        let env = make_env();
        with_contract(&env, || {
            seed_campaign(&env, 3);
            seed_milestone(&env, 0, MilestoneStatus::Released);
            seed_milestone(&env, 1, MilestoneStatus::Unlocked);
            seed_milestone(&env, 2, MilestoneStatus::Locked);
            let result = get_all_milestones_view(&env);
            assert_eq!(result.len(), 3);
            assert_eq!(
                result.get(0).unwrap().data.status,
                MilestoneStatus::Released
            );
            assert_eq!(
                result.get(1).unwrap().data.status,
                MilestoneStatus::Unlocked
            );
            assert_eq!(result.get(2).unwrap().data.status, MilestoneStatus::Locked);
        });
    }

    #[test]
    #[should_panic]
    fn panics_when_not_initialised() {
        let env = make_env();
        with_contract(&env, || {
            let _ = get_all_milestones_view(&env);
        });
    }
}
