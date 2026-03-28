# Fee Estimation Utility - Implementation Guide

## Overview

The Fee Estimation Utility provides dynamic transaction fee estimation for Stellar transactions. It fetches real-time base fees from Horizon, calculates operation-specific costs, detects surge pricing conditions, and caches results for performance optimization.

## ✅ Implementation Status

All core features are fully implemented and tested:

- ✅ **Fetch base fee** - Real-time Horizon API integration with error handling
- ✅ **Calculate operation cost** - Accurate fee calculation for any operation count
- ✅ **Detect surge pricing** - Multi-level surge detection with trend analysis
- ✅ **Cache results** - TTL-based cache with metadata tracking

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│         FeeEstimationService (Main API)                     │
├─────────────────────────────────────────────────────────────┤
│  ├─ HorizonFeeFetcher      (Fetch base fees)               │
│  ├─ FeeCache               (Cache with TTL)                 │
│  ├─ SurgePricingAnalyzer   (Detect surge pricing)           │
│  ├─ FeeHistory             (Track fee trends)               │
│  ├─ CurrencyConverter       (Multi-currency support)        │
│  └─ FeeCalculator          (Fee math & conversions)         │
└─────────────────────────────────────────────────────────────┘
```

## Core Components

### 1. FeeEstimationService

Main service for fee estimation. Coordinates all components and provides a clean API.

```rust
use fee::FeeEstimationService;

let service = FeeEstimationService::public_horizon();
let fee_info = service.estimate_fee(2).await?;

println!("Fee: {} stroops ({} XLM)", 
    fee_info.fee_stroops(),
    fee_info.fee_xlm()
);
```

**Key Methods:**
- `estimate_fee(operation_count)` - Get fee for operation count
- `estimate_fee_in_currency(count, currency)` - Get fee in specific currency
- `is_surging()` - Check if surge pricing active
- `get_fee_stats()` - Get historical statistics
- `get_surge_info()` - Get user-friendly surge message

### 2. HorizonFeeFetcher

Fetches current base fee from Stellar Horizon API.

```rust
let fetcher = HorizonFeeFetcher::public_horizon()
    .with_timeout(30);

let base_fee = fetcher.fetch_base_fee().await?;
// base_fee = 100 stroops (typical)
```

**Features:**
- Configurable server URL (testnet, public, etc.)
- Timeout handling
- Graceful error recovery
- JSON parsing with validation

### 3. FeeCache

Caches base fees with configurable TTL (default 5 minutes).

```rust
let mut cache = FeeCache::default_ttl(); // 5 min TTL
cache.set(100)?;

if let Some(fee) = cache.get() {
    println!("Cached fee: {} stroops", fee);
}

// Get metadata
if let Some(meta) = cache.metadata() {
    println!("Age: {} seconds", meta.age_seconds);
    println!("Expires in: {} seconds", meta.time_until_expiration);
}
```

**Cache Types:**
- **Valid**: Not expired, used immediately
- **Expired**: Returned only if network unavailable
- **Empty**: Forces fresh fetch from Horizon

### 4. SurgePricingAnalyzer

Detects surge pricing based on fee trends and thresholds.

```rust
let mut analyzer = SurgePricingAnalyzer::new(SurgePricingConfig::default());
let analysis = analyzer.analyze(150)?;

println!("Level: {}", analysis.surge_level.name());
println!("Percent: {}", analysis.surge_percent);
println!("Trend: {:?}", analysis.trend);
println!("Recommendation: {}", analysis.recommendation);
```

**Surge Levels:**
- `Normal` (≤100%) - Network operating normally
- `Elevated` (101%-150%) - Slight congestion
- `High` (151%-300%) - Significant congestion
- `Critical` (>300%) - Severe network congestion

**Fee Trends:**
- `Increasing` (📈) - Fees rising, network getting busier
- `Stable` (➡️) - Fees stable, consistent network load
- `Decreasing` (📉) - Fees falling, network quieting

### 5. FeeHistory

Tracks historical fee data for trend analysis and statistics.

```rust
let mut history = FeeHistory::new(1000); // Max 1000 records

history.add(100, "Horizon API".to_string())?;
history.add(150, "Horizon API".to_string())?;

// Get statistics
if let Some(stats) = history.stats() {
    println!("Min: {} stroops", stats.min_fee);
    println!("Max: {} stroops", stats.max_fee);
    println!("Avg: {:.2} stroops", stats.avg_fee);
    println!("Median: {} stroops", stats.median_fee);
    println!("StdDev: {:.2}", stats.std_dev);
}

// Recent statistics (last 1 hour)
if let Some(recent_stats) = history.recent_stats(3600) {
    println!("1-hour change: {}", history.max_change_percent(3600).unwrap_or(0.0));
}
```

### 6. CurrencyConverter

Converts fees between XLM and 9 other currencies.

```rust
let mut converter = CurrencyConverter::new();

// Set exchange rates
converter.set_rate(Currency::XLM, Currency::USD, 0.35)?;
converter.set_rate(Currency::XLM, Currency::EUR, 0.32)?;

