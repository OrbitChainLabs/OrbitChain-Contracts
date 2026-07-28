# Event Topic Hashes (Generated)

Issue #98 — Event topic canonicalisation: backward-compatibility shim for
migrating off `Events::publish` tuples to `#[contractevent]` structs.

During the migration window both topic formats are emitted. After the
deprecation window the old string-based topics will be removed.

## How Hashes Are Generated

Each `#[contractevent]` struct below auto-generates a hashed topic via
Soroban SDK's `#[contractevent]` macro. The topic is derived from the
struct's SC-spec name using SHA-256:

```
topic = SHA-256(sc_spec_name)
```

Where `sc_spec_name` is the PascalCase struct name as it appears in the
source (e.g., `DonationReceived`, `MilestoneUnlocked`).

To verify hashes at compile time, build the WASM and inspect the contract
spec. The `cargo test` output will include the contract spec which contains
the exact topic hashes.

## Event Mapping

| Old String Topic(s)              | New Struct Name       | New Topic (hash of)    |
|----------------------------------|-----------------------|------------------------|
| `donation_received` + contract   | `DonationReceived`    | `DonationReceived`     |
| `milestone_unlocked` + contract  | `MilestoneUnlocked`   | `MilestoneUnlocked`    |
| `milestone_released` + contract  | `MilestoneReleased`   | `MilestoneReleased`    |
| `campaign`, `initialized`        | `CampaignInitialized` | `CampaignInitialized`  |
| `campaign`, `campaign_ended`     | `CampaignEnded`       | `CampaignEnded`        |
| `campaign`, `campaign_cancelled` | `CampaignCancelled`   | `CampaignCancelled`    |
| `campaign`, `campaign_goal_reached` | `CampaignGoalReached` | `CampaignGoalReached` |
| `campaign`, `deadline_extended`   | `DeadlineExtended`    | `DeadlineExtended`     |
| `campaign`, `refund_claimed`     | `RefundClaimed`       | `RefundClaimed`        |
| `campaign`, `asset_refund`       | `AssetRefund`         | `AssetRefund`          |
| `campaign`, `contract_upgraded`  | `ContractUpgraded`    | `ContractUpgraded`     |
| `campaign`, `contract_frozen`    | `ContractFrozen`      | `ContractFrozen`       |
| `campaign`, `contract_unfrozen`  | `ContractUnfrozen`    | `ContractUnfrozen`     |
| `campaign`, `asset_blocked`      | `AssetBlocked`        | `AssetBlocked`         |
| `campaign`, `asset_unblocked`    | `AssetUnblocked`      | `AssetUnblocked`       |
| `ms_skip`, `no_issuer`           | `MilestoneSkipped`    | `MilestoneSkipped`     |

## Migration Timeline

1. **Current (this PR)**: Both old string topics and new hashed topics are
   emitted via the backward-compatibility shim in `event.rs`.
2. **Next minor release**: Off-chain consumers switch to hashed topics.
3. **Two minors later**: Old string topic emissions removed from `event.rs`; all
   call sites use `events::*::publish()` directly.
4. **One release after that**: `event.rs` removed entirely.

## Consumer Migration Guide

If you are consuming events via Horizon's event streaming API or Soroban RPC:

### Before (string topics)
```json
{
  "topic": ["donation_received", "CDLXIK... (contract address)"],
  "data": {"0": "GA...", "1": 10000000, ...}
}
```

### After (hashed topics)
```json
{
  "topic": ["<sha256('DonationReceived')>"],
  "data": {"donor": "GA...", "amount": 10000000, ...}
}
```

During the migration window both formats are emitted, so consumers can
transition gradually.
