use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::horizon_client::HorizonClient;
use crate::config::Config;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebugInfo {
    pub network_info: NetworkInfo,
    pub account_info: AccountInfo,
    pub contract_info: Option<ContractInfo>,
    pub recent_transactions: Vec<TransactionDebugInfo>,
    pub fee_stats: FeeStats,
    pub performance_metrics: PerformanceMetrics,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkInfo {
    pub network: String,
    pub horizon_url: String,
    pub rpc_url: String,
    pub network_passphrase: String,
    pub latest_ledger: u32,
    pub horizon_status: String,
    pub response_time_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountInfo {
    pub account_id: String,
    pub sequence_number: String,
    pub balance: HashMap<String, f64>,
    pub num_subentries: u32,
    pub flags: AccountFlags,
    pub signers: Vec<SignerInfo>,
    pub thresholds: Thresholds,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountFlags {
    pub auth_required: bool,
    pub auth_revocable: bool,
    pub auth_immutable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignerInfo {
    pub key: String,
    pub weight: u8,
    pub type_field: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Thresholds {
    pub low_threshold: u8,
    pub med_threshold: u8,
    pub high_threshold: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractInfo {
    pub contract_id: String,
    pub wasm_hash: String,
    pub instance: serde_json::Value,
    pub storage: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionDebugInfo {
    pub hash: String,
    pub ledger: u32,
    pub status: String,
    pub fee_paid: u32,
    pub operation_count: u32,
    pub error_message: Option<String>,
    pub diagnostic_events: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeeStats {
    pub base_fee: u32,
    pub base_reserve: u64,
    pub min_balance: u64,
    pub recommended_fee: u32,
    pub fee_distribution: FeeDistribution,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeeDistribution {
    pub p10: u32,
    pub p25: u32,
    pub p50: u32,
    pub p75: u32,
    pub p90: u32,
    pub p95: u32,
    pub p99: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub rpc_response_time_ms: u64,
    pub horizon_response_time_ms: u64,
    pub transaction_sim_time_ms: u64,
    pub memory_usage_mb: f64,
    pub cpu_usage_percent: f64,
}

pub struct DebugService;

impl DebugService {
    pub async fn collect_debug_info(
        config: &Config,
        account_id: Option<&str>,
        contract_id: Option<&str>,
    ) -> Result<DebugInfo> {
        let horizon_client = HorizonClient::with_config(
            crate::horizon_client::HorizonClientConfig {
                server_url: config.horizon_url.clone(),
                ..Default::default()
            }
        )?;
        
        let network_info = self::collect_network_info(&horizon_client, config).await?;
        let account_info = if let Some(account) = account_id {
            self::collect_account_info(&horizon_client, account).await?
        } else {
            return Err(anyhow::anyhow!("Account ID required for debugging"));
        };
        
        let contract_info = if let Some(contract) = contract_id {
            Some(self::collect_contract_info(&horizon_client, contract).await?)
        } else {
            None
        };
        
        let recent_transactions = self::collect_recent_transactions(&horizon_client, account_id.unwrap()).await?;
        let fee_stats = self::collect_fee_stats(&horizon_client).await?;
        let performance_metrics = self::collect_performance_metrics(&horizon_client).await?;
        
        Ok(DebugInfo {
            network_info,
            account_info,
            contract_info,
            recent_transactions,
            fee_stats,
            performance_metrics,
        })
    }
    
    async fn collect_network_info(
        client: &HorizonClient,
        config: &Config,
    ) -> Result<NetworkInfo> {
        let start = std::time::Instant::now();
        
        let latest_ledger_response = client.get("/ledgers?order=desc&limit=1").await?;
        let ledger_data: serde_json::Value = serde_json::from_str(&latest_ledger_response)?;
        let latest_ledger = ledger_data["embedded"]["records"][0]["sequence"]
            .as_u64()
            .unwrap_or(0) as u32;
        
        let response_time = start.elapsed().as_millis() as u64;
        
        Ok(NetworkInfo {
            network: config.network.clone(),
            horizon_url: config.horizon_url.clone(),
            rpc_url: config.rpc_url.clone(),
            network_passphrase: config.network_passphrase.clone(),
            latest_ledger,
            horizon_status: "Connected".to_string(),
            response_time_ms: response_time,
        })
    }
    
    async fn collect_account_info(
        client: &HorizonClient,
        account_id: &str,
    ) -> Result<AccountInfo> {
        let url = format!("/accounts/{}", account_id);
        let response = client.get(&url).await?;
        let account_data: serde_json::Value = serde_json::from_str(&response)?;
        
        let mut balances = HashMap::new();
        if let Some(balances_array) = account_data["balances"].as_array() {
            for balance in balances_array {
                let asset_code = balance["asset_code"].as_str().unwrap_or("XLM");
                let balance_str = balance["balance"].as_str().unwrap_or("0");
                let balance_val = balance_str.parse::<f64>().unwrap_or(0.0);
                balances.insert(asset_code.to_string(), balance_val);
            }
        }
        
        let mut signers = Vec::new();
        if let Some(signers_array) = account_data["signers"].as_array() {
            for signer in signers_array {
                signers.push(SignerInfo {
                    key: signer["key"].as_str().unwrap_or("").to_string(),
                    weight: signer["weight"].as_u64().unwrap_or(0) as u8,
                    type_field: signer["type"].as_str().unwrap_or("").to_string(),
                });
            }
        }
        
        Ok(AccountInfo {
            account_id: account_id.to_string(),
            sequence_number: account_data["sequence"].as_str().unwrap_or("0").to_string(),
            balance: balances,
            num_subentries: account_data["num_subentries"].as_u64().unwrap_or(0) as u32,
            flags: AccountFlags {
                auth_required: account_data["flags"]["auth_required"].as_bool().unwrap_or(false),
                auth_revocable: account_data["flags"]["auth_revocable"].as_bool().unwrap_or(false),
                auth_immutable: account_data["flags"]["auth_immutable"].as_bool().unwrap_or(false),
            },
            signers,
            thresholds: Thresholds {
                low_threshold: account_data["thresholds"]["low_threshold"].as_u64().unwrap_or(0) as u8,
                med_threshold: account_data["thresholds"]["med_threshold"].as_u64().unwrap_or(0) as u8,
                high_threshold: account_data["thresholds"]["high_threshold"].as_u64().unwrap_or(0) as u8,
            },
        })
    }
    
    async fn collect_contract_info(
        client: &HorizonClient,
        contract_id: &str,
    ) -> Result<ContractInfo> {
        // This would involve querying the contract via RPC
        // For now, return placeholder data
        Ok(ContractInfo {
            contract_id: contract_id.to_string(),
            wasm_hash: "placeholder_hash".to_string(),
            instance: serde_json::Value::Null,
            storage: HashMap::new(),
        })
    }
    
    async fn collect_recent_transactions(
        client: &HorizonClient,
        account_id: &str,
    ) -> Result<Vec<TransactionDebugInfo>> {
        let url = format!("/accounts/{}/transactions?order=desc&limit=10", account_id);
        let response = client.get(&url).await?;
        let transactions_data: serde_json::Value = serde_json::from_str(&response)?;
        
        let mut transactions = Vec::new();
        
        if let Some(records) = transactions_data["embedded"]["records"].as_array() {
            for tx in records {
                transactions.push(TransactionDebugInfo {
                    hash: tx["hash"].as_str().unwrap_or("").to_string(),
                    ledger: tx["ledger"].as_u64().unwrap_or(0) as u32,
                    status: if tx["successful"].as_bool().unwrap_or(false) {
                        "Success".to_string()
                    } else {
                        "Failed".to_string()
                    },
                    fee_paid: tx["fee_paid"].as_u64().unwrap_or(0) as u32,
                    operation_count: tx["operation_count"].as_u64().unwrap_or(0) as u32,
                    error_message: tx["result_meta"]["transactions"][0]["result"]["transaction"]
                        .get("result")
                        .and_then(|r| r.get("error"))
                        .and_then(|e| e.as_str())
                        .map(|s| s.to_string()),
                    diagnostic_events: Vec::new(),
                });
            }
        }
        
        Ok(transactions)
    }
    
    async fn collect_fee_stats(
        client: &HorizonClient,
    ) -> Result<FeeStats> {
        let response = client.get("/fee_stats").await?;
        let fee_data: serde_json::Value = serde_json::from_str(&response)?;
        
        let base_fee = fee_data["last_ledger"]["base_fee"].as_u64().unwrap_or(100) as u32;
        let base_reserve = fee_data["last_ledger"]["base_reserve"].as_u64().unwrap_or(5000000);
        let min_balance = base_reserve * 2; // Simplified calculation
        
        Ok(FeeStats {
            base_fee,
            base_reserve,
            min_balance,
            recommended_fee: base_fee,
            fee_distribution: FeeDistribution {
                p10: fee_data["fee_charges"]["p10"].as_u64().unwrap_or(100) as u32,
                p25: fee_data["fee_charges"]["p25"].as_u64().unwrap_or(100) as u32,
                p50: fee_data["fee_charges"]["p50"].as_u64().unwrap_or(100) as u32,
                p75: fee_data["fee_charges"]["p75"].as_u64().unwrap_or(100) as u32,
                p90: fee_data["fee_charges"]["p90"].as_u64().unwrap_or(100) as u32,
                p95: fee_data["fee_charges"]["p95"].as_u64().unwrap_or(100) as u32,
                p99: fee_data["fee_charges"]["p99"].as_u64().unwrap_or(100) as u32,
            },
        })
    }
    
    async fn collect_performance_metrics(
        client: &HorizonClient,
    ) -> Result<PerformanceMetrics> {
        let start = std::time::Instant::now();
        let _ = client.get("/").await?;
        let horizon_response_time = start.elapsed().as_millis() as u64;
        
        // Simulate other metrics
        Ok(PerformanceMetrics {
            rpc_response_time_ms: horizon_response_time, // Same for now
            horizon_response_time_ms: horizon_response_time,
            transaction_sim_time_ms: 50,
            memory_usage_mb: 128.0,
            cpu_usage_percent: 5.0,
        })
    }
    
    pub fn analyze_transaction_failure(
        debug_info: &DebugInfo,
        tx_hash: &str,
    ) -> Result<String> {
        let tx = debug_info.recent_transactions
            .iter()
            .find(|t| t.hash == tx_hash)
            .ok_or_else(|| anyhow::anyhow!("Transaction not found in recent transactions"))?;
        
        if tx.status == "Success" {
            return Ok("Transaction was successful".to_string());
        }
        
        let mut analysis = Vec::new();
        
        if let Some(error) = &tx.error_message {
            analysis.push(format!("Error: {}", error));
        }
        
        if tx.fee_paid > debug_info.fee_stats.recommended_fee * 10 {
            analysis.push("Fee seems unusually high - possible network congestion".to_string());
        } else if tx.fee_paid < debug_info.fee_stats.recommended_fee {
            analysis.push("Fee might be too low - possible reason for failure".to_string());
        }
        
        if debug_info.performance_metrics.horizon_response_time_ms > 5000 {
            analysis.push("Network response time is high - possible network issues".to_string());
        }
        
        if analysis.is_empty() {
            analysis.push("No obvious issues detected - check transaction details manually".to_string());
        }
        
        Ok(analysis.join("; "))
    }
    
    pub fn export_debug_report(debug_info: &DebugInfo) -> Result<String> {
        let report = serde_json::to_string_pretty(debug_info)?;
        Ok(report)
    }
}
