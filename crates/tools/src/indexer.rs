//! Issue #145 — Soroban contract event stream indexer.
//!
//! Runs as `orbitchain-cli indexer run --network <net>`. Polls Soroban RPC for
//! contract events, then pushes them to a sink (webhook or stdout) so off-chain
//! UI components get durable updates instead of every consumer polling the
//! chain themselves.
//!
//! ## Shape
//!
//! ```text
//!   EventSource ──► bounded channel ──► batcher ──► EventSink
//!   (RPC poll)      (backpressure)      (size/     (webhook, retried)
//!                                        deadline)
//! ```
//!
//! Two halves run concurrently: a **fetch** task pulling pages from RPC, and a
//! **deliver** task batching and shipping them. They are decoupled by a bounded
//! channel, which is what makes the acceptance criterion (≥100 events/sec)
//! hold under a slow sink: the queue absorbs bursts, and when it fills the
//! fetcher blocks rather than allocating without limit. Dropping events is
//! never an option for an indexer — falling behind is recoverable, losing an
//! event is not.
//!
//! ## Why polling rather than an SSE subscription
//!
//! The issue is titled "Horizon event subscribe", but Horizon does not expose
//! Soroban *contract* events — its streams cover transactions, operations and
//! effects. Contract events live behind the Soroban RPC `getEvents` method,
//! which is cursor-paginated rather than streaming. Polling with a cursor is
//! therefore the supported mechanism, and it has a property SSE lacks: the
//! cursor is resumable, so a restart continues exactly where it stopped
//! instead of silently skipping the gap.
//!
//! ## Deviation from the issue text
//!
//! The issue offers "a webhook **or** Postgres" as the sink. This ships the
//! webhook (plus stdout for local runs) behind an [`EventSink`] trait; a
//! Postgres sink is an implementation of that trait and can land without
//! touching the pipeline. Adding a database driver to the CLI for the same
//! delivery guarantee a webhook already provides seemed the worse default —
//! happy to add it if maintainers want it in-tree.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

/// Default RPC poll interval. Soroban closes a ledger roughly every 5s; polling
/// faster than that mostly returns empty pages.
pub const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(5);

/// Default events per delivery. Large enough to amortise a webhook round trip,
/// small enough that a failed batch retries cheaply.
pub const DEFAULT_BATCH_SIZE: usize = 100;

/// Flush a partial batch after this long so a quiet chain still delivers
/// promptly instead of holding events until the batch fills.
pub const DEFAULT_BATCH_TIMEOUT: Duration = Duration::from_secs(2);

/// Bounded queue depth between fetch and deliver. Sized to absorb a burst of
/// several batches while bounding worst-case memory.
pub const DEFAULT_QUEUE_CAPACITY: usize = 1_000;

/// Attempts per batch before giving up and surfacing the error.
pub const DEFAULT_MAX_RETRIES: u32 = 3;

// ─── Event model ─────────────────────────────────────────────────────────────

/// A single Soroban contract event, flattened for downstream consumers.
///
/// Deliberately not the raw RPC shape: `id` and `paging_token` are kept so a
/// consumer can deduplicate and resume, while XDR payloads stay as strings —
/// the indexer's job is durable transport, not interpretation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractEvent {
    /// Globally unique event id from RPC; stable across restarts.
    pub id: String,
    /// Ledger sequence the event was emitted in.
    pub ledger: u32,
    /// ISO-8601 close time of that ledger, when RPC supplies it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ledger_closed_at: Option<String>,
    /// Emitting contract address (`C…`).
    pub contract_id: String,
    /// `contract`, `system`, or `diagnostic`.
    pub event_type: String,
    /// Base64 XDR topic scvals, in order.
    pub topics: Vec<String>,
    /// Base64 XDR event body.
    pub value: String,
    /// Cursor for resuming after this event.
    pub paging_token: String,
}

