//! Real `invoke` command — wraps `stellar contract invoke` with vault-backed
//! key management (Issue #136).
//!
//! Design:
//! - Core functions (`parse_invoke_args`, `validate_contract_id`,
//!   `method_args_from_json`, `build_stellar_args`, `resolve_source_key`) are
//!   pure — no env access, no process spawning — so they unit-test cleanly.
//! - `handle` is the impure orchestrator: it resolves network + signing key,
//!   then shells out to the `stellar` binary (same precedent as
//!   `scripts/deploy.sh`).
//! - The secret key is passed to the child ONLY via the `STELLAR_ACCOUNT` /
//!   `SOROBAN_ACCOUNT` env vars, never on argv (no `ps` leak). It is never
//!   printed; output names only its *source* (e.g. "SOROBAN_ADMIN_SECRET_KEY").

use anyhow::{Context, Result};
use std::process::Command;

use crate::encrypted_vault::EncryptedVault;
use crate::environment_config::{EnvironmentConfig, NetworkConfig};
use crate::key_manager::KeyManager;
use crate::secure_vault::SecureVault;

/// Parsed `orbitchain-cli invoke` options.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvokeOptions {
    pub contract_id: String,
    pub method: String,
    pub args_json: Option<String>,
    pub source_key: Option<String>,
    pub network: Option<String>,
    pub send: Option<String>,
}

/// A resolved signing key plus a printable description of where it came from
/// (the description must never contain the secret itself).
#[derive(Debug, Clone)]
pub struct ResolvedKey {
    pub secret: String,
    pub source: &'static str,
}

/// Parse `invoke` flags (crate has no clap; manual parsing is the convention).
///
/// Required: `--id`, `--method`. Optional: `--args-json`, `--source-key`,
/// `--network`, `--send`. Flags may appear in any order; unknown flags and
/// flags missing a value are errors.
pub fn parse_invoke_args(args: &[String]) -> Result<InvokeOptions> {
    let mut contract_id: Option<String> = None;
    let mut method: Option<String> = None;
    let mut args_json: Option<String> = None;
    let mut source_key: Option<String> = None;
    let mut network: Option<String> = None;
    let mut send: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        let flag = args[i].as_str();
        let target = match flag {
            "--id" => &mut contract_id,
            "--method" => &mut method,
            "--args-json" => &mut args_json,
            "--source-key" => &mut source_key,
            "--network" => &mut network,
            "--send" => &mut send,
            other => anyhow::bail!(
                "Unknown argument '{}'. Run `orbitchain-cli invoke` for usage.",
                other
            ),
        };
        let value = args
            .get(i + 1)
            .ok_or_else(|| anyhow::anyhow!("Flag '{}' requires a value", flag))?;
        *target = Some(value.clone());
        i += 2;
    }

    let contract_id =
        contract_id.ok_or_else(|| anyhow::anyhow!("Missing required flag '--id <CONTRACT_ID>'"))?;
    let method =
        method.ok_or_else(|| anyhow::anyhow!("Missing required flag '--method <METHOD>'"))?;

    if let Some(ref n) = network {
        if n != "testnet" && n != "mainnet" {
            anyhow::bail!("Invalid --network '{}'. Use 'testnet' or 'mainnet'.", n);
        }
    }
    if let Some(ref s) = send {
        if s != "default" && s != "no" && s != "yes" {
            anyhow::bail!("Invalid --send '{}'. Use 'default', 'no', or 'yes'.", s);
        }
    }

    Ok(InvokeOptions {
        contract_id,
        method,
        args_json,
        source_key,
        network,
        send,
    })
}

/// Shape-check a Soroban contract ID: 'C' prefix, 56 chars, base32 charset
/// (A–Z, 2–7). The stellar CLI performs the authoritative checksum validation.
pub fn validate_contract_id(id: &str) -> Result<()> {
    if !id.starts_with('C') {
        anyhow::bail!("Contract ID must start with 'C' (got '{}')", id);
    }
    if id.len() != 56 {
        anyhow::bail!("Contract ID must be 56 characters (got {})", id.len());
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_uppercase() || ('2'..='7').contains(&c))
    {
        anyhow::bail!("Contract ID contains invalid characters (expected base32: A-Z, 2-7)");
    }
    Ok(())
}

