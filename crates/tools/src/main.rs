use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

mod config;
mod donation_tx_builder;
mod fee;
mod horizon_client;
mod horizon_error;
mod horizon_rate_limit;
mod horizon_retry;
mod transaction_submission;
mod transaction_verification;
mod wallet_signing;

use config::{Config, Network};
use donation_tx_builder::{build_donation_transaction, BuildDonationTxRequest};
use transaction_submission::{
    SubmissionConfig, SubmissionLogger, SubmissionRequest, SubmissionResponse,
    TransactionSubmissionService,
};
use transaction_verification::{TransactionVerificationService, VerificationRequest};
use wallet_signing::{
    CompleteSigningRequest, PrepareSigningRequest, SigningStatus, WalletSigningService, WalletType,
};

const CONTRACT_ID_FILE: &str = ".stellaraid_contract_id";

#[derive(Parser)]
#[command(name = "stellaraid-cli")]
#[command(about = "StellarAid CLI tools for contract deployment and management")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Deploy the core.wasm contract to the specified network
    Deploy {
        /// Network to deploy to (testnet, mainnet, sandbox)
        #[arg(short, long, default_value = "testnet")]
        network: String,
        /// Path to the WASM file (defaults to built contract)
        #[arg(short, long)]
        wasm: Option<String>,
        /// Skip initialization (for contracts that don't require init)
        #[arg(long, default_value = "false")]
        skip_init: bool,
    },
    /// Invoke a method on a deployed contract
    Invoke {
        /// Method to invoke
        #[arg(default_value = "ping")]
        method: String,
        /// Arguments to pass to the method (as JSON)
        #[arg(short, long)]
        args: Option<String>,
        /// Network to use (defaults to stored contract network)
        #[arg(short, long)]
        network: Option<String>,
    },
    /// Get the deployed contract ID
    ContractId {
        /// Show the contract ID for a specific network
        #[arg(short, long)]
        network: Option<String>,
    },
    /// Configuration utilities
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// Print resolved network configuration
    Network,
    /// Build a donation payment transaction XDR for client-side signing
    BuildDonationTx {
        /// Donor public key (source account)
        #[arg(long)]
        donor: String,
        /// Current donor account sequence number
        #[arg(long)]
        donor_sequence: String,
        /// Donation amount (up to 7 decimals, e.g. 10.5)
        #[arg(long)]
        amount: String,
        /// Asset code (XLM for native, or token code like USDC)
        #[arg(long, default_value = "XLM")]
        asset: String,
        /// Asset issuer public key (required for non-XLM assets)
        #[arg(long)]
        issuer: Option<String>,
        /// Project ID used in memo as project_<id>
        #[arg(long)]
        project_id: String,
        /// Destination platform public key (overrides env var)
        #[arg(long)]
        destination: Option<String>,
        /// Transaction timeout in seconds
        #[arg(long, default_value_t = 300)]
        timeout_seconds: i64,
        /// Base fee in stroops per operation
        #[arg(long, default_value_t = 100)]
        base_fee: u32,
        /// Explicit network passphrase (defaults to config value)
        #[arg(long)]
        network_passphrase: Option<String>,
    },
    /// Prepare a wallet-specific transaction signing request
    PrepareWalletSigning {
        /// Wallet name: freighter, albedo, lobstr
        #[arg(long)]
        wallet: String,
        /// Unsigned transaction envelope XDR (base64)
        #[arg(long)]
        xdr: String,
        /// Network passphrase override
        #[arg(long)]
        network_passphrase: Option<String>,
        /// Optional signer public key/address
        #[arg(long)]
        public_key: Option<String>,
        /// Callback URL for popup/deep-link wallets
        #[arg(long)]
        callback_url: Option<String>,
        /// Signing timeout in seconds
        #[arg(long, default_value_t = 180)]
        timeout_seconds: u64,
        /// Log file path for signing attempts/results
        #[arg(long, default_value = ".wallet_signing_attempts.jsonl")]
        log_file: String,
    },
    /// Complete a wallet signing attempt with callback/response payload
    CompleteWalletSigning {
        /// Wallet name: freighter, albedo, lobstr
        #[arg(long)]
        wallet: String,
        /// Attempt ID returned from prepare-wallet-signing
        #[arg(long)]
        attempt_id: String,
        /// Raw wallet response payload (JSON, callback URL, query string, or signed XDR)
        #[arg(long)]
        response: String,
        /// Attempt start UNIX timestamp in seconds
        #[arg(long)]
        started_at_unix: u64,
        /// Signing timeout in seconds
        #[arg(long, default_value_t = 180)]
        timeout_seconds: u64,
        /// Log file path for signing attempts/results
        #[arg(long, default_value = ".wallet_signing_attempts.jsonl")]
        log_file: String,
    },
    /// Submit a signed transaction to the Stellar network
    SubmitTx {
        /// Signed transaction envelope XDR (base64)
        #[arg(long)]
        xdr: String,
        /// Network to submit to (testnet, mainnet)
        #[arg(short, long, default_value = "testnet")]
        network: String,
        /// Maximum submission timeout in seconds
        #[arg(long, default_value_t = 60)]
        timeout_seconds: u64,
        /// Maximum retry attempts for transient failures
        #[arg(long, default_value_t = 3)]
        max_retries: u32,
        /// Disable retry logic
        #[arg(long, default_value_t = false)]
        no_retry: bool,
        /// Log file path for submission attempts
        #[arg(long, default_value = ".transaction_submissions.jsonl")]
        log_file: String,
    },
    /// Check transaction submission status and statistics
    SubmissionStatus {
        /// Show detailed recent submissions
        #[arg(long, default_value_t = false)]
        detailed: bool,
        /// Filter by transaction hash
        #[arg(long)]
        tx_hash: Option<String>,
        /// Log file path
        #[arg(long, default_value = ".transaction_submissions.jsonl")]
        log_file: String,
    },
    /// Verify a transaction on-chain via Horizon
    VerifyTx {
        /// 64-character hex transaction hash to verify
        #[arg(long)]
        hash: String,
        /// Network to query (testnet, mainnet)
        #[arg(short, long, default_value = "testnet")]
        network: String,
        /// Verification timeout in seconds
        #[arg(long, default_value_t = 30)]
        timeout_seconds: u64,
    },
}