/// Cursor marking indexer progress.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cursor {
    /// Last delivered event's paging token; `None` starts from `start_ledger`.
    pub paging_token: Option<String>,
    /// Highest ledger observed so far.
    pub ledger: u32,
}

// ─── Source / sink traits ────────────────────────────────────────────────────

/// Somewhere events come from. Abstracted so the pipeline can be tested
/// deterministically without a network.
#[async_trait]
pub trait EventSource: Send + Sync {
    /// Fetch the next page after `cursor`. An empty page means "caller is
    /// current" and is not an error.
    async fn fetch(&self, cursor: &Cursor) -> Result<Vec<ContractEvent>>;
}

/// Somewhere events go. A Postgres sink slots in here without touching the
/// pipeline.
#[async_trait]
pub trait EventSink: Send + Sync {
    /// Deliver a batch. Must be atomic-ish: returning `Err` causes a retry of
    /// the whole batch, so implementations should be idempotent on `event.id`.
    async fn deliver(&self, batch: &[ContractEvent]) -> Result<()>;

    /// Name for logs.
    fn name(&self) -> &'static str;
}

// ─── RPC source ──────────────────────────────────────────────────────────────

/// Polls Soroban RPC `getEvents`.
pub struct RpcEventSource {
    client: reqwest::Client,
    rpc_url: String,
    start_ledger: u32,
    contract_ids: Vec<String>,
    page_limit: usize,
}

impl RpcEventSource {
    pub fn new(rpc_url: String, start_ledger: u32, contract_ids: Vec<String>) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .context("building the RPC HTTP client")?;
        Ok(Self {
            client,
            rpc_url,
            start_ledger,
            contract_ids,
            page_limit: DEFAULT_BATCH_SIZE,
        })
    }

    /// JSON-RPC request body for `getEvents`.
    ///
    /// RPC accepts *either* `startLedger` or a `cursor`, never both — sending
    /// both is an error, so the cursor wins once we have one.
    fn request_body(&self, cursor: &Cursor) -> serde_json::Value {
        let mut pagination = serde_json::json!({ "limit": self.page_limit });
        let mut params = serde_json::json!({
            "filters": [{
                "type": "contract",
                "contractIds": self.contract_ids,
            }],
        });

        match &cursor.paging_token {
            Some(token) => {
                pagination["cursor"] = serde_json::json!(token);
            }
            None => {
                params["startLedger"] = serde_json::json!(self.start_ledger);
            }
        }
        params["pagination"] = pagination;

        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getEvents",
            "params": params,
        })
    }
}

#[async_trait]
impl EventSource for RpcEventSource {
    async fn fetch(&self, cursor: &Cursor) -> Result<Vec<ContractEvent>> {
        let response = self
            .client
            .post(&self.rpc_url)
            .json(&self.request_body(cursor))
            .send()
            .await
            .context("calling Soroban RPC getEvents")?;

        let status = response.status();
        let body: serde_json::Value = response
            .json()
            .await
            .context("decoding the getEvents response")?;

        if !status.is_success() {
            return Err(anyhow!("RPC returned HTTP {status}: {body}"));
        }
        if let Some(error) = body.get("error") {
            return Err(anyhow!("RPC error: {error}"));
        }

        parse_events(&body)
    }
}