/// Validate a method-argument name so it can safely become a `--<name>` flag.
/// Blocks flag injection: must match `[a-zA-Z_][a-zA-Z0-9_]*`.
fn is_valid_arg_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Convert an `--args-json` object into `stellar contract invoke` method args:
/// `{"donor":"G..","amount":"5"}` → `["--donor", "G..", "--amount", "5"]`.
///
/// Conversion rules (acceptance criterion — covers Address, i128, String, Vec):
/// - String → passed as-is (String, Symbol, Address; quote full-range
///   i128/u128 as decimal strings).
/// - Integer within i64/u64 range → decimal.
/// - Float / fractional / integer beyond u64 → error (serde_json degrades
///   >u64 literals to f64, silently losing precision — quote them instead).
/// - Bool → `true` / `false`.
/// - Array / nested object → compact JSON (stellar-cli accepts JSON values).
/// - `null` → `"null"` (Option::None).
pub fn method_args_from_json(args_json: &str) -> Result<Vec<String>> {
    let value: serde_json::Value =
        serde_json::from_str(args_json).context("--args-json is not valid JSON")?;
    let object = value.as_object().ok_or_else(|| {
        anyhow::anyhow!("--args-json must be a JSON object keyed by parameter name, e.g. '{{\"amount\":\"5\"}}'")
    })?;

    let mut out = Vec::with_capacity(object.len() * 2);
    for (name, val) in object {
        if !is_valid_arg_name(name) {
            anyhow::bail!(
                "Invalid argument name '{}' in --args-json (must match [a-zA-Z_][a-zA-Z0-9_]*)",
                name
            );
        }
        let rendered = match val {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Number(n) => {
                if n.is_i64() || n.is_u64() {
                    n.to_string()
                } else {
                    anyhow::bail!(
                        "Argument '{}' is a float or an integer beyond u64 range ({}). \
                         Quote large integers as strings to preserve precision, e.g. \"{}\"",
                        name,
                        n,
                        n
                    );
                }
            }
            serde_json::Value::Bool(b) => b.to_string(),
            serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
                serde_json::to_string(val).context("Failed to serialize nested JSON value")?
            }
            serde_json::Value::Null => "null".to_string(),
        };
        out.push(format!("--{}", name));
        out.push(rendered);
    }
    Ok(out)
}

/// Build the argv passed to the `stellar` binary. The signing key is NOT part
/// of the returned args — it travels via child env vars only (structural
/// guarantee that the secret never reaches argv).
pub fn build_stellar_args(
    opts: &InvokeOptions,
    network: &NetworkConfig,
    method_args: &[String],
) -> Vec<String> {
    let mut args = vec![
        "contract".to_string(),
        "invoke".to_string(),
        "--id".to_string(),
        opts.contract_id.clone(),
        "--rpc-url".to_string(),
        network.rpc_url.clone(),
        "--network-passphrase".to_string(),
        network.network_passphrase.clone(),
    ];
    if let Some(ref send) = opts.send {
        args.push("--send".to_string());
        args.push(send.clone());
    }
    args.push("--".to_string());
    args.push(opts.method.clone());
    args.extend(method_args.iter().cloned());
    args
}

/// Resolve the signing key. Precedence:
/// 1. `--source-key` flag (validated).
/// 2. `SecureVault` env (`SOROBAN_ADMIN_SECRET_KEY` — same var deploy.sh uses).
/// 3. `EncryptedVault` (`admin_secret_key`, then `master_secret_key`);
///    vault decryption errors fall through rather than aborting.
/// 4. Nothing found → actionable error listing all three options.
pub fn resolve_source_key(
    explicit: Option<&str>,
    vault: &SecureVault,
    encrypted: Option<&EncryptedVault>,
) -> Result<ResolvedKey> {
    if let Some(key) = explicit {
        KeyManager::validate_secret_key(key).context("--source-key is not a valid secret key")?;
        return Ok(ResolvedKey {
            secret: key.to_string(),
            source: "--source-key flag",
        });
    }

    if let Some(key) = vault.admin_secret_key.as_deref() {
        if !key.is_empty() {
            KeyManager::validate_secret_key(key)
                .context("SOROBAN_ADMIN_SECRET_KEY is not a valid secret key")?;
            return Ok(ResolvedKey {
                secret: key.to_string(),
                source: "SOROBAN_ADMIN_SECRET_KEY (SecureVault)",
            });
        }
    }

    if let Some(ev) = encrypted {
        for name in ["admin_secret_key", "master_secret_key"] {
            if let Ok(key) = ev.retrieve_secret_key(name) {
                KeyManager::validate_secret_key(&key).with_context(|| {
                    format!(
                        "Encrypted vault key '{}' decrypted to an invalid secret key",
                        name
                    )
                })?;
                return Ok(ResolvedKey {
                    secret: key,
                    source: "encrypted vault",
                });
            }
        }
    }

    anyhow::bail!(
        "No signing key found. Provide one via:\n  \
         1. --source-key <S...>\n  \
         2. SOROBAN_ADMIN_SECRET_KEY in the environment / .env\n  \
         3. Encrypted vault (VAULT_MASTER_PASSWORD + SOROBAN_ADMIN_SECRET_KEY_ENCRYPTED)"
    )
}