#[derive(Subcommand)]
enum ConfigAction {
    Check,
    Init,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Deploy {
            network,
            wasm,
            skip_init,
        } => {
            deploy_contract(&network, wasm.as_deref(), skip_init)?;
        },
        Commands::Invoke {
            method,
            args,
            network,
        } => {
            invoke_contract(&method, args.as_deref(), network.as_deref())?;
        },
        Commands::ContractId { network } => {
            show_contract_id(network.as_deref())?;
        },
        Commands::Config { action } => match action {
            ConfigAction::Check => {
                println!("Checking configuration...");
                match Config::load(None) {
                    Ok(cfg) => {
                        println!("✅ Configuration valid!");
                        println!("  Network: {}", cfg.network);
                        println!("  RPC URL: {}", cfg.rpc_url);
                        println!(
                            "  Admin Key: {}",
                            cfg.admin_key
                                .map_or("Not set".to_string(), |_| "Configured".to_string())
                        );
                    },
                    Err(e) => {
                        eprintln!("❌ Configuration error: {}", e);
                        std::process::exit(1);
                    },
                }
            },
            ConfigAction::Init => {
                println!("Initializing configuration...");
                initialize_config()?;
            },
        },
        Commands::Network => match Config::load(None) {
            Ok(cfg) => {
                println!("Active network: {}", cfg.network);
                println!("RPC URL: {}", cfg.rpc_url);
                println!("Passphrase: {}", cfg.network_passphrase);
                if let Some(key) = cfg.admin_key {
                    println!("Admin Key: {}", key);
                }
            },
            Err(e) => {
                eprintln!("Failed to load config: {}", e);
                std::process::exit(2);
            },
        },
        Commands::BuildDonationTx {
            donor,
            donor_sequence,
            amount,
            asset,
            issuer,
            project_id,
            destination,
            timeout_seconds,
            base_fee,
            network_passphrase,
        } => {
            build_donation_tx(
                &donor,
                &donor_sequence,
                &amount,
                &asset,
                issuer.as_deref(),
                &project_id,
                destination.as_deref(),
                timeout_seconds,
                base_fee,
                network_passphrase.as_deref(),
            )?;
        },
        Commands::PrepareWalletSigning {
            wallet,
            xdr,
            network_passphrase,
            public_key,
            callback_url,
            timeout_seconds,
            log_file,
        } => {
            prepare_wallet_signing(
                &wallet,
                &xdr,
                network_passphrase.as_deref(),
                public_key.as_deref(),
                callback_url.as_deref(),
                timeout_seconds,
                &log_file,
            )?;
        },
        Commands::CompleteWalletSigning {
            wallet,
            attempt_id,
            response,
            started_at_unix,
            timeout_seconds,
            log_file,
        } => {
            complete_wallet_signing(
                &wallet,
                &attempt_id,
                &response,
                started_at_unix,
                timeout_seconds,
                &log_file,
            )?;
        },
        Commands::SubmitTx {
            xdr,
            network,
            timeout_seconds,
            max_retries,
            no_retry,
            log_file,
        } => {
            submit_transaction(
                &xdr,
                &network,
                timeout_seconds,
                max_retries,
                no_retry,
                &log_file,
            )?;
        },
        Commands::SubmissionStatus {
            detailed,
            tx_hash,
            log_file,
        } => {
            show_submission_status(detailed, tx_hash.as_deref(), &log_file)?;
        },
        Commands::VerifyTx {
            hash,
            network,
            timeout_seconds,
        } => {
            verify_transaction(&hash, &network, timeout_seconds)?;
        },
    }

    Ok(())
}