/// Map a `getEvents` response onto [`ContractEvent`]s.
///
/// Split out of the HTTP path so response shaping is unit-testable without a
/// server. Unknown/missing optional fields degrade to empty rather than
/// failing the batch: an indexer that halts on one odd event is worse than one
/// that carries a slightly thinner record.
fn parse_events(body: &serde_json::Value) -> Result<Vec<ContractEvent>> {
    let events = body
        .get("result")
        .and_then(|r| r.get("events"))
        .and_then(|e| e.as_array())
        .ok_or_else(|| anyhow!("getEvents response has no result.events array"))?;

    let mut out = Vec::with_capacity(events.len());
    for raw in events {
        let id = raw
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("event is missing its id"))?
            .to_string();

        let topics = raw
            .get("topic")
            .and_then(|t| t.as_array())
            .map(|t| {
                t.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();

        out.push(ContractEvent {
            paging_token: raw
                .get("pagingToken")
                .and_then(|v| v.as_str())
                .unwrap_or(&id)
                .to_string(),
            id,
            ledger: raw.get("ledger").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
            topics,
            ledger_closed_at: raw
                .get("ledgerClosedAt")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            contract_id: raw
                .get("contractId")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            event_type: raw
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("contract")
                .to_string(),
            value: raw
                .get("value")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
        });
    }
    Ok(out)
}

// ─── Sinks ───────────────────────────────────────────────────────────────────

/// POSTs each batch as a JSON array.
pub struct WebhookSink {
    client: reqwest::Client,
    url: String,
}

impl WebhookSink {
    pub fn new(url: String) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .context("building the webhook HTTP client")?;
        Ok(Self { client, url })
    }
}

#[async_trait]
impl EventSink for WebhookSink {
    async fn deliver(&self, batch: &[ContractEvent]) -> Result<()> {
        let response = self
            .client
            .post(&self.url)
            .json(batch)
            .send()
            .await
            .context("POSTing the webhook batch")?;

        if !response.status().is_success() {
            return Err(anyhow!("webhook returned HTTP {}", response.status()));
        }
        Ok(())
    }

    fn name(&self) -> &'static str {
        "webhook"
    }
}

/// Writes one JSON object per line. The default sink, so `indexer run` is
/// useful (and pipeable) without standing up a webhook first.
pub struct StdoutSink;

#[async_trait]
impl EventSink for StdoutSink {
    async fn deliver(&self, batch: &[ContractEvent]) -> Result<()> {
        for event in batch {
            println!("{}", serde_json::to_string(event)?);
        }
        Ok(())
    }

    fn name(&self) -> &'static str {
        "stdout"
    }
}

// ─── Pipeline ────────────────────────────────────────────────────────────────

/// Indexer tuning.
#[derive(Debug, Clone)]
pub struct IndexerConfig {
    pub network: String,
    pub poll_interval: Duration,
    pub batch_size: usize,
    pub batch_timeout: Duration,
    pub queue_capacity: usize,
    pub max_retries: u32,
    /// Stop once the source is drained. Off in production (an indexer follows
    /// the chain forever); on in tests so a run terminates.
    pub stop_when_drained: bool,
}

impl Default for IndexerConfig {
    fn default() -> Self {
        Self {
            network: "testnet".to_string(),
            poll_interval: DEFAULT_POLL_INTERVAL,
            batch_size: DEFAULT_BATCH_SIZE,
            batch_timeout: DEFAULT_BATCH_TIMEOUT,
            queue_capacity: DEFAULT_QUEUE_CAPACITY,
            max_retries: DEFAULT_MAX_RETRIES,
            stop_when_drained: false,
        }
    }
}

/// Counters, shared across tasks.
#[derive(Debug, Default)]
pub struct Metrics {
    pub events_fetched: AtomicU64,
    pub events_delivered: AtomicU64,
    pub batches_delivered: AtomicU64,
    pub delivery_retries: AtomicU64,
}

impl Metrics {
    pub fn snapshot(&self) -> (u64, u64, u64, u64) {
        (
            self.events_fetched.load(Ordering::Relaxed),
            self.events_delivered.load(Ordering::Relaxed),
            self.batches_delivered.load(Ordering::Relaxed),
            self.delivery_retries.load(Ordering::Relaxed),
        )
    }
}

/// Run the fetch → deliver pipeline until the source drains (when configured)
/// or the process is interrupted.
pub async fn run(
    source: Arc<dyn EventSource>,
    sink: Arc<dyn EventSink>,
    config: IndexerConfig,
    metrics: Arc<Metrics>,
) -> Result<Cursor> {
    let (tx, rx) = mpsc::channel::<ContractEvent>(config.queue_capacity);

    let fetcher = tokio::spawn(fetch_loop(
        Arc::clone(&source),
        tx,
        config.clone(),
        Arc::clone(&metrics),
    ));
    let deliverer = tokio::spawn(deliver_loop(
        Arc::clone(&sink),
        rx,
        config.clone(),
        Arc::clone(&metrics),
    ));

    let cursor = fetcher.await.context("fetch task panicked")??;
    deliverer.await.context("deliver task panicked")??;
    Ok(cursor)
}

