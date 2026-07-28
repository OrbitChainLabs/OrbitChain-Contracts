# ADR 0001: Record Architecture Decisions

**Status:** Accepted  
**Date:** 2026-07-26  
**Author:** Moonwalker-rgb  

## Context

The OrbitChain Contracts project has grown to include multiple crates (campaign,
common, token-bridge, batch-donor, tools, testkit) with several architectural
decisions made during development (canonicalization of `campaign` over `core`,
the refund window, storage key design, etc.). These decisions are currently
scattered across issue comments, PR descriptions, and informal documentation —
making it difficult for new maintainers to understand _why_ the codebase is
structured the way it is.

## Decision

We adopt **Architecture Decision Records (ADRs)** as described by
Michael Nygard [1]. An ADR is a short text file that captures:

- **Context** — the forces at play and the problem being solved.
- **Decision** — the change we are proposing and/or have implemented.
- **Consequences** — what becomes easier or more difficult as a result.

ADRs live under `docs/adr/` and follow a sequential numbering scheme
(`NNNN-title-with-dashes.md`). Once an ADR is merged, it is immutable —
superseding decisions are captured in a new ADR that references the old one.

## Consequences

- **Positive:** Maintainers can trace the rationale for key architectural
  choices without forensic archaeology across issues and PRs.
- **Positive:** ADRs serve as a lightweight onboarding document for new
  contributors.
- **Neutral:** ADRs introduce a small documentation maintenance burden. Authors
  of significant architectural changes should include an ADR in their PR.
- **Neutral:** ADRs are not a replacement for inline code comments or
  `docs/architecture.md`; they complement both.

## References

[1] Michael Nygard, "Documenting Architecture Decisions",
    https://cognitect.com/blog/2011/11/15/documenting-architecture-decisions
