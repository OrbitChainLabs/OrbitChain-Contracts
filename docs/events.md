# Contract Event Schemas

All events are emitted via `env.events().publish(topics, data)` and are filterable through Stellar Horizon's event streaming API.

---

## `campaign_initialized`

Emitted once when the campaign contract is successfully initialized.

**Topics:** `["campaign", "initialized"]`

**Data:**

| Field | Type | Description |
|---|---|---|
| `creator` | `Address` | Campaign creator's Stellar address |
| `goal_amount` | `i128` | Total funding target in base units |
| `end_time` | `u64` | UNIX timestamp after which donations are rejected |
| `asset_count` | `u32` | Number of accepted assets |
| `milestone_count` | `u32` | Number of milestones registered |

---

## `donation_received`

Emitted after every successful donation, once storage has been updated.

**Topics:** `["donation_received", contract_address]`

**Data:**

| Field | Type | Description |
|---|---|---|
| `donor` | `Address` | Donor's Stellar address |
| `amount` | `i128` | Donated amount in base units |
| `asset_code` | `String` | Asset code (e.g. `"XLM"`, `"USDC"`) |
| `raised_total` | `i128` | Cumulative raised amount after this donation |
| `timestamp` | `u64` | Ledger timestamp of the donation |

---

## `milestone_unlocked`

Emitted once per milestone when its target is first reached. Not re-emitted if the milestone is already unlocked.

**Topics:** `["milestone_unlocked", contract_address]`

**Data:**

| Field | Type | Description |
|---|---|---|
| `milestone_index` | `u32` | Zero-based milestone index |
| `target_amount` | `i128` | Funding threshold that triggered the unlock |
| `raised_total` | `i128` | Cumulative raised amount at time of unlock |

---

## `milestone_released`

Emitted after each successful token transfer during milestone release.
When a multi-asset release transfers tokens from multiple assets, a separate event
is emitted per asset.

**Topics:** `["milestone_released", contract_address]`

**Data:**

| Field | Type | Description |
|---|---|---|
| `milestone_index` | `u32` | Zero-based milestone index |
| `amount` | `i128` | Amount transferred in this asset's base units |
| `asset_code` | `String` | Asset code (e.g. `"XLM"`, `"USDC"`) |
| `recipient` | `Address` | Address that received the funds |
| `timestamp` | `u64` | Ledger timestamp of the release |

---

## `milestone_release_skipped`

Emitted when an asset is skipped during milestone release due to missing issuer,
dust amount, or other release condition.

**Topics:** `["campaign", "milestone_release_skipped"]`

**Data:**

| Field | Type | Description |
|---|---|---|
| `milestone_index` | `u32` | Zero-based milestone index |
| `asset_code` | `String` | Asset code that was skipped (e.g. `"XLM"`) |
| `reason` | `Symbol` | Skip reason: `"no_issuer"`, `"dust_below_minimum"`, `"zero_release_amount"`, or `"already_released"` |

---

## `milestone_release_completed`

Emitted after all per-asset releases complete for a milestone.
Allows indexers to detect the end of a release without counting individual events.

**Topics:** `["campaign", "milestone_release_completed"]`

**Data:**

| Field | Type | Description |
|---|---|---|
| `milestone_index` | `u32` | Zero-based milestone index |
| `total_released` | `i128` | Total amount released across all assets in stroops |
| `asset_count` | `u32` | Number of accepted assets in the campaign |
| `timestamp` | `u64` | Ledger timestamp of the release |

---

## `campaign_ended`

Emitted when the campaign transitions to the `Ended` state (deadline passed or concluded normally).

**Topics:** `["campaign", "campaign_ended"]`

**Data:** `()` (no additional data)

---

## `campaign_cancelled`

Emitted when the campaign creator cancels the campaign.

**Topics:** `["campaign", "campaign_cancelled"]`

**Data:**

| Field | Type | Description |
|---|---|---|
| `creator` | `Address` | Campaign creator's Stellar address |

---

## `refund_claimed`

Emitted when a donor successfully claims a refund.

**Topics:** `["campaign", "refund_claimed"]`

**Data:**

| Field | Type | Description |
|---|---|---|
| `donor` | `Address` | Donor's Stellar address |
| `total_donated` | `i128` | Total amount donated by this donor |

---

## `asset_refund`

Emitted once per asset when a donor's pro-rata refund is transferred for that asset.
Multiple events may be emitted within a single `claim_refund` call when the donor
contributed more than one asset type.

**Topics:** `("campaign", "asset_refund")`

**Data:**

| Field | Type | Description |
|---|---|---|
| `donor` | `Address` | Donor's Stellar address |
| `asset_address` | `Address` | Contract address of the refunded asset |
| `refund_amount` | `i128` | Amount refunded in the asset's base units |

---

## `campaign_goal_reached`

Emitted when the campaign's raised amount reaches or exceeds the goal.

**Topics:** `["campaign", "campaign_goal_reached"]`

**Data:** `i128` — cumulative raised amount at the time the goal was reached.

---

## `deadline_extended`

Emitted when the campaign creator extends the campaign deadline.

**Topics:** `["campaign", "deadline_extended"]`

**Data:**

| Field | Type | Description |
|---|---|---|
| `creator` | `Address` | Campaign creator's Stellar address |
| `old_deadline` | `u64` | Previous deadline UNIX timestamp |
| `new_deadline` | `u64` | New deadline UNIX timestamp |

---

## `contract_upgraded`

Emitted when the contract WASM hash is upgraded by the admin.

**Topics:** `("campaign", "contract_upgraded")`

**Data:**

| Field | Type | Description |
|---|---|---|
| `admin` | `Address` | Admin (creator) address |
| `new_wasm_hash` | `BytesN<32>` | New WASM hash being deployed |
| `timestamp` | `u64` | Ledger timestamp of the upgrade |

---

## `contract_frozen`

Emitted when the contract is frozen by the admin, blocking all mutating operations.

**Topics:** `("campaign", "contract_frozen")`

**Data:**

| Field | Type | Description |
|---|---|---|
| `admin` | `Address` | Admin (creator) address |
| `timestamp` | `u64` | Ledger timestamp of the freeze |

---

## `contract_unfrozen`

Emitted when the contract is unfrozen by the admin, re-enabling mutating operations.

**Topics:** `("campaign", "contract_unfrozen")`

**Data:**

| Field | Type | Description |
|---|---|---|
| `admin` | `Address` | Admin (creator) address |
| `timestamp` | `u64` | Ledger timestamp of the unfreeze |

---

## Naming Convention

- Event names use `snake_case`.
- Topics are a tuple of `(event_name, contract_address)` for domain events, or `("campaign", event_name)` for lifecycle events.
- All amounts are in base units (stroops for XLM, smallest unit for other assets).
- All timestamps are UNIX seconds from the Soroban ledger (`env.ledger().timestamp()`).
