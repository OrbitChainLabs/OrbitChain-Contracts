# 🌟 StellarAid

[![CI](https://github.com/YOUR_USERNAME/stellaraid-contract/workflows/CI/badge.svg)](https://github.com/YOUR_USERNAME/stellaraid-contract/actions)

A blockchain-based crowdfunding platform built on the **Stellar Network** for transparent, borderless, and secure fundraising.

StellarAid enables project creators to raise funds in **XLM** or any Stellar-based asset (USDC, NGNT, custom tokens), while donors can contribute with full on-chain transparency.

## � Workspace Layout

This project uses a Rust Cargo workspace with the following structure:

```
stellarAid-contract/
├── Cargo.toml                 # Workspace configuration
├── crates/
│   ├── contracts/
│   │   └── core/             # Core Soroban smart contract
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

- **`stellaraid-core`**: Main Soroban smart contract implementing the crowdfunding logic
- **`stellaraid-tools`**: CLI utilities for contract deployment, configuration, and management

## 🛠️ Development Setup

### Quick Start (New Contributors)

1. **Clone the repository**

   ```bash
   git clone https://github.com/YOUR_USERNAME/stellaraid-contract.git
   cd stellaraid-contract
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
   cargo build -p stellaraid-core --target wasm32-unknown-unknown
   ```

### Prerequisites

- **Rust stable toolchain** (automatically managed by `rust-toolchain.toml`)
- **wasm32-unknown-unknown target** (auto-installed by toolchain)
- **Soroban CLI** for contract deployment and testing

### Toolchain Configuration

This project uses `rust-toolchain.toml` to ensure consistent development environments:

```toml
[toolchain]
channel = "stable"
targets = ["wasm32-unknown-unknown"]
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
# Build the core contract for WASM
cargo build -p stellaraid-core --target wasm32-unknown-unknown --release

# Build the CLI tools
cargo build -p stellaraid-tools

# Build entire workspace
cargo build --workspace
```

### Testing

```bash
# Run contract tests
cargo test -p stellaraid-core

# Run all tests
cargo test --workspace
```

### CLI Usage

```bash
# Check configuration
cargo run -p stellaraid-tools -- config check

# Initialize configuration (creates .env and contract ID files)
cargo run -p stellaraid-tools -- config init

# Show network configuration
cargo run -p stellaraid-tools -- network

# Deploy contract to testnet
cargo run -p stellaraid-tools -- deploy --network testnet

# Deploy contract to sandbox (local)
cargo run -p stellaraid-tools -- deploy --network sandbox

# Invoke the ping method on deployed contract
cargo run -p stellaraid-tools -- invoke ping

# Invoke with custom network
cargo run -p stellaraid-tools -- invoke ping --network testnet

# Show deployed contract ID
cargo run -p stellaraid-tools -- contract-id
cargo run -p stellaraid-tools -- contract-id --network testnet

# Prepare wallet signing flow (freighter/albedo/lobstr)
cargo run -p stellaraid-tools -- prepare-wallet-signing --wallet freighter --xdr "<UNSIGNED_XDR>"

# Complete wallet signing flow with wallet callback/response payload
cargo run -p stellaraid-tools -- complete-wallet-signing --wallet freighter --attempt-id "<ATTEMPT_ID>" --response "<WALLET_RESPONSE>" --started-at-unix 1700000000
```

## 🚀 Quick Start: Deploy Your First Contract

This guide walks you through deploying the core contract to testnet and invoking the `ping` method.

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
# Build WASM contract
make wasm

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

```bash
# Deploy the contract
cargo run -p stellaraid-tools -- deploy --network testnet
```

Expected output:

```
🚀 Deploying to network: testnet
📦 Using WASM: target/wasm32-unknown-unknown/debug/stellaraid_core.wasm
✅ Contract deployed successfully!
📝 Contract ID: CB7...ABC
✅ Contract ID stored in .stellaraid_contract_id
```

### Step 4: Invoke the ping Method

```bash
# Invoke ping
cargo run -p stellaraid-tools -- invoke ping
```

Expected output:

```
🔄 Invoking method 'ping' on network: testnet
📝 Using contract ID: CB7...ABC
✅ Invocation successful!
📤 Result: 1
```

### Step 5: Check Deployment

```bash
# View all deployed contract IDs
cargo run -p stellaraid-tools -- contract-id

# View network configuration
cargo run -p stellaraid-tools -- network
```

### Using Sandbox (Local Development)

For local testing without testnet:

```bash
# Start local sandbox
soroban sandbox start

# Deploy to sandbox
cargo run -p stellaraid-tools -- deploy --network sandbox

# Invoke on sandbox
cargo run -p stellaraid-tools -- invoke ping --network sandbox
```

### Troubleshooting

- **"WASM file not found"**: Run `make wasm` to build the contract first
- **"No contract ID found"**: Deploy a contract first with `deploy` command
- **"Configuration error"**: Run `cargo run -p stellaraid-tools -- config check` to diagnose
- **"soroban: command not found"**: Install with `cargo install soroban-cli`

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

StellarAid Blockchain Layer is built with:

- Stellar Testnet / Mainnet support
- Donation verification
- On-chain withdrawal system
- Asset‑agnostic design

## 💰 Fee Estimation System

StellarAid includes a comprehensive **fee estimation service** that provides accurate transaction fee calculations, surge pricing detection, and multi-currency conversion.

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

### Documentation

- **[Fee Estimation Guide](./FEE_ESTIMATION.md)**: Comprehensive API reference and architecture
- **[Integration Guide](./DONATION_MODAL_INTEGRATION.md)**: How to integrate with donation modal
- **[Summary](./FEE_SUMMARY.md)**: Implementation details and test coverage

### Key Constants

- **Base Fee**: 100 stroops (0.00001 XLM)
- **Conversion**: 1 XLM = 10,000,000 stroops
- **Cache TTL**: 300 seconds (5 minutes)

# 📌 How to Contribute

### 1. Fork the Repository

Click the **“Fork”** button in the top‑right of the GitHub repo and clone your fork:

```bash
git clone https://github.com/YOUR_USERNAME/stellaraid-contract.git
cd stellaraid-contract
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
2.  **Ignore (Temporary)**: If a fix is not available and you have audited the vulnerability, you can temporarily ignore it by adding it to `deny.toml` under `[advisories] -> ignore`.

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
cargo run -p stellaraid-tools -- network
```

See `.env.example` for a safe example of environment variables you can copy to `.env`.