fn print_usage() {
    println!("🔄 Contract Invoke");
    println!("━━━━━━━━━━━━━━━━━━");
    println!("Usage: orbitchain-cli invoke --id <CONTRACT_ID> --method <METHOD> [options]");
    println!();
    println!("Options:");
    println!("  --id <C...>          Contract ID (56-char C-address)          [required]");
    println!("  --method <name>      Contract method to invoke               [required]");
    println!("  --args-json <json>   Method args as a JSON object keyed by parameter name");
    println!("  --source-key <S...>  Signing key (default: resolved from vault/env)");
    println!("  --network <name>     testnet | mainnet (default: SOROBAN_NETWORK or testnet)");
    println!("  --send <mode>        default | no | yes ('no' = simulate only, for views)");
    println!();
    println!("Key resolution order:");
    println!("  1. --source-key flag");
    println!("  2. SOROBAN_ADMIN_SECRET_KEY (environment / .env)");
    println!("  3. Encrypted vault (VAULT_MASTER_PASSWORD + SOROBAN_ADMIN_SECRET_KEY_ENCRYPTED)");
    println!();
    println!("Example:");
    println!("  orbitchain-cli invoke --id C... --method donate \\");
    println!("      --args-json '{{\"donor\":\"G...\",\"campaign_id\":1,\"amount\":\"50\"}}'");
    println!();
    println!("💡 Quote full-range i128 amounts as strings to preserve precision.");
}

