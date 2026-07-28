<!-- <<< AUTO-GENERATED FROM CONTRACT — DO NOT EDIT >>> -->

# Generated campaign event topics

`#[contractevent]` derives the first topic from the lower-snake-case event
struct name. Fields annotated with `#[topic]` follow it in declaration order.

| Event | Topics |
|---|---|
| `CampaignInitialized` | `campaign_initialized`, `creator` |
| `DonationReceived` | `donation_received`, `donor` |
| `MilestoneUnlocked` | `milestone_unlocked`, `milestone_index` |
| `DeadlineExtended` | `deadline_extended`, `creator` |
| `CampaignCancelled` | `campaign_cancelled`, `creator` |
| `CampaignEnded` | `campaign_ended` |
| `MilestoneReleased` | `milestone_released`, `milestone_index`, `recipient` |
| `CampaignGoalReached` | `campaign_goal_reached` |
| `AssetRefund` | `asset_refund`, `donor`, `asset_address` |
| `RefundClaimed` | `refund_claimed`, `donor` |
| `ContractUpgraded` | `contract_upgraded`, `admin` |
| `ContractFrozen` | `contract_frozen`, `admin` |
| `ContractUnfrozen` | `contract_unfrozen`, `admin` |
| `AssetBlocked` | `asset_blocked`, `admin`, `asset` |
| `AssetUnblocked` | `asset_unblocked`, `admin`, `asset` |
| `MilestoneReleaseSkipped` | `milestone_release_skipped`, `milestone_index` |
