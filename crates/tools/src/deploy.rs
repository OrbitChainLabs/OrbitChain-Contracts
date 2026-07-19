//! Real `deploy` command (issue #135) — replaces the stub in `main.rs`.
//!
//! Mirrors `scripts/deploy.sh` step for step: resolve network → validate
//! prerequisites → idempotency check → `stellar contract deploy` → persist
//! the deployment record. The shell script is kept for back-compat; this is
//! the typed path with better error reporting and TTY progress.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::env;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Default WASM artifact produced by `make build-wasm`.
/// (Same legacy `orbitchain_core` binary the shell script ships — see the
/// README note about `orbitchain-campaign` being canonical.)
pub const DEFAULT_WASM_PATH: &str = "target/wasm32v1-none/release/orbitchain_core.wasm";

/// Directory holding one deployment record per network.
pub const DEPLOYMENTS_DIR: &str = "deployments";

/// Plain contract-ID file older tooling reads (back-compat with deploy.sh).
pub const CONTRACT_ID_FILE: &str = ".orbitchain_contract_id";

/// Networks `scripts/deploy.sh` knows about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Network {
    Testnet,
    Mainnet,
    Sandbox,
}

impl Network {
    pub fn parse(name: &str) -> Result<Self> {
        match name {
            "testnet" => Ok(Self::Testnet),
            "mainnet" => Ok(Self::Mainnet),
            "sandbox" => Ok(Self::Sandbox),
            other => bail!(
                "Unknown network: {} (use testnet | sandbox | mainnet)",
                other
            ),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Testnet => "testnet",
            Self::Mainnet => "mainnet",
            Self::Sandbox => "sandbox",
        }
    }

    /// Environment variable that overrides the RPC URL for this network.
    pub fn rpc_env_var(self) -> &'static str {
        match self {
            Self::Testnet => "SOROBAN_TESTNET_RPC_URL",
            Self::Mainnet => "SOROBAN_MAINNET_RPC_URL",
            Self::Sandbox => "SOROBAN_SANDBOX_RPC_URL",
        }
    }

    /// Default RPC URL — identical to the fallbacks in `scripts/deploy.sh`.
    pub fn default_rpc_url(self) -> &'static str {
        match self {
            Self::Testnet => "https://soroban-testnet.stellar.org:443",
            Self::Mainnet => "https://soroban-rpc.mainnet.stellar.gateway.fm",
            Self::Sandbox => "http://localhost:8000/soroban/rpc",
        }
    }

    /// Environment variable that overrides the network passphrase.
    pub fn passphrase_env_var(self) -> &'static str {
        match self {
            Self::Testnet => "SOROBAN_TESTNET_PASSPHRASE",
            Self::Mainnet => "SOROBAN_MAINNET_PASSPHRASE",
            Self::Sandbox => "SOROBAN_SANDBOX_PASSPHRASE",
        }
    }

    /// Default passphrase — identical to the fallbacks in `scripts/deploy.sh`.
    pub fn default_passphrase(self) -> &'static str {
        match self {
            Self::Testnet => "Test SDF Network ; September 2015",
            Self::Mainnet => "Public Global Stellar Network ; September 2015",
            Self::Sandbox => "Standalone Network ; February 2017",
        }
    }

    pub fn rpc_url(self) -> String {
        env::var(self.rpc_env_var()).unwrap_or_else(|_| self.default_rpc_url().to_string())
    }

    pub fn passphrase(self) -> String {
        env::var(self.passphrase_env_var())
            .unwrap_or_else(|_| self.default_passphrase().to_string())
    }
}

/// Typed arguments for `orbitchain-cli deploy`.
///
/// Accepts the network either positionally (deploy.sh parity:
/// `deploy testnet`) or as `--network testnet`, plus `--wasm <path>` and
/// `--force` to override the idempotency check.
#[derive(Debug, Default, PartialEq)]
pub struct DeployArgs {
    pub network: Option<String>,
    pub wasm: Option<PathBuf>,
    pub force: bool,
    pub help: bool,
}

