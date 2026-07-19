//! Off-chain withdrawal audit log with durable, append-only persistence.
//!
//! Issue #38 — the audit log is the primary *non-blockchain* record of admin
//! actions (`Requested`, `Approved`, `Submitted`, `Rejected`) on creator
//! withdrawals. An earlier design kept the entire history in process memory
//! only, so any crash, restart, or container eviction silently lost the trail —
//! breaking compliance and post-incident forensics.
//!
//! This module keeps the in-memory buffer for fast reads but adds an
//! **append-only on-disk sink** ([`WithdrawalAuditLog::flush_to_disk`]) that
//! writes [JSON Lines](https://jsonlines.org/) with restrictive (`0o600`)
//! permissions, so the log survives process restarts and can be replayed,
//! rotated, and audited later. See `docs/deployment.md` ("Withdrawal audit log")
//! for the rotation policy and on-disk schema.
//!
//! Timestamps come from an injectable [`Clock`] so tests are deterministic and
//! production aligns with a single wall-clock authority. The on-chain Soroban
//! event timestamp can be carried alongside (`ledger_timestamp`) so the
//! off-chain entry and the on-chain `WithdrawalRequested`/`WithdrawalApproved`
//! event can be cross-checked.
//!
//! Note: this module is a self-contained, library-level audit sink. Wiring a
//! periodic background flusher into a long-running worker loop is intentionally
//! out of scope here — `crates/tools` currently ships a synchronous CLI with no
//! off-chain withdrawal-event pipeline. Callers flush explicitly (e.g. after
//! each batch, or on a timer once such a loop exists). Tracked separately.

use std::cell::Cell;
use std::fs::OpenOptions;
use std::io::Write;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// A single admin action on a creator withdrawal, mirroring the on-chain
/// withdrawal lifecycle plus the off-chain `Rejected` outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WithdrawalAction {
    /// A creator requested a withdrawal (on-chain `WithdrawalRequested`).
    Requested,
    /// An admin approved a pending request (on-chain `WithdrawalApproved`).
    Approved,
    /// The approved transaction was submitted/confirmed on-chain.
    Submitted,
    /// An admin rejected a pending request (off-chain only — no on-chain event).
    Rejected,
}

impl std::fmt::Display for WithdrawalAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            WithdrawalAction::Requested => "Requested",
            WithdrawalAction::Approved => "Approved",
            WithdrawalAction::Submitted => "Submitted",
            WithdrawalAction::Rejected => "Rejected",
        };
        f.write_str(s)
    }
}

/// One immutable audit record. Serialized as a single JSON object per line.
///
/// Field set deliberately mirrors the on-chain `WithdrawalRequest`
/// (`campaign_id`, `recipient`, `amount`) so off-chain entries can be joined to
/// the corresponding on-chain event during forensics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WithdrawalLogEntry {
    /// Campaign the withdrawal belongs to.
    pub campaign_id: u64,
    /// Recipient (creator) address, as the canonical Stellar `G...` string.
    pub recipient: String,
    /// Withdrawal amount in base units (matches the on-chain `i128`).
    pub amount: i128,
    /// What happened.
    pub action: WithdrawalAction,
    /// Who performed the action (admin/creator address or operator id).
    pub actor: String,
    /// Audit clock time (Unix seconds) sourced from the injected [`Clock`].
    pub timestamp: i64,
    /// On-chain ledger timestamp of the matching Soroban event, when known.
    /// Lets auditors cross-check the off-chain clock against ledger time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ledger_timestamp: Option<u64>,
    /// Hash of the Soroban transaction that carried the on-chain event, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tx_hash: Option<String>,
}

/// Time source for audit timestamps.
///
/// The default method returns wall-clock Unix seconds via `chrono::Utc::now()`.
/// Production uses [`SystemClock`] (the default); tests inject [`FixedClock`]
/// for deterministic, ledger-aligned timestamps. Per issue #38, `Utc::now()`
/// is reached *only* through this trait so it can never leak into a test path.
pub trait Clock {
    /// Current time in Unix seconds.
    fn now_timestamp(&self) -> i64 {
        chrono::Utc::now().timestamp()
    }
}