// Convert fee
let usd_fee = converter.convert_xlm_fee(0.00005, Currency::USD)?;
println!("Fee: ${:.6}", usd_fee);
```

**Supported Currencies:**
- XLM (Stellar Lumens) - Base
- USD (US Dollar)
- EUR (Euro)
- GBP (British Pound)
- JPY (Japanese Yen)
- CNY (Chinese Yuan)
- INR (Indian Rupee)
- BRL (Brazilian Real)
- AUD (Australian Dollar)
- CAD (Canadian Dollar)

### 7. FeeCalculator

Utility functions for fee math and conversions.

```rust
use fee::*;

// Calculate total fee
let total_stroops = calculate_fee(100, 2)?;  // 2 operations
assert_eq!(total_stroops, 200);

// Unit conversions
let xlm = stroops_to_xlm(10_000_000);        // 1.0 XLM
let stroops = xlm_to_stroops(0.00001);       // 100 stroops

// Surge percentage
let percent = calculate_surge_percent(200, 100);  // 200%
```

## Usage Examples

### Example 1: Basic Fee Estimation

```rust
use fee::FeeEstimationService;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let service = FeeEstimationService::public_horizon();
    
    // Estimate fee for typical donation (2 operations)
    let fee = service.estimate_fee(2).await?;
    
    println!("Base fee: {} stroops", fee.base_fee_stroops);
    println!("Total fee: {} stroops", fee.total_fee_stroops);
    println!("Total fee: {} XLM", fee.total_fee_xlm);
    println!("Surge pricing: {}", if fee.is_surge_pricing { "Yes" } else { "No" });
    
    Ok(())
}
```

### Example 2: Surge Pricing Detection

```rust
use fee::FeeEstimationService;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let service = FeeEstimationService::public_horizon();
    
    if service.is_surging().await? {
        if let Some(surge_info) = service.get_surge_info().await {
            println!("⚠️ Network surge detected!");
            println!("{}", surge_info);
        }
    } else {
        println!("✅ Network operating normally");
    }
    
    Ok(())
}
```

### Example 3: Multi-Currency Support

```rust
use fee::{FeeEstimationService, Currency};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let service = FeeEstimationService::public_horizon();
    
    // Set exchange rates
    service.set_exchange_rate(Currency::XLM, Currency::USD, 0.35).await?;
    
    let (fee_info, usd_amount) = service
        .estimate_fee_in_currency(2, Currency::USD)
        .await?;
    
    println!("Fee for 2 operations:");
    println!("  {} XLM", fee_info.total_fee_xlm);
    println!("  ${:.6}", usd_amount);
    
    Ok(())
}
```

### Example 4: Historical Analysis

```rust
use fee::FeeEstimationService;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let service = FeeEstimationService::public_horizon();
    
    // Make several fee estimates over time
    for i in 0..5 {
        let _fee = service.estimate_fee(1).await?;
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }
    
    // Get statistics
    if let Some(stats) = service.get_fee_stats().await {
        println!("Fee Statistics:");
        println!("  Min: {} stroops", stats.min_fee);
        println!("  Max: {} stroops", stats.max_fee);
        println!("  Avg: {:.2} stroops", stats.avg_fee);
        println!("  Median: {} stroops", stats.median_fee);
        
        // Last hour analysis
        if let Some(recent) = service.get_recent_fee_stats(3600).await {
            println!("Last hour avg: {:.2} stroops", recent.avg_fee);
        }
    }
    
    Ok(())
}
```

### Example 5: Batch Estimation

```rust
use fee::FeeEstimationService;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let service = FeeEstimationService::public_horizon();
    
    // Estimate fees for multiple operation counts
    let operations = vec![1, 2, 5, 10];
    let fees = service.batch_estimate_fees(&operations).await?;
    
    for (ops, fee) in operations.iter().zip(fees.iter()) {
        println!("{} ops: {} XLM", ops, fee.total_fee_xlm);
    }
    
    Ok(())
}
```

## Configuration

### Custom Horizon Server

```rust
use fee::{FeeEstimationService, FeeServiceConfig};

let config = FeeServiceConfig {
    horizon_url: "https://horizon-testnet.stellar.org".to_string(),
    cache_ttl_secs: 600,          // 10 minutes
    fetch_timeout_secs: 15,
    max_history_records: 500,
    enable_surge_detection: true,
    ..Default::default()
};

let service = FeeEstimationService::new(config);
```

### Custom Fee Config

```rust
use fee::{FeeEstimationService, FeeConfig};

let mut service = FeeEstimationService::public_horizon();