pub fn parse_args(args: &[String]) -> Result<DeployArgs> {
    let mut parsed = DeployArgs::default();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--help" | "-h" => parsed.help = true,
            "--force" => parsed.force = true,
            "--network" => {
                let value = iter
                    .next()
                    .context("--network requires a value (testnet | sandbox | mainnet)")?;
                parsed.network = Some(value.clone());
            }
            "--wasm" => {
                let value = iter.next().context("--wasm requires a path")?;
                parsed.wasm = Some(PathBuf::from(value));
            }
            flag if flag.starts_with('-') => {
                bail!("Unknown flag: {} (see 'deploy --help')", flag)
            }
            positional => {
                if parsed.network.is_some() {
                    bail!(
                        "Unexpected argument: {} (network already given)",
                        positional
                    );
                }
                parsed.network = Some(positional.to_string());
            }
        }
    }
    Ok(parsed)
}

/// One deployment record, persisted as `deployments/<network>.json` —
/// same shape `scripts/deploy.sh` writes.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct DeploymentRecord {
    pub network: String,
    pub contract_id: String,
    pub rpc_url: String,
    pub deployed_at: String,
    pub wasm: String,
}

/// Contract ID from an existing record, if one is present and non-empty.
/// Malformed or missing files read as "not deployed" — same tolerance as
/// the shell script's `|| true`.
pub fn existing_contract_id(record_path: &Path) -> Option<String> {
    let raw = std::fs::read_to_string(record_path).ok()?;
    let record: DeploymentRecord = serde_json::from_str(&raw).ok()?;
    if record.contract_id.is_empty() {
        None
    } else {
        Some(record.contract_id)
    }
}

/// Argument vector for `stellar contract deploy` (program name excluded).
pub fn stellar_deploy_args(
    wasm: &Path,
    source_secret: &str,
    rpc_url: &str,
    passphrase: &str,
) -> Vec<String> {
    vec![
        "contract".to_string(),
        "deploy".to_string(),
        "--wasm".to_string(),
        wasm.display().to_string(),
        "--source".to_string(),
        source_secret.to_string(),
        "--rpc-url".to_string(),
        rpc_url.to_string(),
        "--network-passphrase".to_string(),
        passphrase.to_string(),
    ]
}

/// The contract ID is the last non-empty stdout line (the stellar CLI may
/// print progress lines above it).
pub fn contract_id_from_stdout(stdout: &str) -> Option<String> {
    stdout
        .lines()
        .map(str::trim)
        .rfind(|line| !line.is_empty())
        .map(str::to_string)
}

fn print_usage() {
    println!("Usage: orbitchain-cli deploy [network] [--network <net>] [--wasm <path>] [--force]");
    println!();
    println!("Deploy the OrbitChain core contract (Rust mirror of scripts/deploy.sh).");
    println!();
    println!("Arguments:");
    println!(
        "  network            testnet | sandbox | mainnet (default: $SOROBAN_NETWORK or testnet)"
    );
    println!(
        "  --wasm <path>      WASM artifact (default: {})",
        DEFAULT_WASM_PATH
    );
    println!("  --force            Re-deploy even if deployments/<network>.json exists");
    println!();
    println!("Environment (loaded from .env if present):");
    println!("  SOROBAN_ADMIN_SECRET_KEY   required — funded deployer secret key");
    println!("  SOROBAN_<NET>_RPC_URL      optional RPC override per network");
    println!("  SOROBAN_<NET>_PASSPHRASE   optional passphrase override per network");
}

