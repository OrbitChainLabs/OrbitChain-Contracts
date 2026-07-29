# WASM binary size — measurement, optimization, and regression gating

Issue #117. How the contract binaries are shrunk, how the reduction is
verified to be safe, and what was measured along the way.

## Results

`wasm-opt -Oz` (binaryen 130 — the version is pinned in CI, since older
distro-packaged binaryen produces measurably larger output) over the
`opt-level="z"` + fat-LTO cargo output:

| Contract | cargo output | after `wasm-opt -Oz` | reduction |
|---|---:|---:|---:|
| `orbitchain_campaign` | 85,322 B | 63,758 B | **−25.3%** |
| `orbitchain_core` | 38,767 B | 25,919 B | −33.1% |
| `orbitchain_batch_donor` | 18,072 B | 13,260 B | −26.6% |
| `orbitchain_token_bridge` | 1,454 B | 1,010 B | −30.5% |
| `orbitchain_common` | 817 B | 791 B | −3.2% |

All three Soroban custom sections (`contractspecv0`, `contractenvmetav0`,
`contractmetav0`) survive optimization intact — the spec remains readable by
`stellar contract info interface`.

## How to run

```bash
make optimize          # build + wasm-opt → *.optimized.wasm (non-destructive)
make optimize-verify   # optimize + size budgets + host-VM execution gate
```

`make deploy-sandbox` / `make deploy-testnet` now run `optimize` first and
`scripts/deploy.sh` installs the `.optimized.wasm` artifact, so the size win
actually reaches the chain instead of living only in a Makefile target.

## Why "without measurable regression" is enforced, not assumed

Two gates run in `make optimize-verify` and in the `wasm-size` CI job:

1. **Size budgets** (`scripts/wasm_size_check.sh`) — each optimized binary is
   compared against a checked-in byte budget (measured baseline + ~5%
   headroom). A regression that erases the win — or a broken `wasm-opt`
   invocation, which typically shows up as a near-unoptimized size — fails CI.
   Budgets are raised deliberately, in the PR that grows the contract.
2. **Host-VM execution** (`campaign/tests/optimized_wasm_exec.rs`) — the
   *optimized* campaign binary is loaded into the Soroban host VM and driven
   through `hello` → `version` → `initialize` (real `contracttype` struct
   arguments) → a storage-backed view. If `wasm-opt` ever miscompiles or
   strips something the host needs, this fails in CI instead of on-chain.

## What was measured and rejected: `#[inline]` annotations

The issue proposed targeted `#[inline]`/`#[inline(never)]` annotations. This
was tried and measured before being rejected:

- `#[inline(never)]` on the 13 hottest storage helpers (30+ call sites each)
  plus `#[cold]`/`#[inline(never)]` on the panic helper produced a
  **byte-identical binary** — same size, same MD5 — as the unannotated build.
- Gating the `#[derive(Debug)]`s behind `cfg(test)` (to drop ~2.7 KB of
  `core::fmt` machinery) produced **85,330 bytes vs 85,322 baseline**: a net
  zero. The `fmt` code is retained via SDK-internal vtables, not our derives.

Measurement integrity was verified with a canary: changing an exported
constant flips the build's MD5 and reverting restores the *exact* original
hash, so builds are deterministic and the toolchain demonstrably recompiled
between measurements. The nulls are real: at `opt-level="z"` with fat LTO and
`codegen-units=1`, LLVM already makes these inlining decisions optimally, and
annotations only add maintenance surface. The honest lever at the toolchain
level is `wasm-opt`, which is also what `stellar contract optimize` wraps.

Remaining size, for future reference: after optimization the binary is ~53%
code, ~38% `contractspecv0`. The spec section embeds every exported doc
comment; trimming docs would shrink it but is a documentation-quality trade,
not a code-size optimization, and is deliberately not done here.
