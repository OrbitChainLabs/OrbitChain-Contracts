# `orbitchain-core` vs `orbitchain-campaign` Feature Comparison

> Created as part of issue #96 — tracking the migration of legacy `orbitchain-core`
> features into the canonical `campaign/` contract.

## Status Legend

| Mark | Meaning |
|------|---------|
| ✅    | Implemented in `campaign/` |
| ⚠️   | Partially implemented / different semantics |
| ❌    | Not implemented |
| 🗑️   | Should not be migrated (legacy design, not needed) |

## Feature Table

| # | Feature | `core` | `campaign` | Notes |
|---|---------|--------|------------|-------|
| 1 | Single-campaign contract | ❌ | ✅ | `campaign` is single-instance only. `core` supports multiple campaigns by ID. |
| 2 | Campaign initialization | ✅ | ✅ | `core` uses `create_campaign`, `campaign` uses `initialize`. Different parameters. |
| 3 | Campaign status (active/inactive) | ⚠️ | ✅ | `core` has a simple `active: bool`. `campaign` has full lifecycle: Active → GoalReached → Ended/Cancelled. |
| 4 | Donate with fee | ✅ | 🗑️ | `core` deducts `BASE_FEE = 100` stroops. `campaign` intentionally has no fee; migration not needed. |
| 5 | Donate without fee | ❌ | ✅ | `campaign` accepts full donation amount. |
| 6 | Asset whitelist / accepted assets | ❌ | ✅ | `campaign` validates donations against `accepted_assets`. `core` accepts any Symbol. |
| 7 | Per-asset raised tracking | ✅ | ✅ | Both track raised per asset. Different key structures. |
| 8 | Minimum donation amount | ❌ | ✅ | `campaign` supports `min_donation_amount`. |
| 9 | Deadline enforcement | ❌ | ✅ | `campaign` enforces `end_time`. `core` does not enforce deadline. |
| 10 | Campaign deadline extension | ❌ | ✅ | |
| 11 | Milestone-based funding | ❌ | ✅ | Unique to `campaign`. |
| 12 | Milestone unlock on goal | ❌ | ✅ | |
| 13 | Milestone release to beneficiary | ❌ | ✅ | |
| 14 | Multi-asset proportional release | ❌ | ✅ | |
| 15 | Campaign goal tracking | ❌ | ✅ | `core` has `goal` but no GoalReached transition. |
| 16 | Freeze / unfreeze (global) | ❌ | ✅ | |
| 17 | Per-asset block list | ❌ | ✅ | Issue #90. |
| 18 | Emergency circuit breaker | ❌ | ✅ | |
| 19 | Upgrade contract WASM | ❌ | ✅ | |
| 20 | Storage TTL bump | ❌ | ✅ | |
| 21 | Withdrawal lifecycle (Pending→Approved→Submitted) | ✅ | 🗑️ | `campaign` uses milestone release + refunds instead. Not migrating. |
| 22 | Donor record per address | ❌ | ✅ | `campaign` tracks `DonorRecord` per address. |
| 23 | Donor asset donation tracking | ❌ | ✅ | `campaign` tracks per-donor per-asset for pro-rata refunds. |
| 24 | Refund claiming | ❌ | ✅ | |
| 25 | Refund eligibility check | ❌ | ✅ | |
| 26 | Refund window | ❌ | ✅ | 30-day refund window after campaign end. |
| 27 | Donation metadata (memo) | ✅ | 🗑️ | `core` stores memo per donation. `campaign` does not need it (refund focus). |
| 28 | Donation history (paginated) | ✅ | 🗑️ | `core` stores history Vec per campaign. `campaign` uses event-based tracking. |
| 29 | Donor list per campaign | ✅ | ❌ | `core` tracks unique donor addresses. `campaign` tracks aggregate `DonorRecord`. |
| 30 | Unique donor count | ❌ | ✅ | |
| 31 | Donation count | ✅ | ✅ | Different storage approaches. |
| 32 | Release count | ❌ | ✅ | |
| 33 | Transaction count (donations + releases) | ❌ | ✅ | `core` counts donations + withdrawals. |
| 34 | Platform-wide campaign count | ✅ | ⚠️ | `campaign` returns 0 or 1 (single-instance). |
| 35 | Platform summary | ✅ | ✅ | Similar structure, different fields. |
| 36 | Dashboard metrics | ✅ | ✅ | Similar structure, different fields. |
| 37 | Campaign report | ✅ | ✅ | Similar structure, different fields. |
| 38 | Re-entrancy protection | ❌ | ✅ | |
| 39 | Authorization via `require_auth()` | ✅ | ✅ | Both enforce auth correctly. |
| 40 | Events | ✅ | ✅ | Different event schemas. `campaign` has richer events. |
| 41 | Error codes | ✅ | ✅ | `campaign` has more granular error codes. |
| 42 | Ping / hello health check | ✅ | ✅ | |
| 43 | Version string | ❌ | ✅ | |
| 44 | Contract status view | ❌ | ✅ | |

## Migration Decisions

### Features to Migrate (from `core` to `campaign`)

None. Every feature in `core` that is valuable is already implemented in `campaign`
with better semantics. The unique `core` features (fee handling, withdrawal lifecycle,
multi-campaign) are not appropriate for the `campaign` contract's design.

### Features to Decommission

| Feature | Reason |
|---------|--------|
| Multi-campaign by ID | `campaign` is designed as single-instance. Use multiple contract instances. |
| BASE_FEE deduction | `campaign` intentionally has zero fee. |
| Withdrawal lifecycle | Replaced by milestone release + refund model. |
| Donation memo/metadata | Not needed for the refund-based model. |

## Next Steps

1. Merge this comparison document.
2. Ship `orbitchain-core` as a final v0.2.0 release with a deprecation notice.
3. Delete `orbitchain-core` from the workspace in the next major release.
