# 🌟 OrbitChain

[![CI](https://github.com/orbitchain/orbitchain-contracts/workflows/CI/badge.svg)](https://github.com/orbitchain/orbitchain-contracts/actions)

## Contract Canonicalization

Decision: **Option B (conservative)** — keep `campaign/` (`orbitchain-campaign`) as the canonical crowdfunding contract for all new development, audits, deployments, and integrations. The `campaign/` implementation remains the authoritative contract for milestone flows, refunds, freeze/upgrade controls, reentrancy protection, typed errors, and dashboard analytics.

`crates/contracts/core/` (`orbitchain-core`) is retained only as a legacy compatibility/reference contract. Do not add new campaign features there; use `campaign/` for any new logic, analytics endpoints, or deployment work. Any remaining behavior worth preserving from `core` should be migrated into `campaign/` before `core` is removed in a future breaking release.

Canonical campaign analytics now live on `orbitchain-campaign`: use `get_campaign_report`, `get_platform_summary`, `get_dashboard_metrics`, `get_donation_count`, `get_donor_count`, `get_release_count`, and `get_total_tx_count` for dashboard and export workflows.

**OrbitChain** is an on-chain crowdfunding protocol built on the **Stellar Network** and **Soroban smart contracts**. It provides a transparent, trust-minimized platform where campaign creators can raise funds in native XLM or any Stellar-based asset (USDC, NGNT, custom tokens), and donors retain full visibility into how their contributions are deployed.

The protocol is governed by a set of deterministic Soroban contracts — handling campaign lifecycle management, milestone-based fund release, multi-asset donation processing, and cross-chain token bridging — complemented by a comprehensive CLI toolchain for deployment, transaction signing, wallet integration, and network diagnostics.

## � Workspace Layout

This project uses a Rust Cargo workspace with the following structure:

```
orbitChain-contract/
|-- campaign/                  # Canonical campaign contract
|   |-- Cargo.toml
|   `-- src/
|       `-- lib.rs
├── Cargo.toml                 # Workspace configuration
├── crates/
│   ├── contracts/
│   │   └── core/             # Legacy compatibility/reference contract
│   │       ├── Cargo.toml
│   │       └── src/
│   │           └── lib.rs    # Contract implementation
│   └── tools/                # CLI utilities and deployment tools
│       ├── Cargo.toml
│       └── src/
│           └── main.rs       # CLI entry point
├── .gitignore
└── README.md
```

### Crates Overview

- **`orbitchain-campaign`**: Canonical Soroban crowdfunding contract for milestones, multi-asset donations, refunds, lifecycle controls, and analytics
- **`orbitchain-core`**: Legacy compatibility/reference contract; do not use for new campaign features
- **`orbitchain-tools`**: Advanced CLI utilities for contract deployment, configuration, transaction management, and debugging

## 🛠️ Implemented CLI Commands

The `orbitchain-cli` binary (in `crates/tools`) ships with a focused set of
sub-commands today. Anything **not** listed below is unimplemented and will
print either `❌ Unknown command: …` or a stubbed "not yet implemented"
banner with an issue link. Older documentation referenced several commands
that do not exist yet; the canonical status table lives in
[`docs/deployment.md`](docs/deployment.md#known-limitations--cli-status) and
is tracked in [issue #37](https://github.com/OrbitChainLabs/OrbitChain-Contracts/issues/37).

### Configuration & Network

- `config` — Print resolved environment and active network.
- `network` — Print active Soroban network (RPC, Horizon, passphrase).
- `vault` — Show SecureVault status and security best practices.
- `toggle <testnet|mainnet>` — Switch the active network profile.

### Asset Issuing

- `asset config` — Show asset configuration.
- `asset generate` — Generate issuing keypair.
- `asset check` — Check issuing readiness.
- `asset trustline <holder> [asset_code]` — Establish a trustline.
- `asset issue <recipient> <amount>` — Issue assets to a recipient.

### Key Management

- `keymanager encrypt <password> <secret_key>` — Encrypt a Stellar secret key.
- `keymanager decrypt <password> <encrypted_hex>` — Decrypt back to a secret key.
- `keymanager init-vault <password>` — Initialize an encrypted vault.
- `keymanager vault-status` / `vault-save <path>` / `vault-load <path> <password>` — Vault lifecycle.

### Keypair Lifecycle

- `keypair generate-master` — Generate a master keypair.
- `keypair generate-distribution <issuing_public_key>` — Generate a distribution account.
- `keypair show-master` / `keypair show-distribution` — Print stored keypairs (safe view).
- `keypair fund <account_public_key> <amount_xlm>` — Friendbot-fund a testnet account.
- `keypair validate-master` / `keypair validate-distribution` — Validate stored keypairs.

### Wallet Signing & Response

- `signing build-donation <donor> <campaign_id> <amount> [asset] [memo]` — Build a donation signing request.
- `signing build-campaign <creator> <title> <goal> <deadline>` — Build a campaign creation request.
- `signing build-custom <xdr> [description]` — Wrap an external XDR.
- `signing validate <json_file>` / `signing export <json_file>` — Validate or export.
- `response process <json>` / `response validate <file>` / `response save <json> <file>` / `response load <file>` — Wallet response lifecycle.
- `response submit <file>` — **Placeholder** for native network submission (tracked in #37).

### Quick Examples

```bash
# Inspect active configuration and network
orbitchain-cli config
orbitchain-cli network
orbitchain-cli toggle testnet

# Issue a custom asset and establish trustline
orbitchain-cli asset generate
orbitchain-cli asset trustline GABJ2... USDC
orbitchain-cli asset issue GABJ2... 100

# Build a donation signing request for a donor
orbitchain-cli signing build-donation GBJCHU... 1 5000000 XLM "Supporting education"

# Process the wallet's signed response
orbitchain-cli response process '{"requestId":"req_123","xdr":"AAAA...","signer":"GBJCHU...","signedAt":1234567890}'
```

For the full command list, run `orbitchain-cli` with no arguments.

## 🛠️ Development Setup

### Quick Start (New Contributors)

1. **Clone the repository**

   ```bash
   git clone https://github.com/YOUR_USERNAME/orbitchain-contract.git
   cd orbitchain-contract
   ```

2. **Install Rust toolchain** (automatically configured by `rust-toolchain.toml`)

   ```bash
   # Install Rust if not already installed
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

   # The project will automatically use the correct Rust version and targets
   rustup show
   ```

3. **Install Soroban CLI**

   ```bash
   # Method 1: Install via cargo (recommended for development)
   cargo install soroban-cli

   # Method 2: Install via npm (alternative)
   npm install -g soroban-cli

   # Verify installation
   soroban --version
   ```

4. **Build the project**

   ```bash
   # Using Make (recommended)
   make build

   # Or using cargo directly
   cargo build -p orbitchain-campaign --target wasm32v1-none
   ```

### Prerequisites

- **Rust stable toolchain** (automatically managed by `rust-toolchain.toml`)
- **wasm32v1-none target** (auto-installed by toolchain)
- **Soroban CLI** for contract deployment and testing

### Toolchain Configuration

This project uses `rust-toolchain.toml` to ensure consistent development environments:

```toml
[toolchain]
channel = "stable"
targets = ["wasm32v1-none"]
components = ["rustfmt", "clippy"]
```

This ensures:

- Consistent Rust version across all contributors
- Required targets are automatically installed
- Essential components (rustfmt, clippy) are included

### Development Commands

The project includes a Makefile for common development tasks:

```bash
# Build WASM contract
make build

# Run all tests
make test

# Format code
make fmt

# Run linter
make lint

# Clean build artifacts
make clean

# Show all available commands
make help
```

### Building (Manual)

```bash
# Build the canonical campaign contract for WASM
cargo build -p orbitchain-campaign --target wasm32v1-none --release

# Build the CLI tools
cargo build -p orbitchain-tools

# Build entire workspace
cargo build --workspace
```

### Testing

```bash
# Run canonical campaign contract tests
cargo test -p orbitchain-campaign

# Run all tests
cargo test --workspace
```

### CLI Usage

> The commands below match `crates/tools/src/main.rs` and the canonical
> status table in
> [`docs/deployment.md`](docs/deployment.md#known-limitations--cli-status).
> `deploy`, `invoke`, and `account` are currently stubs in the CLI binary;
> use the native `stellar contract …` commands or `make deploy-testnet`
> instead. `config init`, `contract-id`, `build-donation-tx`, `submit-tx`,
> `verify-tx`, `prepare-wallet-signing`, and `complete-wallet-signing`
> shown in older docs are **not implemented** — see issue
> [#37](https://github.com/OrbitChainLabs/OrbitChain-Contracts/issues/37).

```bash
# Inspect resolved configuration / network / vault
cargo run -p orbitchain-tools -- config
cargo run -p orbitchain-tools -- network
cargo run -p orbitchain-tools -- vault
cargo run -p orbitchain-tools -- toggle testnet

# Issue assets via the asset namespace
cargo run -p orbitchain-tools -- asset config
cargo run -p orbitchain-tools -- asset generate
cargo run -p orbitchain-tools -- asset trustline GABJ2... USDC
cargo run -p orbitchain-tools -- asset issue GABJ2... 100

# Encrypted vault operations
cargo run -p orbitchain-tools -- keymanager init-vault "$VAULT_MASTER_PASSWORD"
cargo run -p orbitchain-tools -- keymanager vault-status

# Keypair lifecycle (the entry point that replaced `account create|fund`)
cargo run -p orbitchain-tools -- keypair generate-master
cargo run -p orbitchain-tools -- keypair fund GABJ2... 10

# Wallet signing + response
cargo run -p orbitchain-tools -- signing build-donation GBJCHU... 1 5000000 XLM "Supporting education"
cargo run -p orbitchain-tools -- response process '{"requestId":"req_123","xdr":"AAAA...","signer":"GBJCHU...","signedAt":1234567890}'
```

## 🚀 Quick Start: Deploy Your First Contract

This guide walks you through deploying the canonical campaign contract to testnet and invoking a health-check method.

### Prerequisites

1. **Install Soroban CLI**:

   ```bash
   cargo install soroban-cli
   ```

2. **Generate a keypair** (for testnet):

   ```bash
   soroban keys generate test_account --network testnet
   ```

3. **Get testnet XLM** (optional but recommended for testing):
   - Visit [Stellar Testnet Faucet](https://laboratory.stellar.org/#account-creator?network=testnet)

### Step 1: Build the Contract

```bash
# Build WASM contracts (campaign + core + token-bridge + common)
make build-wasm

# Or build everything including CLI tools
make build
```

### Step 2: Configure Environment

```bash
# Copy the example environment file
cp .env.example .env

# Edit .env and set your admin key:
# SOROBAN_ADMIN_KEY=YOUR_PUBLIC_KEY
```

Or generate and configure a new key:

```bash
# Generate a new keypair
soroban keys generate my_admin --network testnet

# Get the public key
soroban keys list

# Add to .env
SOROBAN_ADMIN_KEY=GA7...
```

### Step 3: Deploy to Testnet

> The in-CLI `deploy` command is a stub today. Use the build-in Makefile
> target (or `scripts/deploy.sh`) which is wired into `stellar contract deploy`
> for real network output. Tracking: issue
> [#37](https://github.com/OrbitChainLabs/OrbitChain-Contracts/issues/37).

```bash
# Deploy via the Makefile wrapper (uses scripts/deploy.sh + stellar-cli)
make deploy-testnet
# Or invoke the deploy script directly:
bash scripts/deploy.sh testnet
```

Expected output:

```
ℹ️  Using optimized WASM: target/wasm32v1-none/release/orbitchain_core.wasm
🚀 Deploying to testnet...
   RPC: https://soroban-testnet.stellar.org:443
   WASM: target/wasm32v1-none/release/orbitchain_core.wasm
✅ Contract deployed!
📝 Contract ID: CB7...ABC
💾 Deployment record saved to deployments/testnet.json
✅ Contract ID stored in .orbitchain_contract_id
```

### Step 4: Invoke the ping Method

> The in-CLI `invoke` command is also a stub. Use `stellar contract invoke`
> natively against your deployed contract ID.

```bash
# Read the contract ID that Step 3 wrote out
CONTRACT_ID=$(cat .orbitchain_contract_id)

# Invoke a contract method (replace `version` with any contract method such as `ping`)
stellar contract invoke \
  --id "$CONTRACT_ID" \
  --source test_account \
  --network testnet \
  -- version
```

Expected output:

```
🔄 Invoking method 'version' on network: testnet
📝 Using contract ID: CB7...ABC
✅ Invocation successful!
📤 Result: <contract version bytes>
```

### Step 5: Check Deployment

```bash
# View the deployed contract ID written by scripts/deploy.sh
cat .orbitchain_contract_id

# View the per-network deployment record
cat deployments/testnet.json

# View active network configuration
cargo run -p orbitchain-tools -- network
```

### Using Sandbox (Local Development)

For local testing without testnet:

```bash
# Start local sandbox (requires Docker)
make sandbox-start

# Deploy to sandbox (uses scripts/deploy.sh sandbox)
make deploy-sandbox

# Invoke on sandbox natively
CONTRACT_ID=$(cat .orbitchain_contract_id)
stellar contract invoke \
  --id "$CONTRACT_ID" \
  --source test_account \
  --rpc-url http://localhost:8000/soroban/rpc \
  --network-passphrase "Standalone Network ; February 2017" \
  -- version
```

### Troubleshooting

- **"WASM file not found"**: Run `make build-wasm` to build the contracts first.
- **"Unknown command" or "coming soon"**: You ran an `orbitchain-cli` command
  that is still a stub (`deploy`, `invoke`, `account`). Run
  `cargo run -p orbitchain-tools` with no arguments to see which commands are
  actually implemented, and follow
  [`docs/deployment.md`](docs/deployment.md#known-limitations--cli-status).
- **"No contract ID found"**: Run `make deploy-testnet` first — the
  `scripts/deploy.sh` wrapper writes the ID to `.orbitchain_contract_id`.
- **"Configuration error"**: Run `cargo run -p orbitchain-tools -- config` to
  inspect resolved environment values.
- **"soroban: command not found"**: Install with `cargo install --locked stellar-cli --features opt`.

## 📌 Features

### 🎯 For Donors

- Discover global fundraising campaigns
- Donate in XLM or Stellar assets
- Wallet integration (Freighter, Albedo, Lobstr)
- On-chain transparency: verify all transactions

### 🎯 For Creators

- Create social impact projects
- Accept multi-asset contributions
- Real-time donation tracking
- Withdraw funds directly on-chain

### 🎯 For Admins

- Campaign approval workflow
- User & KYC management
- Analytics dashboard

## 🏗️ Architecture Overview

OrbitChain Blockchain Layer is built with:

- Stellar Testnet / Mainnet support
- Donation verification
- On-chain withdrawal system
- Asset‑agnostic design

## 💰 Fee Estimation System

OrbitChain includes a comprehensive **fee estimation service** that provides accurate transaction fee calculations, surge pricing detection, and multi-currency conversion.

### Features

- **Real-time Fee Estimation**: Fetch current base fees from Stellar Horizon
- **Surge Pricing Detection**: 4-level detection (Normal, Elevated, High, Critical)
- **Multi-Currency Display**: Convert fees to 10+ supported currencies
- **Caching**: 5-minute TTL cache to reduce API calls
- **Fee History Tracking**: 1000+ records for analytics and trend detection

### Quick Start

```rust
use fee::FeeEstimationService;

#[tokio::main]
async fn main() -> Result<()> {
    let service = FeeEstimationService::public_horizon();
    
    // Estimate fee for 2-operation donation
    let fee_info = service.estimate_fee(2).await?;
    println!("Fee: {:.8} XLM", fee_info.total_fee_xlm);
    
    // Check for surge pricing
    if fee_info.is_surge_pricing {
        println!("⚠️ Network surging at {}%!", fee_info.surge_percent as i64);
    }
    
    Ok(())
}
```

### Key Constants

- **Base Fee**: 100 stroops (0.00001 XLM)
- **Conversion**: 1 XLM = 10,000,000 stroops
- **Cache TTL**: 300 seconds (5 minutes)

# 📌 How to Contribute

### 1. Fork the Repository

Click the **“Fork”** button in the top‑right of the GitHub repo and clone your fork:

```bash
git clone https://github.com/YOUR_USERNAME/orbitchain-contract.git
cd orbitchain-contract
```

### 2. Create a Branch

```bash
git checkout -b feature/add-donation-flow
```

### 3. Commit Messages

Use conventional commits:

```bash
feat: add wallet connection modal
fix: resolve donation API error
docs: update project README
refactor: clean up project creation form
```

### 4. Submitting a Pull Request (PR)

Push your branch:

```bash
git push origin feature/add-donation-flow
```

Open a Pull Request from your fork back to the main branch.

## Security Scans

This project uses `cargo-audit` and `cargo-deny` to maintain high security standards and license compliance.

### Local Scans

You can run the security scans locally using the following commands:

- **Check for vulnerabilities**:
  ```bash
  make audit
  ```
- **Check for license and ban policies**:
  ```bash
  make deny
  ```

### Resolving Failures

#### Vulnerabilities (`cargo audit`)

If a vulnerability is found, you should:

1.  **Update dependencies**: Run `cargo update` to see if a newer version of the crate resolves the issue.
2.  **Ignore (Temporary)**: If a fix is not available and you have audited the vulnerability, you can temporarily ignore it by adding it to the `[advisories] -> ignore` list in the deny configuration.

#### License/Ban Policy (`cargo deny`)

If a license or ban policy violation is found:

1.  **Check Licenses**: Ensure all dependencies use approved licenses. If a new license needs to be allowed, update the `allow` list in `deny.toml`.
2.  **Banned Crates**: If a crate is banned, you must find an alternative or justify its use and add it to the `skip` list in `deny.toml`.

### Automated CI

Security scans are automatically run on every push and pull request. CI will fail if any known vulnerabilities or policy violations are detected.

# 📜 License

MIT License — free to use, modify, and distribute.

## Soroban Configuration (networks)

This workspace includes a deterministic, strongly-typed Soroban network configuration system.

Add a network (example CLI stub):

```bash
soroban config network add <name> \
   --rpc-url <url> \
   --network-passphrase "<passphrase>"
```

List networks (profiles in `soroban.toml`):

```bash
soroban config network ls
```

Select a network (this sets the active profile name; loader reads `SOROBAN_NETWORK`):

```bash
soroban config network use <name>
```

Environment variable override behavior

- `SOROBAN_NETWORK` selects a profile (e.g. `testnet`, `mainnet`, `sandbox`).
- `SOROBAN_RPC_URL` and `SOROBAN_NETWORK_PASSPHRASE` override profile values when set.

Verify the resolved network with the included CLI tool:

```bash
cargo run -p orbitchain-tools -- network
```

See `.env.example` for a safe example of environment variables you can copy to `.env`.