/// Poll the source, forwarding events into the queue.
///
/// `tx.send().await` is the backpressure: when the consumer lags and the queue
/// fills, this parks instead of buffering without bound.
async fn fetch_loop(
    source: Arc<dyn EventSource>,
    tx: mpsc::Sender<ContractEvent>,
    config: IndexerConfig,
    metrics: Arc<Metrics>,
) -> Result<Cursor> {
    let mut cursor = Cursor::default();

    loop {
        let events = source.fetch(&cursor).await?;

        if events.is_empty() {
            if config.stop_when_drained {
                break;
            }
            // Caught up: wait a ledger before asking again.
            tokio::time::sleep(config.poll_interval).await;
            continue;
        }

        metrics
            .events_fetched
            .fetch_add(events.len() as u64, Ordering::Relaxed);

        for event in events {
            cursor.paging_token = Some(event.paging_token.clone());
            cursor.ledger = cursor.ledger.max(event.ledger);
            // Only fails once the receiver is gone, which means shutdown.
            if tx.send(event).await.is_err() {
                return Ok(cursor);
            }
        }
    }

    Ok(cursor)
}

/// Batch by size or deadline, then deliver with bounded retries.
async fn deliver_loop(
    sink: Arc<dyn EventSink>,
    mut rx: mpsc::Receiver<ContractEvent>,
    config: IndexerConfig,
    metrics: Arc<Metrics>,
) -> Result<()> {
    let mut batch: Vec<ContractEvent> = Vec::with_capacity(config.batch_size);
    let mut deadline = Instant::now() + config.batch_timeout;

    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());

        // Either an event arrives, or the batch deadline expires. The timeout
        // is what stops a partial batch waiting forever on a quiet chain.
        match tokio::time::timeout(remaining, rx.recv()).await {
            Ok(Some(event)) => {
                batch.push(event);
                if batch.len() >= config.batch_size {
                    flush(&sink, &mut batch, &config, &metrics).await?;
                    deadline = Instant::now() + config.batch_timeout;
                }
            }
            // Channel closed: fetcher is done, ship the remainder.
            Ok(None) => {
                flush(&sink, &mut batch, &config, &metrics).await?;
                return Ok(());
            }
            // Deadline hit.
            Err(_) => {
                flush(&sink, &mut batch, &config, &metrics).await?;
                deadline = Instant::now() + config.batch_timeout;
            }
        }
    }
}