fn status_indicator(status: &SigningStatus) -> &'static str {
    match status {
        SigningStatus::AwaitingUser => "🟡",
        SigningStatus::Signed => "✅",
        SigningStatus::Rejected => "🛑",
        SigningStatus::TimedOut => "⏱️",
        SigningStatus::Invalid => "❌",
    }
}

#[allow(clippy::too_many_arguments)]
fn prepare_wallet_signing(
    wallet: &str,
    xdr: &str,
    network_passphrase_override: Option<&str>,
    public_key: Option<&str>,
    callback_url: Option<&str>,
    timeout_seconds: u64,
    log_file: &str,
) -> Result<()> {
    let wallet = wallet.parse::<WalletType>()?;
    let network_passphrase = if let Some(passphrase) = network_passphrase_override {
        passphrase.to_string()
    } else {
        Config::load(None)
            .map(|cfg| cfg.network_passphrase)
            .context(
                "Failed to resolve network passphrase from config. Pass --network-passphrase or configure soroban.toml",
            )?
    };

    let service = WalletSigningService::new(PathBuf::from(log_file));
    let prepared = service.prepare_signing(PrepareSigningRequest {
        wallet,
        unsigned_xdr: xdr.to_string(),
        network_passphrase,
        public_key: public_key.map(ToString::to_string),
        callback_url: callback_url.map(ToString::to_string),
        timeout_seconds,
    })?;

    println!(
        "{} Wallet signing request prepared",
        status_indicator(&prepared.status)
    );
    println!("  Wallet: {}", prepared.wallet.as_str());
    println!("  Attempt ID: {}", prepared.attempt_id);
    println!("  Status: {:?}", prepared.status);
    println!("  Message: {}", prepared.message);
    println!("  Started At: {}", prepared.created_at_unix);
    println!("  Expires At: {}", prepared.expires_at_unix);
    if let Some(launch_url) = &prepared.launch_url {
        println!("  Launch URL: {}", launch_url);
    }
    println!("  Request Payload: {}", prepared.request_payload);
    println!("  Log File: {}", log_file);

    Ok(())
}