/// Production clock — uses the [`Clock`] trait default (`chrono::Utc::now()`).
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;
impl Clock for SystemClock {}

/// Test clock — always returns a fixed, caller-supplied timestamp.
#[derive(Debug, Clone, Copy)]
pub struct FixedClock(pub i64);
impl Clock for FixedClock {
    fn now_timestamp(&self) -> i64 {
        self.0
    }
}

/// In-memory withdrawal audit log backed by an append-only on-disk sink.
///
/// `flush_to_disk` is incremental: it appends only entries logged since the
/// previous flush (tracked by an internal cursor), so repeated/periodic flushes
/// never duplicate or rewrite existing lines. Reload prior history with
/// [`WithdrawalAuditLog::load_from_disk`] on startup before logging more.
pub struct WithdrawalAuditLog {
    entries: Vec<WithdrawalLogEntry>,
    /// Index of the first not-yet-flushed entry. `Cell` so `flush_to_disk`
    /// can advance it through a shared `&self` reference.
    flushed: Cell<usize>,
    clock: Box<dyn Clock>,
}

impl Default for WithdrawalAuditLog {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            flushed: Cell::new(0),
            clock: Box::new(SystemClock),
        }
    }
}

impl std::fmt::Debug for WithdrawalAuditLog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WithdrawalAuditLog")
            .field("entries", &self.entries)
            .field("flushed", &self.flushed.get())
            .finish_non_exhaustive()
    }
}

impl WithdrawalAuditLog {
    /// Create an empty log using the production [`SystemClock`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Create an empty log with an injected clock (use [`FixedClock`] in tests).
    pub fn with_clock(clock: Box<dyn Clock>) -> Self {
        Self {
            entries: Vec::new(),
            flushed: Cell::new(0),
            clock,
        }
    }

    /// Append an audit entry, stamping it with the injected clock.
    ///
    /// `ledger_timestamp`/`tx_hash` carry the matching on-chain event metadata
    /// when available (pass `None` for off-chain-only actions like `Rejected`).
    /// Returns the stored entry for convenience.
    #[allow(clippy::too_many_arguments)] // audit entries carry the full event shape
    pub fn log(
        &mut self,
        action: WithdrawalAction,
        campaign_id: u64,
        recipient: impl Into<String>,
        amount: i128,
        actor: impl Into<String>,
        ledger_timestamp: Option<u64>,
        tx_hash: Option<String>,
    ) -> &WithdrawalLogEntry {
        self.entries.push(WithdrawalLogEntry {
            campaign_id,
            recipient: recipient.into(),
            amount,
            action,
            actor: actor.into(),
            timestamp: self.clock.now_timestamp(),
            ledger_timestamp,
            tx_hash,
        });
        self.entries.last().expect("entry was just pushed")
    }

    /// All entries currently held in memory.
    pub fn entries(&self) -> &[WithdrawalLogEntry] {
        &self.entries
    }

    /// Number of entries in memory.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the in-memory log is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Number of entries not yet flushed to disk.
    pub fn pending_flush(&self) -> usize {
        self.entries.len() - self.flushed.get()
    }

    /// Append all not-yet-flushed entries to `path` as JSON Lines.
    ///
    /// - **Append-only**: opens with `O_APPEND | O_CREAT`, never truncating an
    ///   existing file, so a crash-restart never destroys prior history.
    /// - **Atomic per write**: all pending entries are serialized into one
    ///   buffer and written with a single `write_all`; POSIX `O_APPEND`
    ///   guarantees the write lands at end-of-file even with concurrent writers.
    /// - **Durable**: `sync_all` (fsync) is called so entries survive a crash.
    /// - **Restrictive perms**: the file is `chmod 0o600` on Unix.
    ///
    /// Idempotent across periodic calls: only entries logged since the previous
    /// successful flush are written, then the internal cursor advances.
    pub fn flush_to_disk(&self, path: &str) -> Result<()> {
        let start = self.flushed.get();
        let pending = &self.entries[start..];
        if pending.is_empty() {
            return Ok(());
        }

        let mut buf = String::with_capacity(pending.len() * 128);
        for entry in pending {
            let line = serde_json::to_string(entry)
                .context("Failed to serialize withdrawal audit entry")?;
            buf.push_str(&line);
            buf.push('\n');
        }

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .with_context(|| format!("Failed to open audit log for append: {path}"))?;

        // Tighten permissions to owner-only before writing sensitive records.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = file
                .metadata()
                .context("Failed to read audit log metadata")?
                .permissions();
            perms.set_mode(0o600);
            std::fs::set_permissions(path, perms).context("Failed to set audit log permissions")?;
        }

