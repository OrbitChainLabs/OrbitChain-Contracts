# Contributing to OrbitChain

Thank you for your interest in contributing to OrbitChain! This document outlines the development workflow, prerequisites, and guidelines.

## Prerequisites

Before setting up the project, ensure you have the following tools installed:

### Required

- **Rust toolchain** (stable) — managed automatically by `rust-toolchain.toml`
  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  ```
- **wasm32v1-none target** — auto-installed when running `rustup show` in the project root
- **`stellar-cli`** — for contract deployment and interaction
  ```bash
  cargo install --locked stellar-cli --features opt
  ```

### Security Scanning Tools

These tools are required to run `make audit` and `make deny` locally. CI jobs install them automatically.

- **`cargo-audit`** — vulnerability scanning
  ```bash
  cargo install cargo-audit --locked
  ```
- **`cargo-deny`** — license compliance and policy checks
  ```bash
  cargo install cargo-deny --locked
  ```

## Getting Started

1. **Fork and clone the repository**

   ```bash
   git clone https://github.com/YOUR_USERNAME/OrbitChain-Contracts.git
   cd OrbitChain-Contracts
   ```

2. **Verify the toolchain**

   ```bash
   rustup show
   ```

3. **Build the project**

   ```bash
   make build
   ```

4. **Run tests**

   ```bash
   make test
   ```

## Development Workflow

### Branch Naming

Use conventional branch names:

- `feat/<description>` — new features
- `fix/<description>` — bug fixes
- `docs/<description>` — documentation updates
- `refactor/<description>` — code refactoring
- `chore/<description>` — maintenance tasks

### Commit Messages

Use [conventional commits](https://www.conventionalcommits.org/):

```
feat: add wallet connection modal
fix: resolve donation API error
docs: update project README
refactor: clean up project creation form
```

### Before Submitting a Pull Request

1. Ensure the project builds successfully:
   ```bash
   make build
   ```

2. Run all tests and ensure they pass:
   ```bash
   make test
   ```

3. Format your code:
   ```bash
   make fmt
   ```

4. Run the linter and fix any warnings:
   ```bash
   make lint
   ```

5. Run security scans (requires `cargo-audit` and `cargo-deny`):
   ```bash
   make audit
   make deny
   ```

## Pull Request Process

1. Create a branch from `main` with a descriptive name.
2. Make your changes and commit them with conventional commit messages.
3. Push your branch to your fork.
4. Open a pull request against the `main` branch of the upstream repository.
5. Ensure all CI checks pass (including security scans).
6. Request review from the maintainers.

## Code Style

- Follow Rust's standard formatting (`rustfmt`) — run `make fmt` before committing.
- Adhere to Clippy lint recommendations — run `make lint` to check.
- Write documentation comments for public APIs.
- Add unit tests for new functionality.

## Questions?

If you have questions or need help, open a GitHub Discussion or reach out to the maintainers.