fn complete_wallet_signing(
    wallet: &str,
    attempt_id: &str,
    response: &str,
    started_at_unix: u64,
    timeout_seconds: u64,
    log_file: &str,
) -> Result<()> {
    let wallet = wallet.parse::<WalletType>()?;
    let service = WalletSigningService::new(PathBuf::from(log_file));

    let completion = service.complete_signing(CompleteSigningRequest {
        attempt_id: attempt_id.to_string(),
        wallet,
        wallet_response: response.to_string(),
        started_at_unix,
        timeout_seconds,
    })?;

    println!(
        "{} Wallet signing completion",
        status_indicator(&completion.status)
    );
    println!("  Wallet: {}", completion.wallet.as_str());
    println!("  Attempt ID: {}", completion.attempt_id);
    println!("  Status: {:?}", completion.status);
    println!("  Message: {}", completion.message);

    if let Some(signed_xdr) = completion.signed_xdr {
        println!("  Signed XDR: {}", signed_xdr);
    }
    if let Some(envelope_xdr) = completion.envelope_xdr {
        println!("  Envelope XDR: {}", envelope_xdr);
    }
    println!("  Log File: {}", log_file);

    Ok(())
}

fn resolve_platform_public_key(destination_override: Option<&str>) -> Result<String> {
    if let Some(destination) = destination_override {
        return Ok(destination.to_string());
    }

    env::var("STELLARAID_PLATFORM_PUBLIC_KEY")
        .or_else(|_| env::var("PLATFORM_PUBLIC_KEY"))
        .context(
            "Missing destination account. Pass --destination or set STELLARAID_PLATFORM_PUBLIC_KEY",
        )
}

#[allow(clippy::too_many_arguments)]
fn build_donation_tx(
    donor: &str,
    donor_sequence: &str,
    amount: &str,
    asset: &str,
    issuer: Option<&str>,
    project_id: &str,
    destination_override: Option<&str>,
    timeout_seconds: i64,
    base_fee: u32,
    network_passphrase_override: Option<&str>,
) -> Result<()> {
    let destination = resolve_platform_public_key(destination_override)?;

    let network_passphrase = if let Some(passphrase) = network_passphrase_override {
        passphrase.to_string()
    } else {
        Config::load(None)
            .map(|cfg| cfg.network_passphrase)
            .context(
                "Failed to resolve network passphrase from config. Pass --network-passphrase or configure soroban.toml",
            )?
    };

    let request = BuildDonationTxRequest {
        donor_address: donor.to_string(),
        donor_sequence: donor_sequence.to_string(),
        platform_address: destination,
        donation_amount: amount.to_string(),
        asset_code: asset.to_string(),
        asset_issuer: issuer.map(ToString::to_string),
        project_id: project_id.to_string(),
        network_passphrase,
        timeout_seconds,
        base_fee_stroops: base_fee,
    };

    match build_donation_transaction(request) {
        Ok(result) => {
            println!("✅ Donation transaction built successfully");
            println!("  Destination: {}", result.destination);
            println!("  Asset: {}", result.asset);
            println!("  Amount (stroops): {}", result.amount_stroops);
            println!("  Memo: {}", result.memo);
            println!("  Fee (stroops): {}", result.fee);
            println!("  XDR (ready for signing): {}", result.xdr);
            Ok(())
        },
        Err(err) => {
            eprintln!("❌ Failed to build donation transaction: {}", err);
            std::process::exit(1);
        },
    }
}