let fee_config = FeeConfig {
    base_fee_stroops: 100,
    min_fee_xlm: 0.00001,
    max_fee_xlm: 100.0,
    surge_threshold_percent: 150.0,
};
```

## Algorithm Details

### Fee Calculation

$$\text{Total Fee (stroops)} = \text{Base Fee} \times \text{Number of Operations}$$

Example:
- Base fee: 100 stroops
- Operations: 2 (payment + call)
- Total: 100 × 2 = 200 stroops
- In XLM: 200 ÷ 10,000,000 = 0.00002 XLM

### Surge Pricing Detection

Compares current base fee to normal baseline (100 stroops):

$$\text{Surge Percent} = \frac{\text{Current Fee}}{\text{Normal Fee}} \times 100\%$$

**Thresholds:**
- 0-100%: Normal
- 101-150%: Elevated
- 151-300%: High
- >300%: Critical

### Fee Trend Analysis

Compares recent fees vs older fees:

$$\text{Trend} = \frac{\text{Recent Avg} - \text{Older Avg}}{\text{Older Avg}} \times 100\%$$

- **Increasing**: >10% change upward
- **Stable**: ±10% change
- **Decreasing**: >10% change downward

## Error Handling

All operations return `FeeResult<T>` which is `Result<T, FeeError>`.

**Common Errors:**

| Error | Cause | Solution |
|-------|-------|----------|
| `HorizonUnavailable` | Network/API down | Retry or use cached value |
| `InvalidFeeValue` | Negative fee or overflow | Check inputs |
| `Timeout` | Horizon slow | Increase timeout or retry |
| `NetworkError` | Connection issue | Check network, retry |
| `ParseError` | Invalid JSON response | Check Horizon API |
| `CurrencyConversionFailed` | Missing exchange rate | Set rate first |
| `InvalidOperationCount` | Zero operations | Must have ≥1 operation |

## Performance Characteristics

| Operation | Time | Memory |
|-----------|------|--------|
| Estimate fee (cached) | <1ms | O(1) |
| Estimate fee (network) | ~200-500ms | O(1) |
| Surge pricing detection | <1ms | O(1) |
| History analysis | <5ms | O(n) where n = records |
| Currency conversion | <1ms | O(1) |

## Cache Behavior

### Default Configuration
- **TTL**: 5 minutes (300 seconds)
- **Fallback**: Uses stale cache if network fails
- **Validation**: Checks expiration before returning

### Cache Lifecycle

```
1. Service initializes → Cache empty
2. First fee request → Fetch from Horizon, cache result
3. Requests within TTL → Return cached value
4. Request after TTL expires → Fetch fresh from Horizon
5. Network unavailable → Return stale cache with warning
```

## Testing

Comprehensive test suite covers:

```
✓ Fee calculations (single & multiple operations)
✓ Surge pricing levels and detection
✓ Cache validity and expiration
✓ Currency conversions
✓ Fee trends and statistics
✓ Error handling
✓ Batch operations
✓ Configuration options
```

Run tests:
```bash
cargo test --package stellaraid-tools fee::
```

## Best Practices

### 1. Always Handle Errors
```rust
match service.estimate_fee(2).await {
    Ok(fee) => println!("Fee: {}", fee.total_fee_xlm),
    Err(e) => eprintln!("Fee estimation failed: {}", e),
}
```

### 2. Cache Exchange Rates
```rust
// Set rates once, reuse
service.set_exchange_rate(Currency::XLM, Currency::USD, rate).await?;

// Now all conversions use cached rate
let (_, usd) = service.estimate_fee_in_currency(2, Currency::USD).await?;
```

### 3. Check for Surge Before High-Value Transactions
```rust
if service.is_surging().await? {
    println!("Consider waiting for lower fees");
    // Check surge_info for recommendation
}
```

### 4. Monitor Fee Trends
```rust
let stats = service.get_fee_stats().await;
if let Some(s) = stats {
    if s.avg_fee > normal_fee {
        // Fee increase detected
    }
}
```

### 5. Use Appropriate Timeouts
- **UI Display**: 2-5 second timeout (fallback to cache)
- **Transaction Building**: 10-15 second timeout
- **Batch Operations**: 30 second timeout

## Future Enhancements

1. **Fee Forecasting**: ML-based fee prediction
2. **Network Health Metrics**: Bandwidth, latency monitoring
3. **Custom Thresholds**: Per-application surge settings
4. **Rate Limiting**: Governor-based request throttling
5. **Persistence**: Store history to database
6. **Fee Alerts**: Notifications on price changes
7. **Analytics**: Dashboard-friendly metrics

## Integration with StellarAid

### Donation Processing
```rust
let service = FeeEstimationService::public_horizon();
let fee = service.estimate_fee(2).await?;  // Payment + contract
if fee.exceeds_threshold(0.01) {
    return Err("Fee exceeds acceptable threshold");
}
// Process donation
```

### Transaction Submission
```rust
if service.is_surging().await? {
    warn!("High fees detected during transaction submission");
}
```

## References

- [Stellar Fee Documentation](https://developers.stellar.org/docs/learn/fees)
- [Horizon API Reference](https://developers.stellar.org/api/introduction/)
- [Soroban Contract Fees](https://soroban.stellar.org/)
- [Fee Statistics Collection](https://github.com/stellar/py-stellar-base)

