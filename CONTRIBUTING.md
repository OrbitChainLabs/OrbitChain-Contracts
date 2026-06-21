# Contributing to OrbitChain-Contracts

Thanks for your interest in contributing! OrbitChain is an on-chain crowdfunding protocol built on the Stellar Network using Soroban smart contracts. Because these contracts handle real funds, contributions are held to a high standard for correctness, security, and clarity.

This guide covers how to set up your environment, build and test the contracts, and submit a pull request. To report a security vulnerability, please follow the [Security Policy](SECURITY.md) instead of opening a public issue or PR.

## Code of Conduct

Please be respectful and constructive. We want OrbitChain to be a welcoming project for newcomers and experienced contributors alike.

## Getting Started

### Prerequisites

- **Rust** — stable toolchain, automatically managed by `rust-toolchain.toml`
- **`wasm32-unknown-unknown`** target — auto-installed by the toolchain
- **Soroban CLI** — for building, deploying, and testing contracts

### Setup

```bash
# 1. Fork the repo on GitHub, then clone your fork
git clone https://github.com/YOUR_USERNAME/orbitchain-contract.git
cd orbitchain-contract

# 2. Install Rust if needed; the toolchain file pins the version and targets
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup show

# 3. Install the Soroban CLI
cargo install soroban-cli

# 4. Build to confirm your environment works
make build
```

## Workspace Layout

OrbitChain is a Rust Cargo workspace:

- `campaign/` (`orbitchain-campaign`) — the **canonical** crowdfunding contract. All new campaign logic, analytics, and deployment work goes here.
- `crates/contracts/core/` (`orbitchain-core`) — legacy compatibility/reference contract. **Do not add new features here.**
- `crates/tools/` (`orbitchain-tools`) — CLI utilities for deployment, signing, and diagnostics.

> When in doubt about where new contract logic belongs, target `campaign/`.

## Development Workflow

The `Makefile` wraps the common tasks:

```bash
make build   # Build the WASM contract
make test    # Run all tests
make fmt     # Format code (rustfmt)
make lint    # Run clippy
make audit   # Scan dependencies for vulnerabilities (cargo-audit)
make deny    # Check license and ban policies (cargo-deny)
make help    # List all available targets
```

Before opening a PR, please make sure the following all pass locally:

```bash
make fmt
make lint
make test
make audit
make deny
```

You can also run the equivalent `cargo` commands directly — see the [README](README.md) for the full list.

## Commit Messages

Use [Conventional Commits](https://www.conventionalcommits.org/):

```
feat: add milestone refund flow
fix: correct overflow in donation total
docs: clarify deployment steps
refactor: simplify campaign state machine
test: add coverage for freeze controls
chore: bump soroban-sdk
```

Keep each commit focused, and describe *what* changed and *why*.

## Pull Requests

1. Create a topic branch off `main`:
   ```bash
   git checkout -b feat/short-description
   ```
2. Make your change, with tests where it makes sense.
3. Run the full local check suite above.
4. Push your branch and open a PR against `OrbitChainLabs/OrbitChain-Contracts:main`.
5. Fill out the PR template, link any related issues (e.g. `Closes #123`), and describe how you tested.

A maintainer will review your PR. Please keep the conversation focused and be responsive to review feedback. Security scans (`cargo-audit`, `cargo-deny`) and tests run automatically in CI and must pass before a PR can be merged.

## Security

Smart contracts handle real funds, so security review is taken seriously. Pay particular attention to:

- Arithmetic overflow/underflow in fund calculations
- Access control on admin and contributor functions
- Reentrancy and state-manipulation safety
- Correctness of milestone, refund, and freeze/upgrade flows

If you discover a vulnerability, **do not** open a public issue or PR — follow the [Security Policy](SECURITY.md) to report it privately.

## License

By contributing, you agree that your contributions will be licensed under the MIT License that covers this project.