/// Get the path to the WASM file
fn get_wasm_path(custom_path: Option<&str>) -> Result<PathBuf> {
    if let Some(path) = custom_path {
        let p = PathBuf::from(path);
        if p.exists() {
            return Ok(p);
        }
        anyhow::bail!("WASM file not found: {}", path);
    }

    // Try default paths
    let default_paths = vec![
        PathBuf::from("target/wasm32-unknown-unknown/debug/stellaraid_core.wasm"),
        PathBuf::from("target/wasm32-unknown-unknown/release/stellaraid_core.wasm"),
        PathBuf::from("contracts/core/target/wasm32-unknown-unknown/debug/stellaraid_core.wasm"),
        PathBuf::from(
            "crates/contracts/core/target/wasm32-unknown-unknown/debug/stellaraid_core.wasm",
        ),
    ];

    for p in &default_paths {
        if p.exists() {
            return Ok(p.clone());
        }
    }

    // Check if we're in the workspace root
    let cwd = env::current_dir()?;
    let wasm_path = cwd.join("target/wasm32-unknown-unknown/debug/stellaraid_core.wasm");
    if wasm_path.exists() {
        return Ok(wasm_path);
    }

    anyhow::bail!("WASM file not found. Build with 'make wasm' or specify with --wasm flag")
}

/// Store the contract ID in a local file
fn store_contract_id(contract_id: &str, network: &str) -> Result<()> {
    let cwd = env::current_dir()?;
    let file_path = cwd.join(CONTRACT_ID_FILE);

    let content = if file_path.exists() {
        let existing: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&file_path)?).unwrap_or(serde_json::json!({}));
        let mut map = serde_json::Map::new();
        if let Some(obj) = existing.as_object() {
            for (k, v) in obj {
                map.insert(k.clone(), v.clone());
            }
        }
        map.insert(network.to_string(), serde_json::json!(contract_id));
        serde_json::Value::Object(map)
    } else {
        serde_json::json!({ network: contract_id })
    };

    fs::write(&file_path, serde_json::to_string_pretty(&content)?)?;
    println!("✅ Contract ID stored in {}", CONTRACT_ID_FILE);
    Ok(())
}

/// Load the contract ID from local file
fn load_contract_id(network: &str) -> Result<String> {
    let cwd = env::current_dir()?;
    let file_path = cwd.join(CONTRACT_ID_FILE);

    if !file_path.exists() {
        anyhow::bail!("No contract ID found. Deploy a contract first with 'deploy' command");
    }

    let content: serde_json::Value = serde_json::from_str(&fs::read_to_string(&file_path)?)?;

    if let Some(id) = content.get(network).and_then(|v| v.as_str()) {
        Ok(id.to_string())
    } else {
        let available = content
            .as_object()
            .map(|obj| obj.keys().cloned().collect::<Vec<_>>().join(", "))
            .unwrap_or_else(|| "none".to_string());
        anyhow::bail!(
            "No contract ID found for network '{}'. Available: {}",
            network,
            available
        );
    }
}

/// Deploy the contract to the specified network
fn deploy_contract(network: &str, wasm_path: Option<&str>, skip_init: bool) -> Result<()> {
    println!("🚀 Deploying to network: {}", network);

    // Load configuration
    env::set_var("SOROBAN_NETWORK", network);
    let config = Config::load(None).context("Failed to load configuration")?;

    // Get WASM path
    let wasm = get_wasm_path(wasm_path)?;
    println!("📦 Using WASM: {}", wasm.display());

    // Build soroban deploy command
    let output = Command::new("soroban")
        .args([
            "contract",
            "deploy",
            "--wasm",
            wasm.to_str().unwrap(),
            "--network",
            network,
            "--rpc-url",
            &config.rpc_url,
            "--network-passphrase",
            &config.network_passphrase,
        ])
        .output()
        .context("Failed to execute soroban CLI")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!("❌ Deployment failed: {}", stderr);
        std::process::exit(1);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let contract_id = stdout.trim();

    println!("✅ Contract deployed successfully!");
    println!("📝 Contract ID: {}", contract_id);

    // Store contract ID
    store_contract_id(contract_id, network)?;

    // Initialize the contract if needed
    if !skip_init {
        if let Some(admin_key) = &config.admin_key {
            println!("🔧 Initializing contract with admin: {}", admin_key);
            let init_output = Command::new("soroban")
                .args([
                    "contract",
                    "invoke",
                    "--network",
                    network,
                    "--rpc-url",
                    &config.rpc_url,
                    "--network-passphrase",
                    &config.network_passphrase,
                    contract_id,
                    "--",
                    "init",
                    "--admin",
                    admin_key,
                ])
                .output()
                .context("Failed to initialize contract")?;

            if init_output.status.success() {
                println!("✅ Contract initialized!");
            } else {
                let stderr = String::from_utf8_lossy(&init_output.stderr);
                eprintln!("⚠️  Initialization warning: {}", stderr);
            }
        } else {
            println!("ℹ️  No admin key configured. Skipping initialization.");
            println!("   Set SOROBAN_ADMIN_KEY environment variable to initialize the contract.");
        }
    }

    Ok(())
}

