# ADR 0002: Canonicalize `campaign` over `core`

**Status:** Accepted  
**Date:** 2026-07-26  
**Author:** Moonwalker-rgb  
**Supersedes:** None  

## Context

The repository originally contained two campaign-style contracts:

1. **`orbitchain-core`** (`crates/contracts/core/`) — a multi-campaign
   registry supporting campaign-by-ID, donation fees, withdrawal lifecycles,
   and paginated donation history.
2. **`orbitchain-campaign`** (`campaign/`) — a single-instance campaign
   contract with milestones, pro-rata refunds, asset whitelisting, freeze/upgrade
   support, and richer events.

Having two contracts with overlapping feature sets created confusion for
integrators and doubled the maintenance surface. A decision was needed on
which contract to treat as canonical going forward.

## Decision

**`orbitchain-campaign` is declared the canonical campaign implementation.**
`orbitchain-core` is frozen at v0.2.0 with a deprecation notice and will be
removed from the workspace in the next major release.

Key rationale:

| Factor | `campaign` advantage |
|--------|---------------------|
| Milestone-based funding | `core` has none; `campaign` has full lifecycle |
| Pro-rata refunds | `core` has none; `campaign` tracks per-donor per-asset |
| Asset whitelist | `core` accepts any Symbol; `campaign` validates accepted assets |
| Freeze / upgrade | `core` has none; `campaign` supports admin freeze and WASM upgrade |
| Governance scope | Single-instance model is simpler to reason about |
| Events | `campaign` has richer, more granular event emission |

Features in `core` that are intentionally **not** migrated:
- **BASE_FEE deduction** — `campaign` has zero fee by design.
- **Withdrawal lifecycle** — replaced by milestone release + refund model.
- **Multi-campaign by ID** — deploy multiple `campaign` instances instead.

The full feature comparison is documented in `docs/core-vs-campaign.md`.

## Consequences

- **Positive:** Single canonical implementation reduces integrator confusion
  and maintenance burden.
- **Positive:** All new feature work targets `campaign/`; no ambiguity.
- **Negative:** Existing `core` integrators must migrate to `campaign`. The
  migration path is documented in `docs/core-vs-campaign.md`.
- **Neutral:** `core` remains in-tree as a reference until the next major
  release, but receives no new features.
