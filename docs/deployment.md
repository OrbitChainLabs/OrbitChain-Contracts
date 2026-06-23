# Contract Deployment Guide

## Prerequisites

- Rust + `rustup` (stable toolchain)
- `stellar-cli`: `cargo install --locked stellar-cli --features opt`
- WASM target: `rustup target add wasm32v1-none`
- A funded Stellar account (source keypair)

## Environment Variables

| Variable | Description |
|---|---|
| `STELLAR_SECRET_KEY` | Deployer keypair secret |
| `STELLAR_NETWORK` | `testnet` or `mainnet` |
| `STELLAR_RPC_URL` | Soroban RPC endpoint |

## Testnet Deployment

```bash
make setup && make build
stellar contract deploy   --wasm target/wasm32v1-none/release/campaign.wasm   --source $STELLAR_SECRET_KEY   --network testnet
```

## Verify Deployment

```bash
stellar contract invoke   --id $CONTRACT_ID   --source $STELLAR_SECRET_KEY   --network testnet   -- version
```

## Mainnet Deployment

```bash
stellar contract deploy   --wasm target/wasm32v1-none/release/campaign.wasm   --source $STELLAR_SECRET_KEY   --network mainnet   --rpc-url $STELLAR_RPC_URL
```

## Contract Initialization

```bash
stellar contract invoke   --id $CONTRACT_ID   --source $STELLAR_SECRET_KEY   --network testnet   -- initialize
```

## Error Code Stability

The campaign contract owns the canonical `#[contracterror]` enum for campaign
failures in `campaign/src/types.rs`. The shared `common` crate intentionally
does not publish a `#[contracterror]` enum; it only contains reusable data
types. This avoids overlapping stable discriminants between shared and
contract-local crates while preserving the campaign error numbers that may
already appear in `Error(Contract, #N)` results.

No migration or redeployment sequencing is required for this cleanup because no
existing campaign error discriminants were renumbered. Off-chain indexers should
continue to interpret campaign failures with the campaign error table.

## Deadline Extensions

Campaign deadline extensions are capped at ten years from the current ledger
timestamp. This prevents accidental or malicious `u64`-scale future dates from
making status views, refund-window checks, milestone release arithmetic, and
campaign reports meaningless while still allowing long-running campaigns.

## Troubleshooting

- **`InsufficientFee`**: Add `--fee 1000000` to the deploy command.
- **`WasmAlreadyExists`**: Binary is already on-chain; proceed directly to `invoke`.
- **WASM target missing**: Run `rustup target add wasm32v1-none`.

---

## Known Limitations / CLI Status