/// Invoke a method on a deployed contract
fn invoke_contract(method: &str, args: Option<&str>, network_override: Option<&str>) -> Result<()> {
    // Determine which network to use
    let network = if let Some(n) = network_override {
        n.to_string()
    } else {
        // Try to load from stored contract ID
        if let Ok(cfg) = Config::load(None) {
            match cfg.network {
                Network::Testnet => "testnet".to_string(),
                Network::Mainnet => "mainnet".to_string(),
                Network::Sandbox => "sandbox".to_string(),
                Network::Custom(n) => n,
            }
        } else {
            "testnet".to_string()
        }
    };

    println!("🔄 Invoking method '{}' on network: {}", method, network);

    // Load configuration
    env::set_var("SOROBAN_NETWORK", &network);
    let config = Config::load(None).context("Failed to load configuration")?;

    // Load contract ID
    let contract_id = load_contract_id(&network)?;
    println!("📝 Using contract ID: {}", contract_id);

    // Build invoke command
    let mut cmd_args = vec![
        "contract".to_string(),
        "invoke".to_string(),
        "--network".to_string(),
        network.clone(),
        "--rpc-url".to_string(),
        config.rpc_url.clone(),
        "--network-passphrase".to_string(),
        config.network_passphrase.clone(),
        contract_id.clone(),
        "--".to_string(),
        method.to_string(),
    ];

    // Add arguments if provided
    if let Some(arguments) = args {
        // Parse JSON arguments and add them
        let parsed: serde_json::Value =
            serde_json::from_str(arguments).context("Failed to parse arguments as JSON")?;

        if let Some(arr) = parsed.as_array() {
            for val in arr {
                cmd_args.push(val.to_string());
            }
        }
    }

    let output = Command::new("soroban")
        .args(&cmd_args)
        .output()
        .context("Failed to execute soroban CLI")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!("❌ Invocation failed: {}", stderr);
        std::process::exit(1);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("✅ Invocation successful!");
    println!("📤 Result: {}", stdout.trim());

    Ok(())
}

/// Show the contract ID for a network
fn show_contract_id(network_override: Option<&str>) -> Result<()> {
    if let Some(network) = network_override {
        let contract_id = load_contract_id(network)?;
        println!("Contract ID for {}: {}", network, contract_id);
    } else {
        // Show all stored contract IDs
        let cwd = env::current_dir()?;
        let file_path = cwd.join(CONTRACT_ID_FILE);

        if !file_path.exists() {
            println!("No contract IDs stored. Deploy a contract first.");
            return Ok(());
        }

        let content: serde_json::Value = serde_json::from_str(&fs::read_to_string(&file_path)?)?;

        println!("Stored contract IDs:");
        if let Some(obj) = content.as_object() {
            for (network, id) in obj {
                println!("  {}: {}", network, id);
            }
        }
    }
    Ok(())
}

