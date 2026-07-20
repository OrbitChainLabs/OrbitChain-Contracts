# End-to-end lifecycle tests

`run_e2e.sh` walks the full campaign lifecycle against a **real Soroban
network** — sequence numbers, ledger clock, event ordering, and an actual
token transfer, none of which `Env::default()` unit tests exercise:

```
init → donate → milestone unlock → release_milestone → balance assertion
```

## Running

```sh
make e2e                        # local sandbox (stellar/quickstart in Docker)
bash e2e/run_e2e.sh local       # same thing
bash e2e/run_e2e.sh futurenet   # same lifecycle against Futurenet (no Docker)
```

Requirements: `stellar` CLI and `curl`; Docker for `local` mode.

The script prints a step-by-step report and exits non-zero on the first
failed step:

```
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
E2E lifecycle report (local)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
  ✅ Preflight: required tooling
  ✅ Boot Soroban sandbox (Docker: stellar/quickstart:latest)
  ✅ Create + fund test accounts (creator, donor, beneficiary)
  ...
🎉 E2E PASSED — full lifecycle (init → donate → unlock → release) verified on-chain.
```

## What it does

1. Boots `stellar/quickstart` in Docker (`local` mode) or health-checks
   Futurenet, probing both the `/rpc` and `/soroban/rpc` paths.
2. Creates and friendbot-funds three accounts: creator, donor, beneficiary.
3. Builds and deploys `orbitchain-campaign` plus the native-XLM Stellar
   Asset Contract.
4. `initialize` — one milestone whose target equals the goal, XLM accepted.
5. `donate` — reaches the goal; asserts `get_total_raised`, the
   `GoalReached` status transition, and the milestone auto-unlock.
6. Settles the pledge with a real SAC transfer into the contract
   (`donate` records the pledge on-ledger but does not custody tokens).
7. `release_milestone` — asserts the `Released` state, the beneficiary's
   actual SAC balance, and `get_release_count`.

Each invocation uses the proper signer (`--source-account`), so every
`require_auth()` in the contract is exercised for real — no mocked auth.

## Environment

| Variable | Default | Purpose |
|---|---|---|
| `E2E_QUICKSTART_IMAGE` | `stellar/quickstart:latest` | sandbox image |
| `E2E_RPC_PORT` | `8000` | host port for the sandbox |
| `E2E_SKIP_BUILD` | unset | `1` reuses an existing wasm artifact |

## CI

The scheduled **E2E (Futurenet)** workflow
(`.github/workflows/e2e-futurenet.yml`) runs this same script against
Futurenet daily and on demand. It is deliberately not a PR gate — a live
public network can fail for reasons unrelated to any code change.