The `orbitchain-cli` binary (`crates/tools`) is in active development. Several
commands documented in earlier README revisions are **not yet implemented** in
the binary. This page is the canonical status table for every command name
that has ever appeared in user-facing documentation; it is the source of
truth that both the README and `crates/tools/src/main.rs`'s `help` output link
to. Tracker: [issue #37](https://github.com/OrbitChainLabs/OrbitChain-Contracts/issues/37).

### Status of every documented command

| Command documented in README | Status | Where it actually lives | Action |
|---|---|---|---|
| `orbitchain-cli config` | ✅ Implemented | `handle_config` in `crates/tools/src/main.rs` | Use as-is |
| `orbitchain-cli network` | ✅ Implemented | `handle_network` | Use as-is |
| `orbitchain-cli vault` | ✅ Implemented | `handle_vault` | Use as-is |
| `orbitchain-cli toggle <net>` | ✅ Implemented | `handle_toggle` | Use as-is |
| `orbitchain-cli asset …` | ✅ Implemented | `handle_asset` (5 sub-commands) | Use as-is |
| `orbitchain-cli keymanager …` | ✅ Implemented | `handle_keymanager` (6 sub-commands) | Use as-is |
| `orbitchain-cli keypair …` | ✅ Implemented | `handle_keypair` (7 sub-commands) | Use as-is |
| `orbitchain-cli signing …` | ✅ Implemented | `handle_signing` (5 sub-commands) | Use as-is |
| `orbitchain-cli response …` | ✅ Implemented | `handle_response` (5 sub-commands) | Use as-is |
| `orbitchain-cli deploy` | ⚠️ **Stub** | `handle_deploy` prints an "NOT yet implemented" banner | Use `make deploy-testnet` or `bash scripts/deploy.sh testnet` |
| `orbitchain-cli invoke <method>` | ⚠️ **Stub** | `handle_invoke` prints an "NOT yet implemented" banner | Use `stellar contract invoke --id $CONTRACT_ID --source <KEY> --network testnet -- <method> [args…]` |
| `orbitchain-cli account` | ⚠️ **Stub** | `handle_account` prints an "NOT yet implemented" banner | Use `orbitchain-cli keypair generate-master` (creation) or `keypair fund` (testnet funding) |
| `orbitchain-cli account create` | ❌ **Missing** | not in the dispatcher → `Unknown command` | Use `orbitchain-cli keypair generate-master` |
| `orbitchain-cli account fund` | ❌ **Missing** (under `account` namespace) | Implemented under `keypair fund` | Use `orbitchain-cli keypair fund <account> <amount_xlm>` |
| `orbitchain-cli config init` | ❌ **Missing** | not in the dispatcher → `Unknown command` | Run `cp .env.example .env` and edit it manually |
| `orbitchain-cli config check` | ❌ **Missing** (subcommand) | top-level `config` prints + validates everything already | Use `orbitchain-cli config` |
| `orbitchain-cli contract-id` | ❌ **Missing** | not in the dispatcher → `Unknown command` | Use `cat .orbitchain_contract_id` or `cat deployments/<network>.json` |
| `orbitchain-cli tx-history` | ❌ **Missing** | not in the dispatcher → `Unknown command` | Track in #37 |
| `orbitchain-cli batch` | ❌ **Missing** | not in the dispatcher → `Unknown command` | Track in #37 |
| `orbitchain-cli debug` | ❌ **Missing** | not in the dispatcher → `Unknown command` | Track in #37 |
| `orbitchain-cli contract query` | ❌ **Missing** | not in the dispatcher → `Unknown command` | Use `stellar contract invoke --simulate …` natively |
| `orbitchain-cli build-donation-tx` | ❌ **Missing** (top-level alias) | Implemented as `signing build-donation` | Use `orbitchain-cli signing build-donation …` |
| `orbitchain-cli submit-tx` | ❌ **Missing** (top-level alias) | Implemented as `response submit` (placeholder) | Use `response submit <file>` (placeholder) or `stellar contract invoke` natively |
| `orbitchain-cli verify-tx` | ❌ **Missing** | not in the dispatcher → `Unknown command` | Use Horizon / Soroban RPC events directly |
| `orbitchain-cli prepare-wallet-signing` | ❌ **Missing** | not in the dispatcher → `Unknown command` | Use `signing build-donation` (donation), `signing build-campaign` (creation) or `signing build-custom` (wrap XDR) |
| `orbitchain-cli complete-wallet-signing` | ❌ **Missing** | not in the dispatcher → `Unknown command` | Use `response process` (parse) or `response save` (persist) |
| `orbitchain-cli response submit` | ⚠️ **Placeholder** | `handle_response → "submit"` prints a planned flow but does not submit | Use `stellar contract invoke` natively for now |

### What to do if your command is missing

1. Confirm the command is `❌ Missing` above — the dispatcher will reply
   `❌ Unknown command: …` followed by the full list of implemented
   commands and a pointer to this page.
2. Use a working alternative from the table, or invoke the native
   `stellar contract …` command directly. Deployment wrappers live in
   `scripts/deploy.sh` and `Makefile` (`make deploy-testnet`,
   `make deploy-sandbox`).
3. If no alternative exists yet, track the gap as part of
   [issue #37](https://github.com/OrbitChainLabs/OrbitChain-Contracts/issues/37).

### Prior track reference

This CLI status audit is filed as a follow-up to
[issue #15](https://github.com/OrbitChainLabs/OrbitChain-Contracts/issues/15),
which originally triaged the `account create`/`fund` flows against the
campaign-lifecycle ledger. The current pass (#37) extends that traceability
across every CLI surface — implemented, stubbed, and missing — so future
contributors have a single source of truth before opening follow-up issues.
The post-#15 stabilization PRs (#60, #58, #54, #53, #57) established the
contract-test and CI baseline this audit relies on; they are not direct
descendants of #15, but adjacent work in the same release series.