/// Initialize configuration files
fn initialize_config() -> Result<()> {
    let cwd = env::current_dir()?;

    // Check if .env already exists
    let env_path = cwd.join(".env");
    if env_path.exists() {
        println!("⚠️  .env file already exists");
        return Ok(());
    }

    // Create .env file with example values
    let env_content = r#"# StellarAid Configuration
# Network: testnet, mainnet, or sandbox
SOROBAN_NETWORK=testnet

# RPC URL (optional - will use soroban.toml if not set)
# SOROBAN_RPC_URL=https://soroban-testnet.stellar.org

# Network passphrase (optional - will use soroban.toml if not set)
# SOROBAN_NETWORK_PASSPHRASE=Test SDF Network ; September 2015

# Admin key for contract deployment (optional)
# Use 'soroban keys generate' to create a new key
# SOROBAN_ADMIN_KEY=
"#;

    fs::write(&env_path, env_content)?;
    println!("✅ Created .env file");
    println!("ℹ️  Edit .env to configure your network and admin key");

    // Check if contract ID file exists
    let contract_path = cwd.join(CONTRACT_ID_FILE);
    if !contract_path.exists() {
        let empty: serde_json::Value = serde_json::json!({});
        fs::write(&contract_path, serde_json::to_string_pretty(&empty)?)?;
        println!("✅ Created {} file", CONTRACT_ID_FILE);
    }

    Ok(())
}

/// Submit a signed transaction to the Stellar network
fn submit_transaction(
    xdr: &str,
    network: &str,
    timeout_seconds: u64,
    max_retries: u32,
    no_retry: bool,
    log_file: &str,
) -> Result<()> {
    use std::time::Duration;

    println!("🚀 Submitting transaction to {}...", network);

    // Determine Horizon URL based on network
    let horizon_url = match network {
        "testnet" => "https://horizon-testnet.stellar.org",
        "mainnet" => "https://horizon.stellar.org",
        _ => {
            eprintln!("❌ Unknown network: {}. Use 'testnet' or 'mainnet'", network);
            std::process::exit(1);
        }
    };

    // Build configuration
    let config = SubmissionConfig {
        horizon_url: horizon_url.to_string(),
        timeout: Duration::from_secs(timeout_seconds),
        max_retries: if no_retry { 0 } else { max_retries },
        log_path: Some(PathBuf::from(log_file)),
        ..Default::default()
    };

    // Create submission service
    let service = TransactionSubmissionService::with_config(config)
        .map_err(|e| anyhow::anyhow!("Failed to create submission service: {}", e))?;

    // Create submission request
    let request = SubmissionRequest::new(xdr)
        .with_timeout(Duration::from_secs(timeout_seconds))
        .with_retries(if no_retry { 0 } else { max_retries });

    // Run the submission
    let runtime = tokio::runtime::Runtime::new()?;
    let response = runtime.block_on(service.submit(request));

    // Display results
    match response.status {
        super::transaction_submission::SubmissionStatus::Success => {
            println!("✅ Transaction submitted successfully!");
            println!("   Transaction Hash: {}", response.transaction_hash.as_ref().unwrap());
            if let Some(ledger) = response.ledger_sequence {
                println!("   Ledger Sequence: {}", ledger);
            }
            println!("   Attempts: {}", response.attempts);
        }
        super::transaction_submission::SubmissionStatus::Duplicate => {
            println!("⚠️  Transaction already submitted (duplicate)");
            println!("   Transaction Hash: {}", response.transaction_hash.as_ref().unwrap());
        }
        _ => {
            eprintln!("❌ Transaction submission failed");
            eprintln!("   Status: {:?}", response.status);
            if let Some(error) = &response.error_message {
                eprintln!("   Error: {}", error);
            }
            if let Some(code) = &response.error_code {
                eprintln!("   Error Code: {}", code);
            }
            eprintln!("   Attempts: {}", response.attempts);
            std::process::exit(1);
        }
    }

    Ok(())
}

