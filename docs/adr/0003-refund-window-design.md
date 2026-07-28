# ADR 0003: Refund Window Design

**Status:** Accepted  
**Date:** 2026-07-26  
**Author:** Moonwalker-rgb  
**Supersedes:** None  

## Context

The campaign contract supports donor refunds when a campaign is cancelled or
ends without reaching its goal. A critical design question was: _how long
should the refund window remain open?_

Key constraints:

1. **Donor protection** — donors need reasonable time to discover that a
   campaign has ended/cancelled and submit a claim transaction.
2. **Creator finality** — after the window closes, the creator needs certainty
   that remaining funds won't be drained and that their milestone-based release
   model isn't undermined.
3. **On-chain simplicity** — the refund eligibility check must be a pure
   function of ledger state, with no dependence on off-chain oracles or
   human-mediated processes.
4. **Soroban ledger constraints** — Soroban contracts have limited access to
   absolute time (only `env.ledger().timestamp()`), so the window must be
   expressed in ledger seconds.

## Decision

**The refund window is set to 30 days (2,592,000 ledger seconds) from the
campaign's `end_time`.**

This is implemented as:

```rust
pub const REFUND_WINDOW: u64 = 30 * 24 * 60 * 60;
```

The eligibility check (`check_refund_eligibility` in `validation.rs`) verifies
that `env.ledger().timestamp() <= campaign.end_time + REFUND_WINDOW`. If the
current timestamp exceeds this threshold, the refund is rejected with
`Error::RefundWindowClosed`.

### Alternatives Considered

| Alternative | Rejected because |
|-------------|-----------------|
| **7 days** | Too short — donors on vacation or offline may miss the window entirely |
| **90 days** | Too long — delays creator finality and extends treasury uncertainty |
| **No window** | Unbounded refund liability undermines milestone release economics |
| **Dynamic window** | Adds complexity; no clear benefit over a fixed, predictable constant |

**30 days** was chosen as the pragmatic middle ground: long enough for donors
to notice and act, short enough for creators to achieve timely finality.

## Consequences

- **Positive:** Donors have a clear, predictable window to claim refunds.
- **Positive:** Creators have certainty that after 30 days post-end, no further
  refund claims will succeed.
- **Positive:** The constant is public (`campaign::REFUND_WINDOW`) and
  discoverable by indexers and UIs.
- **Negative:** A 30-day window may feel long for campaigns that end with
  minimal funds, but shortening it would require a governance change (new
  contract version).
- **Neutral:** Changing the window in the future requires a contract upgrade
  via the `upgrade()` entrypoint, which is admin-gated.
