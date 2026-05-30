use soroban_sdk::{contract, contractimpl, Env};
use crate::types::{Error, MilestoneData};
use crate::storage::get_milestone;

/// Issue #199 â€“ `get_milestone` view function
///
/// Returns the full `MilestoneData` for the milestone at `index`.
/// Panics with `Error::MilestoneNotFound` if the index is out of range.
/// No authentication required.
pub fn get_milestone_view(env: &Env, index: u32) -> MilestoneData {
    get_milestone(env, index).unwrap_or_else(|| {
        soroban_sdk::panic_with_error!(env, Error::MilestoneNotFound)
    })
}