/// Verify a transaction on-chain by querying Horizon
fn verify_transaction(hash: &str, network: &str, timeout_seconds: u64) -> Result<()> {
    use std::time::Duration;
    use transaction_verification::VerificationConfig;

    println!("Verifying transaction on {}...", network);
    println!("  Hash: {}", hash);

    let config = match network {
        "testnet" => VerificationConfig::testnet(),
        "mainnet" => VerificationConfig::mainnet(),
        _ => {
            eprintln!("Unknown network: {}. Use 'testnet' or 'mainnet'", network);
            std::process::exit(1);
        }
    }
    .with_timeout(Duration::from_secs(timeout_seconds));

    let service = TransactionVerificationService::with_config(config)
        .map_err(|e| anyhow::anyhow!("Failed to create verification service: {}", e))?;

    let request = VerificationRequest::new(hash).with_timeout(Duration::from_secs(timeout_seconds));

    let runtime = tokio::runtime::Runtime::new()?;
    let response = runtime
        .block_on(service.verify(request))
        .map_err(|e| anyhow::anyhow!("Verification error: {}", e))?;

    // Display result
    match response.status {
        transaction_verification::VerificationStatus::Confirmed => {
            println!("Transaction confirmed on-chain!");
            if let Some(ledger) = response.ledger_sequence {
                println!("  Ledger: {}", ledger);
            }
            if let Some(time) = &response.ledger_close_time {
                println!("  Ledger Close Time: {}", time);
            }
            if let Some(fee) = &response.fee_charged {
                println!("  Fee Charged (stroops): {}", fee);
            }
            if let Some(contract) = &response.contract_result {
                println!("  Contract Execution: success={}", contract.success);
                if let Some(xdr) = &contract.return_value_xdr {
                    println!("  Return Value XDR: {}", xdr);
                }
                if !contract.events.is_empty() {
                    println!("  Contract Events: {}", contract.events.len());
                }
                if !contract.operation_results.is_empty() {
                    println!("  Operations: {}", contract.operation_results.len());
                }
            }
        }
        transaction_verification::VerificationStatus::Failed => {
            eprintln!("Transaction is on-chain but failed!");
            if let Some(code) = &response.result_code {
                eprintln!("  Result Code: {}", code);
            }
            if let Some(msg) = &response.error_message {
                eprintln!("  Reason: {}", msg);
            }
            std::process::exit(1);
        }
        transaction_verification::VerificationStatus::NotFound => {
            eprintln!("Transaction not found on {}.", network);
            std::process::exit(1);
        }
    }

    Ok(())
}

/// Show transaction submission status and statistics
fn show_submission_status(
    detailed: bool,
    tx_hash_filter: Option<&str>,
    log_file: &str,
) -> Result<()> {
    let logger = SubmissionLogger::new(log_file);

    // Load logs from file
    let logs = logger.load_from_file()?;

    if logs.is_empty() {
        println!("No submission logs found.");
        return Ok(());
    }

    // Filter by transaction hash if specified
    let filtered_logs: Vec<_> = if let Some(hash) = tx_hash_filter {
        logs.into_iter()
            .filter(|log| {
                log.transaction_hash
                    .as_ref()
                    .map(|h| h == hash)
                    .unwrap_or(false)
            })
            .collect()
    } else {
        logs
    };

    if filtered_logs.is_empty() {
        println!("No submissions found matching the criteria.");
        return Ok(());
    }

    // Show statistics
    let stats = logger.get_stats();
    println!("📊 Submission Statistics");
    println!("   Total: {}", stats.total);
    println!("   Successful: {}", stats.successful);
    println!("   Failed: {}", stats.failed);
    println!("   Pending: {}", stats.pending);
    println!("   Duplicates: {}", stats.duplicates);
    println!("   Avg Duration: {}ms", stats.avg_duration_ms);

    // Show detailed logs if requested
    if detailed {
        println!("\n📋 Recent Submissions:");
        for log in filtered_logs.iter().rev().take(10) {
            println!("\n   Request ID: {}", log.request_id);
            println!("   Status: {}", log.status);
            if let Some(hash) = &log.transaction_hash {
                println!("   Transaction Hash: {}", hash);
            }
            if let Some(ledger) = log.ledger_sequence {
                println!("   Ledger: {}", ledger);
            }
            println!("   Timestamp: {}", log.timestamp);
            println!("   Duration: {}ms", log.duration_ms);
            println!("   Attempts: {}", log.attempts);
            if let Some(error) = &log.error_message {
                println!("   Error: {}", error);
            }
            println!("   ---");
        }
    }

    Ok(())
}