        file.write_all(buf.as_bytes())
            .with_context(|| format!("Failed to append to audit log: {path}"))?;
        file.sync_all()
            .with_context(|| format!("Failed to fsync audit log: {path}"))?;

        self.flushed.set(self.entries.len());
        Ok(())
    }

    /// Read an existing JSON Lines audit file back into memory using the
    /// production clock. Call this on startup so subsequent `flush_to_disk`
    /// appends to — rather than re-appends — the existing history (the cursor
    /// starts past all loaded entries). Returns an empty log if the file does
    /// not exist yet.
    pub fn load_from_disk(path: &str) -> Result<Self> {
        Self::load_from_disk_with_clock(path, Box::new(SystemClock))
    }

    /// Like [`load_from_disk`](Self::load_from_disk) but with an injected clock
    /// for new entries appended after loading.
    pub fn load_from_disk_with_clock(path: &str, clock: Box<dyn Clock>) -> Result<Self> {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::with_clock(clock));
            }
            Err(e) => {
                return Err(e).with_context(|| format!("Failed to read audit log: {path}"));
            }
        };

        let mut entries = Vec::new();
        for (lineno, line) in content.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let entry: WithdrawalLogEntry = serde_json::from_str(line)
                .with_context(|| format!("Malformed audit entry at {path}:{}", lineno + 1))?;
            entries.push(entry);
        }

        let flushed = entries.len();
        Ok(Self {
            entries,
            flushed: Cell::new(flushed),
            clock,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXED: i64 = 1_700_000_000; // deterministic audit clock for tests

    fn temp_path(name: &str) -> String {
        // Per-test unique path under the OS temp dir; cleaned up by each test.
        let dir = std::env::temp_dir();
        let pid = std::process::id();
        dir.join(format!("orbitchain_audit_{pid}_{name}.jsonl"))
            .to_string_lossy()
            .into_owned()
    }

    fn sample_log() -> WithdrawalAuditLog {
        WithdrawalAuditLog::with_clock(Box::new(FixedClock(FIXED)))
    }

    #[test]
    fn clock_injection_is_deterministic() {
        let mut log = sample_log();
        let entry = log.log(
            WithdrawalAction::Requested,
            7,
            "GCREATOR",
            1_000,
            "GADMIN",
            Some(42),
            None,
        );
        assert_eq!(entry.timestamp, FIXED);
        assert_eq!(entry.action, WithdrawalAction::Requested);
        assert_eq!(entry.ledger_timestamp, Some(42));
    }

    #[test]
    fn flush_writes_valid_json_lines() -> Result<()> {
        let path = temp_path("valid_lines");
        let _ = std::fs::remove_file(&path);

        let mut log = sample_log();
        log.log(
            WithdrawalAction::Requested,
            1,
            "GA",
            500,
            "GADMIN",
            None,
            None,
        );
        log.log(
            WithdrawalAction::Approved,
            1,
            "GA",
            500,
            "GADMIN",
            Some(99),
            None,
        );
        log.flush_to_disk(&path)?;

        let content = std::fs::read_to_string(&path)?;
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2);
        for line in &lines {
            // Each line must independently parse back to an entry.
            let _: WithdrawalLogEntry = serde_json::from_str(line)?;
        }
        let first: WithdrawalLogEntry = serde_json::from_str(lines[0])?;
        assert_eq!(first.amount, 500);
        assert_eq!(first.action, WithdrawalAction::Requested);

        std::fs::remove_file(&path)?;
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn flush_sets_restrictive_permissions() -> Result<()> {
        use std::os::unix::fs::PermissionsExt;
        let path = temp_path("perms");
        let _ = std::fs::remove_file(&path);

        let mut log = sample_log();
        log.log(
            WithdrawalAction::Requested,
            1,
            "GA",
            1,
            "GADMIN",
            None,
            None,
        );
        log.flush_to_disk(&path)?;

        let mode = std::fs::metadata(&path)?.permissions().mode();
        assert_eq!(mode & 0o777, 0o600, "audit log must be owner-only");

        std::fs::remove_file(&path)?;
        Ok(())
    }

    #[test]
    fn repeated_flush_is_incremental_not_duplicated() -> Result<()> {
        let path = temp_path("incremental");
        let _ = std::fs::remove_file(&path);

        let mut log = sample_log();
        log.log(
            WithdrawalAction::Requested,
            1,
            "GA",
            1,
            "GADMIN",
            None,
            None,
        );
        log.flush_to_disk(&path)?;
        // A flush with no new entries is a no-op.
        log.flush_to_disk(&path)?;
        assert_eq!(std::fs::read_to_string(&path)?.lines().count(), 1);

        // New entry → only that entry is appended.
        log.log(WithdrawalAction::Approved, 1, "GA", 1, "GADMIN", None, None);
        log.flush_to_disk(&path)?;
        assert_eq!(std::fs::read_to_string(&path)?.lines().count(), 2);

        std::fs::remove_file(&path)?;
        Ok(())
    }

    /// Crash-restart-fork: a fresh process reloads the log and appends without
    /// truncating the prior history.
    #[test]
    fn crash_restart_preserves_history_without_truncation() -> Result<()> {
        let path = temp_path("restart");
        let _ = std::fs::remove_file(&path);

        // Process 1: log two events and flush.
        {
            let mut log = sample_log();
            log.log(
                WithdrawalAction::Requested,
                5,
                "GA",
                100,
                "GADMIN",
                None,
                None,
            );
            log.log(
                WithdrawalAction::Approved,
                5,
                "GA",
                100,
                "GADMIN",
                Some(7),
                None,
            );
            log.flush_to_disk(&path)?;
        } // "crash": process 1 drops everything in memory.

        // Process 2: reload from disk, then log + flush a third event.
        {
            let mut log =
                WithdrawalAuditLog::load_from_disk_with_clock(&path, Box::new(FixedClock(FIXED)))?;
            assert_eq!(log.len(), 2, "history must be reloaded, not lost");
            assert_eq!(log.pending_flush(), 0, "loaded entries are already on disk");

            log.log(
                WithdrawalAction::Submitted,
                5,
                "GA",
                100,
                "GADMIN",
                Some(8),
                Some("deadbeef".into()),
            );
            log.flush_to_disk(&path)?;
        }

        // File now holds all three entries in order — nothing truncated.
        let content = std::fs::read_to_string(&path)?;
        let entries: Vec<WithdrawalLogEntry> = content
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].action, WithdrawalAction::Requested);
        assert_eq!(entries[1].action, WithdrawalAction::Approved);
        assert_eq!(entries[2].action, WithdrawalAction::Submitted);
        assert_eq!(entries[2].tx_hash.as_deref(), Some("deadbeef"));

        std::fs::remove_file(&path)?;
        Ok(())
    }

    #[test]
    fn load_missing_file_returns_empty_log() -> Result<()> {
        let path = temp_path("missing_never_created");
        let _ = std::fs::remove_file(&path);
        let log = WithdrawalAuditLog::load_from_disk(&path)?;
        assert!(log.is_empty());
        Ok(())
    }

    #[test]
    fn action_round_trips_through_json() {
        for action in [
            WithdrawalAction::Requested,
            WithdrawalAction::Approved,
            WithdrawalAction::Submitted,
            WithdrawalAction::Rejected,
        ] {
            let json = serde_json::to_string(&action).unwrap();
            let back: WithdrawalAction = serde_json::from_str(&json).unwrap();
            assert_eq!(action, back);
        }
        // snake_case wire format is part of the on-disk schema contract.
        assert_eq!(
            serde_json::to_string(&WithdrawalAction::Requested).unwrap(),
            "\"requested\""
        );
    }
}
