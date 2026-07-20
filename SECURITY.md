# Security Policy

## Reporting a Vulnerability

OrbitChain-Contracts contains Soroban smart contracts handling crowdfunding and fund management. Security vulnerabilities can have serious financial consequences. Please report them responsibly.

**Do NOT open a public issue to report a security vulnerability.**

### How to Report

- **GitHub Private Advisories**: Use [GitHub's private vulnerability reporting](https://github.com/OrbitChainLabs/OrbitChain-Contracts/security/advisories/new)
- **Email**: Contact the maintainers listed in the repository README

### What to Include

- Description of the vulnerability and its potential impact
- Steps to reproduce
- Affected contract(s) and function(s)
- Proof-of-concept (if available)
- Suggested fix (optional)

### Response Timeline

| Stage | Timeline |
|-------|----------|
| Acknowledgement | 48 hours |
| Initial triage | 5 business days |
| Fix or mitigation | 30 days for critical issues |

### Scope

High-priority security areas for this project:

- Smart contract logic errors (Soroban/Stellar)
- Arithmetic overflow/underflow in fund calculations
- Unauthorized access to admin or contributor functions
- Reentrancy or state manipulation vulnerabilities
- Incorrect access control on contract invocations

Thank you for helping keep OrbitChain and its users safe.

---

## Operational Playbook: Emergency Circuit Breaker

### Per-Asset Block List

The contract supports per-asset blocking as a graded incident response mechanism.
This allows an admin to halt donations in a specific asset without freezing the
entire campaign.

### Admin Functions

- **`block_asset(asset: Address)`** — Blocks donations in the specified token contract address.
  Only callable by the campaign creator/admin. Emits `asset_blocked` event.
- **`unblock_asset(asset: Address)`** — Unblocks donations in the specified token contract address.
  Only callable by the campaign creator/admin. Emits `asset_unblocked` event.
- **`is_asset_blocked_view(asset: Address) -> bool`** — Public view function; no auth required.

### Behaviour

- Donations in a blocked asset panic with `Error::AssetBlocked` (error code 90).
- All other assets continue to function normally while one is blocked.
- The block list is stored in persistent storage and survives across ledger cycles.
- The global freeze flag (`freeze`/`unfreeze`) still blocks all mutating operations
  regardless of per-asset block status.

### Recommended Incident Response Flow

1. **Detect** anomalous activity in a specific asset (e.g., suspected hot-wallet compromise).
2. **Block** the affected asset via `block_asset(asset)` — other assets remain operational.
3. **Investigate** off-chain while donations continue in unblocked assets.
4. **Resolve** by calling `unblock_asset(asset)` once the issue is contained.
5. If the entire contract must be halted, use `freeze()` (global freeze) instead.
