# Campaign Error Codes

All errors in the OrbitChain campaign contract are surfaced as typed `#[contracterror]` codes.
Each code maps to a unique `u32` discriminant visible in transaction results as `Error(Contract, #N)`.

## Quick Reference

| Code | Name | Message |
|------|------|---------|
| 1 | `AlreadyInitialized` | `initialize` called on an already-initialised contract. |
| 2 | `NotInitialized` | Contract has not been initialised yet. |
| 3 | `Unauthorized` | Caller is not authorised to perform the operation. |
| 4 | `CampaignEnded` | The campaign deadline has already passed. |
| 5 | `CampaignNotActive` | Operation requires the campaign to be Active or GoalReached. |
| 6 | `AssetNotAccepted` | Donated asset is not in the campaign's accepted assets list. |
| 7 | `DonationTooSmall` | Donation amount is below the campaign's minimum threshold. |
| 8 | `MilestoneNotFound` | Milestone index is out of range for this campaign. |
| 9 | `MilestoneNotUnlocked` | Milestone has not been unlocked yet and cannot be released. |
| 10 | `PreviousMilestoneNotReleased` | A previous milestone must be released before this one can be released. |
| 11 | `CannotCancelWithFunds` | Cannot cancel the campaign while it still holds funds. |
| 12 | `RefundWindowClosed` | Refunds are no longer permitted for this campaign. |
| 13 | `InvalidGoalAmount` | `goal_amount` must be strictly positive. |
| 14 | `InvalidEndTime` | `end_time` must be strictly greater than the current ledger timestamp. |
| 15 | `InvalidMilestones` | Milestones must be strictly ascending and the last must equal `goal_amount`. |
| 16 | `InsufficientContractBalance` | Contract does not hold enough funds to fulfil the requested transfer. |
| 17 | `Overflow` | A checked arithmetic operation overflowed. |
| 18 | `InvalidAssets` | `accepted_assets` must be non-empty. |
| 19 | `InvalidAssetCode` | `asset_code` must be non-empty and ≤ 12 characters. |
| 20 | `MilestoneMismatch` | Last milestone `target_amount` does not equal `goal_amount`. |
| 21 | `InvalidMilestoneCount` | Milestone count must be in the range [1, MAX_MILESTONES]. |
| 22 | `InvalidCampaignTransition` | The requested campaign status transition is not permitted. |
| 23 | `InvalidMilestoneTransition` | The requested milestone status transition is not permitted. |
| 24 | `GoalNotReached` | Cannot transition to GoalReached — raised amount < goal. |
| 25 | `InvalidStorageValue` | A storage read returned an unexpectedly invalid value. |
| 26 | `StorageWriteError` | A storage write failed (entry too large, quota exceeded, etc.). |
| 30 | `InvalidRecipient` | Recipient address is the contract itself — would lock funds permanently. |
| 31 | `MissingIssuerAddress` | The asset has no issuer address; transfers require a token contract. |
| 32 | `ZeroReleaseAmount` | Computed release amount is zero after proportional rounding. |
| 33 | `NothingToRelease` | `released_amount` already equals `target_amount`; nothing left to release. |
| 34 | `MilestoneReleasedExceedsTarget` | `released_amount` would exceed `target_amount` after this operation. |
| 40 | `MilestoneAlreadyReleased` | Milestone is already in the Released state. |
| 41 | `UnreleasedMilestonesExist` | All milestones must be Released before the campaign can be concluded. |
| 50 | `RefundNotPermitted` | Refunds only permitted when Cancelled or Ended without reaching goal. |
| 51 | `NoDonorRecord` | No donor record found for the requesting address. |
| 52 | `RefundAlreadyClaimed` | Donor has already claimed a refund for this campaign. |
| 60 | `ReentrantCall` | A re-entrant call was detected; operation aborted. |
| 70 | `InvalidAmount` | A generic negative or otherwise invalid amount was supplied. |
| 80 | `ContractFrozen` | Contract is frozen; all mutating operations are blocked. |

## CLI Usage

The `orbitchain-cli` binary exposes an `errors` subcommand for querying this mapping at the terminal:

```sh
# List all error codes
orbitchain-cli errors list

# Export as JSON
orbitchain-cli errors json

# Look up a single code
orbitchain-cli errors lookup 1
# → Name: AlreadyInitialized
# → Message: initialize called on an already-initialised contract.
```

## Adding a New Error Code

1. Add the variant to `campaign/src/types.rs` with a unique discriminant.
2. Add a corresponding entry in `crates/tools/src/error_mapper.rs` in the correct numeric range.
3. Add the row to this table in `docs/errors.md`.
4. Update the `campaign_error_discriminants_are_unique_without_common_error_space` test in `types.rs`.
5. Update the `test_load_builtin_has_all_campaign_errors` test in `error_mapper.rs` to reflect the new count.

## Source of Truth

- **Error enum**: `campaign/src/types.rs`
- **CLI mapping**: `crates/tools/src/error_mapper.rs`
- **Documentation**: `docs/errors.md`