/// Entry point for `orbitchain-cli invoke` (Issue #136).
///
/// Resolves network + signing key, then executes `stellar contract invoke`.
/// Returns `Err` (non-zero exit) on any failure so the command is scriptable.
pub fn handle(args: &[String]) -> Result<()> {
    if args.is_empty() {
        print_usage();
        return Ok(());
    }

    let opts = parse_invoke_args(args)?;
    validate_contract_id(&opts.contract_id)?;

    let method_args = match opts.args_json.as_deref() {
        Some(json) => method_args_from_json(json)?,
        None => Vec::new(),
    };

    let mut config = EnvironmentConfig::from_env()?;
    if let Some(ref network) = opts.network {
        config.network = network.clone();
    }
    let network = config.get_active_network()?;

    let vault = SecureVault::from_env();
    let encrypted = EncryptedVault::from_env().ok();
    let key = resolve_source_key(opts.source_key.as_deref(), &vault, encrypted.as_ref())?;

    let stellar_args = build_stellar_args(&opts, &network, &method_args);

    println!("🔄 Invoking Contract Method");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Contract: {}", opts.contract_id);
    println!("Method:   {}", opts.method);
    println!("Network:  {} ({})", network.name, network.rpc_url);
    println!("Source:   via {}", key.source);
    println!();

    // Secret goes only into the child's environment, never argv.
    let output = Command::new("stellar")
        .args(&stellar_args)
        .env("STELLAR_ACCOUNT", &key.secret)
        .env("SOROBAN_ACCOUNT", &key.secret)
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                anyhow::anyhow!(
                    "`stellar` CLI not found on PATH. Install it with `make setup` \
                     (or `cargo install --locked stellar-cli`)."
                )
            } else {
                anyhow::anyhow!("Failed to run `stellar`: {}", e)
            }
        })?;

    if output.status.success() {
        println!("✅ Invocation succeeded");
        let stdout = String::from_utf8_lossy(&output.stdout);
        if !stdout.trim().is_empty() {
            println!();
            println!("{}", stdout.trim_end());
        }
        Ok(())
    } else {
        println!("❌ Invocation failed (exit: {})", output.status);
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.trim().is_empty() {
            println!();
            println!("{}", stderr.trim_end());
        }
        anyhow::bail!(
            "stellar contract invoke failed for method '{}' on {}",
            opts.method,
            opts.contract_id
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_CID: &str = "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC";
    const VALID_SECRET: &str = "SADQOBYHA4DQOBYHA4DQOBYHA4DQOBYHA4DQOBYHA4DQOBYHA4DQP54X";

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    fn testnet() -> NetworkConfig {
        NetworkConfig {
            name: "testnet".to_string(),
            rpc_url: "https://soroban-testnet.stellar.org:443".to_string(),
            horizon_url: "https://horizon-testnet.stellar.org".to_string(),
            network_passphrase: "Test SDF Network ; September 2015".to_string(),
        }
    }

    /// Assert `pair` (flag, value) appears adjacently in `args` — args-json
    /// map iteration order is a serde_json implementation detail, so tests
    /// must not assume key order.
    fn assert_has_pair(args: &[String], flag: &str, value: &str) {
        let found = args.windows(2).any(|w| w[0] == flag && w[1] == value);
        assert!(found, "expected pair ({}, {}) in {:?}", flag, value, args);
    }

    // ── parse_invoke_args ──────────────────────────────────────────────

    #[test]
    fn test_parse_all_flags_any_order() {
        let args = s(&[
            "--network",
            "testnet",
            "--args-json",
            "{}",
            "--method",
            "donate",
            "--send",
            "no",
            "--id",
            VALID_CID,
            "--source-key",
            VALID_SECRET,
        ]);
        let opts = parse_invoke_args(&args).unwrap();
        assert_eq!(opts.contract_id, VALID_CID);
        assert_eq!(opts.method, "donate");
        assert_eq!(opts.args_json.as_deref(), Some("{}"));
        assert_eq!(opts.source_key.as_deref(), Some(VALID_SECRET));
        assert_eq!(opts.network.as_deref(), Some("testnet"));
        assert_eq!(opts.send.as_deref(), Some("no"));
    }

    #[test]
    fn test_parse_minimal() {
        let opts = parse_invoke_args(&s(&["--id", VALID_CID, "--method", "version"])).unwrap();
        assert_eq!(opts.method, "version");
        assert!(opts.args_json.is_none());
        assert!(opts.source_key.is_none());
        assert!(opts.network.is_none());
        assert!(opts.send.is_none());
    }

    #[test]
    fn test_parse_missing_id() {
        let err = parse_invoke_args(&s(&["--method", "version"])).unwrap_err();
        assert!(err.to_string().contains("--id"));
    }

    #[test]
    fn test_parse_missing_method() {
        let err = parse_invoke_args(&s(&["--id", VALID_CID])).unwrap_err();
        assert!(err.to_string().contains("--method"));
    }

    #[test]
    fn test_parse_flag_without_value() {
        let err = parse_invoke_args(&s(&["--id", VALID_CID, "--method"])).unwrap_err();
        assert!(err.to_string().contains("requires a value"));
    }

    #[test]
    fn test_parse_unknown_flag() {
        let err = parse_invoke_args(&s(&["--id", VALID_CID, "--method", "x", "--bogus", "1"]))
            .unwrap_err();
        assert!(err.to_string().contains("Unknown argument"));
    }

    #[test]
    fn test_parse_invalid_network() {
        let err = parse_invoke_args(&s(&[
            "--id",
            VALID_CID,
            "--method",
            "x",
            "--network",
            "devnet",
        ]))
        .unwrap_err();
        assert!(err.to_string().contains("--network"));
    }

    #[test]
    fn test_parse_invalid_send() {
        let err = parse_invoke_args(&s(&["--id", VALID_CID, "--method", "x", "--send", "maybe"]))
            .unwrap_err();
        assert!(err.to_string().contains("--send"));
    }

    // ── validate_contract_id ───────────────────────────────────────────

    #[test]
    fn test_valid_contract_id() {
        assert!(validate_contract_id(VALID_CID).is_ok());
    }

    #[test]
    fn test_contract_id_rejects_g_address() {
        let g_addr = "GBJCHUKZMTFSLOMNC2P4TS4VJJBTCYL3SDKW3KSMSGQUZ6EFLXVX77JV";
        assert!(validate_contract_id(g_addr).is_err());
    }

    #[test]
    fn test_contract_id_rejects_wrong_length() {
        assert!(validate_contract_id("CSHORT").is_err());
        assert!(validate_contract_id(&format!("{}A", VALID_CID)).is_err());
    }

    #[test]
    fn test_contract_id_rejects_bad_charset() {
        // '0', '1', '8', '9' and lowercase are not in the base32 alphabet.
        let bad = format!("C018{}", &VALID_CID[4..]);
        assert!(validate_contract_id(&bad).is_err());
        assert!(validate_contract_id(&VALID_CID.to_lowercase()).is_err());
    }

    // ── method_args_from_json ──────────────────────────────────────────

    #[test]
    fn test_json_address_and_string() {
        let args = method_args_from_json(
            r#"{"donor":"GBJCHUKZMTFSLOMNC2P4TS4VJJBTCYL3SDKW3KSMSGQUZ6EFLXVX77JV","memo":"hello"}"#,
        )
        .unwrap();
        assert_has_pair(
            &args,
            "--donor",
            "GBJCHUKZMTFSLOMNC2P4TS4VJJBTCYL3SDKW3KSMSGQUZ6EFLXVX77JV",
        );
        assert_has_pair(&args, "--memo", "hello");
        assert_eq!(args.len(), 4);
    }

    #[test]
    fn test_json_integers() {
        let args = method_args_from_json(r#"{"pos":42,"neg":-7}"#).unwrap();
        assert_has_pair(&args, "--pos", "42");
        assert_has_pair(&args, "--neg", "-7");
    }

    #[test]
    fn test_json_i128_as_string() {
        // Full-range i128 values must be quoted; they pass through verbatim.
        let args = method_args_from_json(r#"{"amount":"170141183460469231731687303715884105727"}"#)
            .unwrap();
        assert_has_pair(&args, "--amount", "170141183460469231731687303715884105727");
    }

    #[test]
    fn test_json_negative_i128_as_string() {
        let args =
            method_args_from_json(r#"{"amount":"-170141183460469231731687303715884105728"}"#)
                .unwrap();
        assert_has_pair(
            &args,
            "--amount",
            "-170141183460469231731687303715884105728",
        );
    }

    #[test]
    fn test_json_huge_unquoted_integer_rejected() {
        // serde_json degrades >u64 literals to f64 — must error, not corrupt.
        let err = method_args_from_json(r#"{"amount":170141183460469231731687303715884105727}"#)
            .unwrap_err();
        assert!(err.to_string().contains("Quote large integers"));
    }

    #[test]
    fn test_json_float_rejected() {
        assert!(method_args_from_json(r#"{"amount":1.5}"#).is_err());
    }

    #[test]
    fn test_json_bool() {
        let args = method_args_from_json(r#"{"flag":true,"other":false}"#).unwrap();
        assert_has_pair(&args, "--flag", "true");
        assert_has_pair(&args, "--other", "false");
    }

    #[test]
    fn test_json_vec() {
        let args = method_args_from_json(r#"{"targets":[100,200,300]}"#).unwrap();
        assert_has_pair(&args, "--targets", "[100,200,300]");
    }

    #[test]
    fn test_json_nested_object() {
        let args = method_args_from_json(r#"{"asset":{"code":"USDC","amount":"5"}}"#).unwrap();
        assert_eq!(args[0], "--asset");
        let nested: serde_json::Value = serde_json::from_str(&args[1]).unwrap();
        assert_eq!(nested["code"], "USDC");
        assert_eq!(nested["amount"], "5");
    }

    #[test]
    fn test_json_null() {
        let args = method_args_from_json(r#"{"memo":null}"#).unwrap();
        assert_has_pair(&args, "--memo", "null");
    }

    #[test]
    fn test_json_empty_object() {
        assert!(method_args_from_json("{}").unwrap().is_empty());
    }

    #[test]
    fn test_json_malformed() {
        assert!(method_args_from_json("{not json").is_err());
    }

    #[test]
    fn test_json_top_level_array_rejected() {
        let err = method_args_from_json(r#"[1,2,3]"#).unwrap_err();
        assert!(err.to_string().contains("JSON object"));
    }

    #[test]
    fn test_json_bad_key_name_rejected() {
        // A key starting with '-' could inject extra CLI flags.
        assert!(method_args_from_json(r#"{"--rpc-url":"evil"}"#).is_err());
        assert!(method_args_from_json(r#"{"1bad":"x"}"#).is_err());
        assert!(method_args_from_json(r#"{"has space":"x"}"#).is_err());
        assert!(method_args_from_json(r#"{"":"x"}"#).is_err());
    }

    // ── build_stellar_args ─────────────────────────────────────────────

    fn opts_with(send: Option<&str>) -> InvokeOptions {
        InvokeOptions {
            contract_id: VALID_CID.to_string(),
            method: "donate".to_string(),
            args_json: None,
            source_key: Some(VALID_SECRET.to_string()),
            network: None,
            send: send.map(|s| s.to_string()),
        }
    }

    #[test]
    fn test_build_stellar_args_full_ordering() {
        let net = testnet();
        let method_args = s(&["--amount", "50"]);
        let args = build_stellar_args(&opts_with(Some("no")), &net, &method_args);
        assert_eq!(
            args,
            s(&[
                "contract",
                "invoke",
                "--id",
                VALID_CID,
                "--rpc-url",
                "https://soroban-testnet.stellar.org:443",
                "--network-passphrase",
                "Test SDF Network ; September 2015",
                "--send",
                "no",
                "--",
                "donate",
                "--amount",
                "50",
            ])
        );
    }

    #[test]
    fn test_build_stellar_args_omits_send_when_unset() {
        let args = build_stellar_args(&opts_with(None), &testnet(), &[]);
        assert!(!args.contains(&"--send".to_string()));
        // "--" separator still present, method last.
        let sep = args.iter().position(|a| a == "--").unwrap();
        assert_eq!(args[sep + 1], "donate");
    }

    #[test]
    fn test_build_stellar_args_never_contains_secret() {
        // Structural guarantee: the secret travels via child env, never argv.
        let net = testnet();
        let method_args = method_args_from_json(r#"{"amount":"50"}"#).unwrap();
        let args = build_stellar_args(&opts_with(Some("yes")), &net, &method_args);
        assert!(args.iter().all(|a| !a.contains(VALID_SECRET)));
        assert!(args.iter().all(|a| !a.starts_with('S') || a.len() < 56));
    }

    // ── resolve_source_key ─────────────────────────────────────────────

    fn empty_vault() -> SecureVault {
        SecureVault {
            admin_secret_key: None,
            admin_public_key: None,
            issuing_secret_key: None,
            issuing_public_key: None,
        }
    }

    #[test]
    fn test_resolve_explicit_key_wins() {
        let vault = SecureVault {
            admin_secret_key: Some(
                "SANOTHERKEYANOTHERKEYANOTHERKEYANOTHERKEYANOTHERKEYANOTH".to_string(),
            ),
            ..empty_vault()
        };
        let key = resolve_source_key(Some(VALID_SECRET), &vault, None).unwrap();
        assert_eq!(key.secret, VALID_SECRET);
        assert!(key.source.contains("--source-key"));
    }

    #[test]
    fn test_resolve_explicit_key_invalid() {
        let err = resolve_source_key(Some("not-a-key"), &empty_vault(), None).unwrap_err();
        assert!(format!("{:#}", err).contains("--source-key"));
    }

    #[test]
    fn test_resolve_falls_back_to_secure_vault() {
        let vault = SecureVault {
            admin_secret_key: Some(VALID_SECRET.to_string()),
            ..empty_vault()
        };
        let key = resolve_source_key(None, &vault, None).unwrap();
        assert_eq!(key.secret, VALID_SECRET);
        assert!(key.source.contains("SOROBAN_ADMIN_SECRET_KEY"));
    }

    #[test]
    fn test_resolve_falls_back_to_encrypted_vault() {
        let mut ev = EncryptedVault::with_password("test-password").unwrap();
        ev.store_secret_key("admin_secret_key", VALID_SECRET)
            .unwrap();
        let key = resolve_source_key(None, &empty_vault(), Some(&ev)).unwrap();
        assert_eq!(key.secret, VALID_SECRET);
        assert_eq!(key.source, "encrypted vault");
    }

    #[test]
    fn test_resolve_encrypted_vault_master_key_fallback() {
        let mut ev = EncryptedVault::with_password("test-password").unwrap();
        ev.store_secret_key("master_secret_key", VALID_SECRET)
            .unwrap();
        let key = resolve_source_key(None, &empty_vault(), Some(&ev)).unwrap();
        assert_eq!(key.secret, VALID_SECRET);
    }

    #[test]
    fn test_resolve_all_empty_lists_three_options() {
        let empty_ev = EncryptedVault::new();
        let err = resolve_source_key(None, &empty_vault(), Some(&empty_ev)).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("--source-key"));
        assert!(msg.contains("SOROBAN_ADMIN_SECRET_KEY"));
        assert!(msg.contains("vault"));
    }

    #[test]
    fn test_resolve_ignores_empty_env_key() {
        let vault = SecureVault {
            admin_secret_key: Some(String::new()),
            ..empty_vault()
        };
        assert!(resolve_source_key(None, &vault, None).is_err());
    }
}
