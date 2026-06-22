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

## Deadline Extensions

Campaign deadline extensions are capped at ten years from the current ledger
timestamp. This prevents accidental or malicious `u64`-scale future dates from
making status views, refund-window checks, milestone release arithmetic, and
campaign reports meaningless while still allowing long-running campaigns.

## Error-Code Migration Note

`campaign::types::Error` owns the contract-local `1..=999` error namespace.
`common::ErrorCode` owns the shared workspace `1000..=1099` namespace. This
keeps `Error(Contract, #N)` values unambiguous for off-chain indexers and any
future crate that imports both enums.

If a deployed integration previously interpreted `common::ErrorCode` values as
`1..=4`, migrate that integration before it consumes the shared crate again:
`NotInitialized=1000`, `AlreadyInitialized=1001`, `Unauthorized=1002`, and
`InvalidAmount=1003`. Campaign contract error values are unchanged.

## Troubleshooting

- **`InsufficientFee`**: Add `--fee 1000000` to the deploy command.
- **`WasmAlreadyExists`**: Binary is already on-chain; proceed directly to `invoke`.
- **WASM target missing**: Run `rustup target add wasm32v1-none`.