/// Entry point for `orbitchain-cli deploy`.
pub fn run(args: &[String]) -> Result<()> {
    dotenv::dotenv().ok();

    let parsed = parse_args(args)?;
    if parsed.help {
        print_usage();
        return Ok(());
    }

    let network_name = parsed
        .network
        .or_else(|| env::var("SOROBAN_NETWORK").ok())
        .unwrap_or_else(|| "testnet".to_string());
    let network = Network::parse(&network_name)?;
    let rpc_url = network.rpc_url();
    let passphrase = network.passphrase();
    let wasm_path = parsed
        .wasm
        .unwrap_or_else(|| PathBuf::from(DEFAULT_WASM_PATH));

    println!("🚀 Deploying OrbitChain to {}", network.name());
    println!("   RPC:  {}", rpc_url);
    println!("   WASM: {}", wasm_path.display());
    println!();

    // [1/4] Prerequisites — fail early with actionable messages.
    println!("[1/4] Validating prerequisites…");
    if !wasm_path.exists() {
        bail!(
            "WASM not found at {} — run 'make build-wasm' first (or pass --wasm <path>)",
            wasm_path.display()
        );
    }
    let secret = env::var("SOROBAN_ADMIN_SECRET_KEY")
        .ok()
        .filter(|s| !s.is_empty());
    let Some(secret) = secret else {
        bail!("SOROBAN_ADMIN_SECRET_KEY is not set. Add it to .env or export it.");
    };

    // [2/4] Idempotency — same behaviour as deploy.sh, plus --force to override.
    println!("[2/4] Checking for an existing deployment…");
    let record_path = Path::new(DEPLOYMENTS_DIR).join(format!("{}.json", network.name()));
    if let Some(existing) = existing_contract_id(&record_path) {
        if parsed.force {
            println!(
                "   ⚠️  Existing deployment {} — re-deploying (--force).",
                existing
            );
        } else {
            println!(
                "ℹ️  Contract already deployed on {}: {}",
                network.name(),
                existing
            );
            println!(
                "   Pass --force (or delete {}) to re-deploy.",
                record_path.display()
            );
            return Ok(());
        }
    }

    // [3/4] Deploy via the stellar CLI — exactly what the shell script runs.
    println!("[3/4] Running stellar contract deploy…");
    let output = Command::new("stellar")
        .args(stellar_deploy_args(
            &wasm_path,
            &secret,
            &rpc_url,
            &passphrase,
        ))
        .output()
        .map_err(|e| {
            if e.kind() == ErrorKind::NotFound {
                anyhow::anyhow!(
                    "The 'stellar' CLI was not found on PATH. Install it with \
                     'cargo install --locked stellar-cli' (or 'brew install stellar-cli')."
                )
            } else {
                anyhow::anyhow!("Failed to launch the stellar CLI: {}", e)
            }
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "stellar contract deploy failed ({}):\n{}",
            output.status,
            stderr.trim()
        );
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let Some(contract_id) = contract_id_from_stdout(&stdout) else {
        bail!("stellar contract deploy succeeded but printed no contract ID");
    };
    if !(contract_id.len() == 56 && contract_id.starts_with('C')) {
        println!(
            "   ⚠️  Output does not look like a contract ID (expected C…, 56 chars): {}",
            contract_id
        );
    }
    println!("✅ Contract deployed!");
    println!("📝 Contract ID: {}", contract_id);

    // [4/4] Persist the record + the back-compat plain-ID file.
    println!("[4/4] Saving deployment record…");
    let record = DeploymentRecord {
        network: network.name().to_string(),
        contract_id: contract_id.clone(),
        rpc_url,
        deployed_at: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        wasm: wasm_path.display().to_string(),
    };
    std::fs::create_dir_all(DEPLOYMENTS_DIR)
        .with_context(|| format!("Failed to create {}", DEPLOYMENTS_DIR))?;
    let json = serde_json::to_string_pretty(&record).context("Failed to serialize record")?;
    std::fs::write(&record_path, json)
        .with_context(|| format!("Failed to write {}", record_path.display()))?;
    std::fs::write(CONTRACT_ID_FILE, format!("{}\n", contract_id))
        .with_context(|| format!("Failed to write {}", CONTRACT_ID_FILE))?;
    println!("💾 Deployment record saved to {}", record_path.display());
    println!("✅ Contract ID stored in {}", CONTRACT_ID_FILE);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parses_all_three_networks() {
        assert_eq!(Network::parse("testnet").unwrap(), Network::Testnet);
        assert_eq!(Network::parse("mainnet").unwrap(), Network::Mainnet);
        assert_eq!(Network::parse("sandbox").unwrap(), Network::Sandbox);
    }

    #[test]
    fn rejects_unknown_network() {
        let err = Network::parse("futurenet").unwrap_err().to_string();
        assert!(
            err.contains("futurenet"),
            "error names the bad input: {}",
            err
        );
    }

    #[test]
    fn defaults_match_deploy_sh() {
        assert_eq!(
            Network::Testnet.default_rpc_url(),
            "https://soroban-testnet.stellar.org:443"
        );
        assert_eq!(
            Network::Mainnet.default_rpc_url(),
            "https://soroban-rpc.mainnet.stellar.gateway.fm"
        );
        assert_eq!(
            Network::Sandbox.default_rpc_url(),
            "http://localhost:8000/soroban/rpc"
        );
        assert_eq!(
            Network::Testnet.default_passphrase(),
            "Test SDF Network ; September 2015"
        );
        assert_eq!(
            Network::Mainnet.default_passphrase(),
            "Public Global Stellar Network ; September 2015"
        );
        assert_eq!(
            Network::Sandbox.default_passphrase(),
            "Standalone Network ; February 2017"
        );
    }

    #[test]
    fn parse_args_defaults() {
        let parsed = parse_args(&[]).unwrap();
        assert_eq!(parsed, DeployArgs::default());
    }

    #[test]
    fn parse_args_positional_network() {
        let parsed = parse_args(&strings(&["testnet"])).unwrap();
        assert_eq!(parsed.network.as_deref(), Some("testnet"));
    }

    #[test]
    fn parse_args_network_flag() {
        let parsed = parse_args(&strings(&["--network", "sandbox"])).unwrap();
        assert_eq!(parsed.network.as_deref(), Some("sandbox"));
    }

    #[test]
    fn parse_args_wasm_and_force() {
        let parsed = parse_args(&strings(&["--wasm", "a/b.wasm", "--force"])).unwrap();
        assert_eq!(parsed.wasm.as_deref(), Some(Path::new("a/b.wasm")));
        assert!(parsed.force);
    }

    #[test]
    fn parse_args_help() {
        assert!(parse_args(&strings(&["--help"])).unwrap().help);
        assert!(parse_args(&strings(&["-h"])).unwrap().help);
    }

    #[test]
    fn parse_args_rejects_unknown_flag() {
        assert!(parse_args(&strings(&["--frobnicate"])).is_err());
    }

    #[test]
    fn parse_args_rejects_missing_values_and_double_network() {
        assert!(parse_args(&strings(&["--network"])).is_err());
        assert!(parse_args(&strings(&["--wasm"])).is_err());
        assert!(parse_args(&strings(&["testnet", "mainnet"])).is_err());
    }

    #[test]
    fn record_round_trips_through_json() {
        let record = DeploymentRecord {
            network: "testnet".into(),
            contract_id: "C".repeat(56),
            rpc_url: "https://example".into(),
            deployed_at: "2026-01-01T00:00:00Z".into(),
            wasm: DEFAULT_WASM_PATH.into(),
        };
        let json = serde_json::to_string(&record).unwrap();
        let back: DeploymentRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(back, record);
    }

    #[test]
    fn existing_contract_id_tolerates_missing_and_malformed() {
        let dir = std::env::temp_dir().join("orbitchain-deploy-test");
        std::fs::create_dir_all(&dir).unwrap();

        let missing = dir.join("missing.json");
        let _ = std::fs::remove_file(&missing);
        assert_eq!(existing_contract_id(&missing), None);

        let malformed = dir.join("malformed.json");
        std::fs::write(&malformed, "not json").unwrap();
        assert_eq!(existing_contract_id(&malformed), None);

        let empty_id = dir.join("empty.json");
        std::fs::write(
            &empty_id,
            r#"{"network":"testnet","contract_id":"","rpc_url":"","deployed_at":"","wasm":""}"#,
        )
        .unwrap();
        assert_eq!(existing_contract_id(&empty_id), None);

        let good = dir.join("good.json");
        std::fs::write(
            &good,
            r#"{"network":"testnet","contract_id":"CABC","rpc_url":"","deployed_at":"","wasm":""}"#,
        )
        .unwrap();
        assert_eq!(existing_contract_id(&good), Some("CABC".to_string()));
    }

    #[test]
    fn stellar_args_are_ordered_and_complete() {
        let args = stellar_deploy_args(
            Path::new("t.wasm"),
            "SSECRET",
            "https://rpc.example",
            "Pass ; Phrase",
        );
        assert_eq!(
            args,
            strings(&[
                "contract",
                "deploy",
                "--wasm",
                "t.wasm",
                "--source",
                "SSECRET",
                "--rpc-url",
                "https://rpc.example",
                "--network-passphrase",
                "Pass ; Phrase",
            ])
        );
    }

    #[test]
    fn contract_id_is_last_non_empty_stdout_line() {
        assert_eq!(
            contract_id_from_stdout("progress...\nmore\nCABC123\n\n"),
            Some("CABC123".to_string())
        );
        assert_eq!(contract_id_from_stdout("\n \n"), None);
    }
}
