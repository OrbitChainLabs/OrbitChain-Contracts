# Campaign Contract Architecture

## Module Tree

```
campaign/src/
├── lib.rs                  # Contract entrypoints and module facade (< 200 LOC)
├── contract.rs             # Lifecycle management (end, cancel, extend deadline, status)
├── event.rs                # Typed event emission helpers
├── get_all_milestones.rs   # Enriched milestone enumeration view
├── get_milestone.rs        # Single milestone view
├── multi_asset_release.rs  # Proportional multi-asset milestone release
├── release_milestone.rs    # Single-asset milestone release
├── reports.rs              # Campaign report and analytics helpers
├── storage.rs              # Persistent and temporary storage access
├── types.rs                # Domain types, error codes, storage keys
├── validation.rs           # Input validation and transition logic
├── views.rs                # Enriched milestone view types and helpers
└── test/                   # Integration and unit tests
    ├── mod.rs
    ├── bump_storage_tests.rs
    ├── claim_refund_tests.rs
    ├── concluded_ledger_tests.rs
    ├── get_campaign_status_tests.rs
    ├── integration_tests.rs
    ├── invariant_tests.rs
    ├── milestone_batch_tests.rs
    ├── negative_path_tests.rs
    ├── refund_eligibility_tests.rs
    └── release_milestone_tests.rs
```

## Module Responsibilities

### `lib.rs`
- Declares all public modules
- Defines `CampaignContract` struct and `#[contractimpl]` impl block
- Contains contract entrypoints that delegate to feature modules
- Constants: `VERSION`, `REFUND_WINDOW`, `MAX_DEADLINE_GAP_SECONDS`
- Re-exports workspace semver constants from `common::version`

### `contract.rs`
- `end_campaign` — transitions campaign to `Ended`
- `cancel_campaign` — transitions campaign to `Cancelled`
- `extend_deadline` — extends campaign deadline
- `get_campaign_status` — returns status with days remaining

### `event.rs`
- Typed event emission functions for all contract events

### `get_all_milestones.rs`
- `get_all_milestones_view` — returns enriched views for all milestones

### `get_milestone.rs`
- `get_milestone_view` — returns enriched view for a single milestone

### `multi_asset_release.rs`
- `release_milestone_multi_asset` — proportional multi-asset release

### `release_milestone.rs`
- `release_milestone` — single-asset milestone release

### `reports.rs`
- `build_campaign_report` — builds `CampaignReport` from storage
- `active_campaign_count` — returns 1 if campaign is accepting donations
- `calculate_refund_amount` — pro-rata refund with anti-dust floor

### `storage.rs`
- Persistent storage helpers (campaign, milestones, donors, counters)
- Temporary storage helpers (contract status, re-entrancy lock, freeze flag)
- TTL management (bump thresholds and amounts)

### `types.rs`
- `Error` enum — typed error codes
- `CampaignStatus` / `MilestoneStatus` — lifecycle enums
- `DataKey` — storage key enum
- Domain types (CampaignData, MilestoneData, DonorRecord, etc.)
- Event types (CampaignInitializedEvent, DonationReceivedEvent, etc.)

### `validation.rs`
- `require_creator` — asserts caller is the campaign creator
- `validate_assets` — validates asset codes are non-empty
- `validate_milestones` — validates milestone ordering and goal match
- `validate_campaign_transition` — validates campaign status transitions
- `validate_milestone_transition` — validates milestone status transitions
- `get_token_address_for_asset` — resolves asset to token contract address
- `resolve_asset_code` — resolves asset code string
- `check_refund_eligibility` — checks all refund conditions

### `views.rs`
- `MilestoneView` — enriched milestone view type
- `get_milestone_by_index` — returns enriched milestone at index
- `find_next_pending_index` — finds the next unreleased milestone index

## Module Dependency Graph

```
lib.rs
├── contract.rs (→ storage, event, validation)
├── event.rs
├── get_all_milestones.rs (→ storage, views)
├── get_milestone.rs (→ storage, views)
├── multi_asset_release.rs (→ storage, event)
├── release_milestone.rs (→ storage, event)
├── reports.rs (→ storage)
├── storage.rs (→ types)
├── types.rs
├── validation.rs (→ storage, types)
└── views.rs (→ storage, types)
```

No circular dependencies exist between feature modules. All modules depend
only on `storage`, `types`, and shared helpers.
