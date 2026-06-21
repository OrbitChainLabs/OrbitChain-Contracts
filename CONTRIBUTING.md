# Contributing

Thanks for helping improve OrbitChain. This guide covers the local checks contributors should run before opening a pull request.

## Prerequisites

- Rust stable toolchain, managed by `rust-toolchain.toml`
- `wasm32v1-none` target for Soroban contract builds
- Soroban/Stellar CLI for deployment workflows
- Security scan tools:

```bash
cargo install cargo-audit --locked
cargo install cargo-deny --locked
```

## Local Workflow

```bash
make fmt
make lint
make test
make audit
make deny
```

`make audit` checks dependencies with `cargo-audit`. `make deny` checks license and dependency policy with `cargo-deny`.

If either security tool is missing, the Makefile prints the exact `cargo install ... --locked` command and exits with a non-zero status before running the scan.

## Pull Request Checklist

- [ ] Run formatting, linting, and tests for the touched crates.
- [ ] Run `make audit` and `make deny`, or explain why they were not run.
- [ ] Update README or contract docs when behavior, commands, or contributor workflow changes.
- [ ] Call out security-sensitive changes, especially auth, signatures, fund movement, or dependency policy updates.