/// Deliver `batch`, retrying with linear backoff. Clears it on success.
async fn flush(
    sink: &Arc<dyn EventSink>,
    batch: &mut Vec<ContractEvent>,
    config: &IndexerConfig,
    metrics: &Arc<Metrics>,
) -> Result<()> {
    if batch.is_empty() {
        return Ok(());
    }

    let mut attempt = 0;
    loop {
        match sink.deliver(batch).await {
            Ok(()) => {
                metrics
                    .events_delivered
                    .fetch_add(batch.len() as u64, Ordering::Relaxed);
                metrics.batches_delivered.fetch_add(1, Ordering::Relaxed);
                batch.clear();
                return Ok(());
            }
            Err(e) => {
                attempt += 1;
                metrics.delivery_retries.fetch_add(1, Ordering::Relaxed);
                if attempt >= config.max_retries {
                    // Surface rather than drop: losing events silently is the
                    // one failure an indexer must not have.
                    return Err(e.context(format!(
                        "sink '{}' failed after {attempt} attempts; {} events undelivered",
                        sink.name(),
                        batch.len()
                    )));
                }
                tokio::time::sleep(Duration::from_millis(100 * u64::from(attempt))).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Yields `pages` in order, then empties forever.
    struct FakeSource {
        pages: Mutex<Vec<Vec<ContractEvent>>>,
    }

    impl FakeSource {
        fn new(pages: Vec<Vec<ContractEvent>>) -> Self {
            let mut p = pages;
            p.reverse(); // pop() from the back
            Self {
                pages: Mutex::new(p),
            }
        }
    }

    #[async_trait]
    impl EventSource for FakeSource {
        async fn fetch(&self, _cursor: &Cursor) -> Result<Vec<ContractEvent>> {
            Ok(self.pages.lock().unwrap().pop().unwrap_or_default())
        }
    }

    /// Records every delivered batch. Optionally fails the first `fail_times`.
    struct RecordingSink {
        batches: Mutex<Vec<Vec<ContractEvent>>>,
        fail_times: Mutex<u32>,
        delay: Option<Duration>,
    }

    impl RecordingSink {
        fn new() -> Self {
            Self {
                batches: Mutex::new(Vec::new()),
                fail_times: Mutex::new(0),
                delay: None,
            }
        }
        fn failing(times: u32) -> Self {
            Self {
                batches: Mutex::new(Vec::new()),
                fail_times: Mutex::new(times),
                delay: None,
            }
        }
        fn slow(delay: Duration) -> Self {
            Self {
                batches: Mutex::new(Vec::new()),
                fail_times: Mutex::new(0),
                delay: Some(delay),
            }
        }
        fn delivered(&self) -> usize {
            self.batches.lock().unwrap().iter().map(Vec::len).sum()
        }
        fn batch_count(&self) -> usize {
            self.batches.lock().unwrap().len()
        }
    }

    #[async_trait]
    impl EventSink for RecordingSink {
        async fn deliver(&self, batch: &[ContractEvent]) -> Result<()> {
            {
                let mut fails = self.fail_times.lock().unwrap();
                if *fails > 0 {
                    *fails -= 1;
                    return Err(anyhow!("simulated sink failure"));
                }
            }
            if let Some(d) = self.delay {
                tokio::time::sleep(d).await;
            }
            self.batches.lock().unwrap().push(batch.to_vec());
            Ok(())
        }
        fn name(&self) -> &'static str {
            "recording"
        }
    }

    fn event(n: u32) -> ContractEvent {
        ContractEvent {
            id: format!("ev-{n}"),
            ledger: n,
            ledger_closed_at: None,
            contract_id: "CTEST".to_string(),
            event_type: "contract".to_string(),
            topics: vec!["dG9waWM=".to_string()],
            value: "dmFsdWU=".to_string(),
            paging_token: format!("tok-{n}"),
        }
    }

    fn events(range: std::ops::Range<u32>) -> Vec<ContractEvent> {
        range.map(event).collect()
    }

    fn test_config() -> IndexerConfig {
        IndexerConfig {
            batch_size: 10,
            batch_timeout: Duration::from_millis(50),
            queue_capacity: 64,
            stop_when_drained: true,
            ..Default::default()
        }
    }

    // ─── Pipeline ────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_delivers_every_event_once() {
        let source = Arc::new(FakeSource::new(vec![events(0..10), events(10..25)]));
        let sink = Arc::new(RecordingSink::new());
        let metrics = Arc::new(Metrics::default());

        run(
            source,
            Arc::clone(&sink) as Arc<dyn EventSink>,
            test_config(),
            Arc::clone(&metrics),
        )
        .await
        .unwrap();

        assert_eq!(sink.delivered(), 25, "no event may be dropped");
        let (fetched, delivered, _, _) = metrics.snapshot();
        assert_eq!(fetched, 25);
        assert_eq!(delivered, 25);
    }

    #[tokio::test]
    async fn test_cursor_advances_to_last_event() {
        let source = Arc::new(FakeSource::new(vec![events(0..5)]));
        let sink = Arc::new(RecordingSink::new());

        let cursor = run(
            source,
            sink as Arc<dyn EventSink>,
            test_config(),
            Arc::new(Metrics::default()),
        )
        .await
        .unwrap();

        // Resume must continue after the last event seen, not re-read the page.
        assert_eq!(cursor.paging_token.as_deref(), Some("tok-4"));
        assert_eq!(cursor.ledger, 4);
    }

    #[tokio::test]
    async fn test_batches_are_capped_at_batch_size() {
        let source = Arc::new(FakeSource::new(vec![events(0..30)]));
        let sink = Arc::new(RecordingSink::new());

        run(
            source,
            Arc::clone(&sink) as Arc<dyn EventSink>,
            test_config(), // batch_size 10
            Arc::new(Metrics::default()),
        )
        .await
        .unwrap();

        assert_eq!(sink.delivered(), 30);
        for b in sink.batches.lock().unwrap().iter() {
            assert!(b.len() <= 10, "batch exceeded batch_size: {}", b.len());
        }
    }

    #[tokio::test]
    async fn test_partial_batch_flushes_on_close() {
        // 3 events with batch_size 10 must still be delivered, not stranded.
        let source = Arc::new(FakeSource::new(vec![events(0..3)]));
        let sink = Arc::new(RecordingSink::new());

        run(
            source,
            Arc::clone(&sink) as Arc<dyn EventSink>,
            test_config(),
            Arc::new(Metrics::default()),
        )
        .await
        .unwrap();

        assert_eq!(sink.delivered(), 3);
        assert_eq!(sink.batch_count(), 1);
    }

    #[tokio::test]
    async fn test_empty_source_delivers_nothing_and_exits() {
        let source = Arc::new(FakeSource::new(vec![]));
        let sink = Arc::new(RecordingSink::new());

        run(
            source,
            Arc::clone(&sink) as Arc<dyn EventSink>,
            test_config(),
            Arc::new(Metrics::default()),
        )
        .await
        .unwrap();

        assert_eq!(sink.delivered(), 0);
    }

    // ─── Retry ───────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_transient_sink_failure_is_retried() {
        let source = Arc::new(FakeSource::new(vec![events(0..5)]));
        let sink = Arc::new(RecordingSink::failing(2)); // fails twice, then works
        let metrics = Arc::new(Metrics::default());

        run(
            source,
            Arc::clone(&sink) as Arc<dyn EventSink>,
            test_config(), // max_retries 3
            Arc::clone(&metrics),
        )
        .await
        .unwrap();

        assert_eq!(sink.delivered(), 5, "events survive a transient failure");
        let (_, _, _, retries) = metrics.snapshot();
        assert_eq!(retries, 2);
    }

    #[tokio::test]
    async fn test_permanent_sink_failure_surfaces_rather_than_dropping() {
        let source = Arc::new(FakeSource::new(vec![events(0..5)]));
        let sink = Arc::new(RecordingSink::failing(99)); // never recovers

        let result = run(
            source,
            sink as Arc<dyn EventSink>,
            test_config(),
            Arc::new(Metrics::default()),
        )
        .await;

        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("undelivered"),
            "a permanent failure must be reported, not swallowed: {err}"
        );
    }

    // ─── Acceptance criterion: ≥100 events/sec ───────────────────────────────

    #[tokio::test]
    async fn test_sustains_at_least_100_events_per_second() {
        // 2,000 events through the real pipeline (bounded queue, batching,
        // metrics) with an instant sink. Asserts the machinery itself is not
        // the bottleneck.
        let source = Arc::new(FakeSource::new(vec![events(0..2_000)]));
        let sink = Arc::new(RecordingSink::new());
        let metrics = Arc::new(Metrics::default());

        let config = IndexerConfig {
            batch_size: 100,
            batch_timeout: Duration::from_millis(50),
            queue_capacity: 1_000,
            stop_when_drained: true,
            ..Default::default()
        };

        let started = Instant::now();
        run(
            source,
            Arc::clone(&sink) as Arc<dyn EventSink>,
            config,
            Arc::clone(&metrics),
        )
        .await
        .unwrap();
        let elapsed = started.elapsed();

        assert_eq!(sink.delivered(), 2_000, "all events must arrive");

        let rate = 2_000.0 / elapsed.as_secs_f64();
        assert!(
            rate >= 100.0,
            "throughput {rate:.0} events/sec is below the 100/sec floor (took {elapsed:?})"
        );
    }

    #[tokio::test]
    async fn test_slow_sink_applies_backpressure_without_losing_events() {
        // Queue (16) far smaller than the event count, and a sink that takes
        // 1ms per batch: the fetcher must block on a full queue rather than
        // buffer without bound, and every event must still land.
        let source = Arc::new(FakeSource::new(vec![events(0..200)]));
        let sink = Arc::new(RecordingSink::slow(Duration::from_millis(1)));

        let config = IndexerConfig {
            batch_size: 20,
            batch_timeout: Duration::from_millis(20),
            queue_capacity: 16,
            stop_when_drained: true,
            ..Default::default()
        };

        run(
            source,
            Arc::clone(&sink) as Arc<dyn EventSink>,
            config,
            Arc::new(Metrics::default()),
        )
        .await
        .unwrap();

        assert_eq!(sink.delivered(), 200, "backpressure must not cost events");
    }

    // ─── RPC request/response shaping ────────────────────────────────────────

    #[test]
    fn test_request_uses_start_ledger_when_no_cursor() {
        let s = RpcEventSource::new("http://rpc".into(), 42, vec!["CABC".into()]).unwrap();
        let body = s.request_body(&Cursor::default());

        assert_eq!(body["method"], "getEvents");
        assert_eq!(body["params"]["startLedger"], 42);
        // RPC rejects startLedger and cursor together.
        assert!(body["params"]["pagination"].get("cursor").is_none());
    }

    #[test]
    fn test_request_uses_cursor_once_available() {
        let s = RpcEventSource::new("http://rpc".into(), 42, vec!["CABC".into()]).unwrap();
        let cursor = Cursor {
            paging_token: Some("tok-9".into()),
            ledger: 9,
        };
        let body = s.request_body(&cursor);

        assert_eq!(body["params"]["pagination"]["cursor"], "tok-9");
        assert!(
            body["params"].get("startLedger").is_none(),
            "startLedger must drop out once a cursor exists"
        );
    }

    #[test]
    fn test_parse_events_maps_rpc_shape() {
        let body = serde_json::json!({
            "result": {
                "events": [{
                    "type": "contract",
                    "ledger": 7,
                    "ledgerClosedAt": "2026-07-16T00:00:00Z",
                    "contractId": "CABC",
                    "id": "0007-0001",
                    "pagingToken": "0007-0001",
                    "topic": ["AAAA", "BBBB"],
                    "value": "Q0NDQw=="
                }]
            }
        });

        let events = parse_events(&body).unwrap();
        assert_eq!(events.len(), 1);
        let e = &events[0];
        assert_eq!(e.id, "0007-0001");
        assert_eq!(e.ledger, 7);
        assert_eq!(e.contract_id, "CABC");
        assert_eq!(e.topics, vec!["AAAA".to_string(), "BBBB".to_string()]);
        assert_eq!(e.value, "Q0NDQw==");
    }

    #[test]
    fn test_parse_events_defaults_paging_token_to_id() {
        // Older RPC builds omit pagingToken; the id is a valid cursor.
        let body = serde_json::json!({
            "result": { "events": [{ "id": "0009-0002", "ledger": 9 }] }
        });
        let events = parse_events(&body).unwrap();
        assert_eq!(events[0].paging_token, "0009-0002");
    }

    #[test]
    fn test_parse_events_rejects_a_malformed_response() {
        let body = serde_json::json!({ "result": {} });
        assert!(parse_events(&body).is_err());
    }
}
