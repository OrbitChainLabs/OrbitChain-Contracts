use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Maps contract error codes to human-readable messages.
///
/// The mapping is derived from the campaign contract's `Error` enum in
/// `campaign/src/types.rs`. Each variant has a stable numeric code (the
/// `#[contracterror]` discriminant) and a user-facing explanation.
///
/// # Usage
///
/// ```ignore
/// let mapper = ErrorMapper::load_builtin();
/// println!("{}", mapper.lookup(1));      // "AlreadyInitialized: initialize called on an already-initialised contract."
/// println!("{}", mapper.lookup(999));    // "Unknown error code: 999"
/// println!("{}", mapper.to_json_pretty());
/// ```
pub struct ErrorMapper {
    /// Built-in campaign error codes, keyed by numeric discriminant.
    codes: BTreeMap<u32, ErrorEntry>,
}

/// A single error mapping entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorEntry {
    /// Numeric error code (matches the `#[contracterror]` discriminant).
    pub code: u32,
    /// Short machine-readable variant name (e.g. `"AlreadyInitialized"`).
    pub name: String,
    /// Human-readable explanation of what the error means.
    pub message: String,
    /// Optional severity level.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub severity: Option<String>,
}

impl ErrorMapper {
    /// Load the built-in error mapping derived from the campaign contract.
    #[must_use]
    pub fn load_builtin() -> Self {
        let mut codes = BTreeMap::new();

        // ── Requested contract error codes ────────────────────────────────
        codes.insert(
            1,
            ErrorEntry {
                code: 1,
                name: "AlreadyInitialized".into(),
                message: "initialize called on an already-initialised contract.".into(),
                severity: None,
            },
        );
        codes.insert(
            2,
            ErrorEntry {
                code: 2,
                name: "NotInitialized".into(),
                message: "Contract has not been initialised yet.".into(),
                severity: None,
            },
        );
        codes.insert(
            3,
            ErrorEntry {
                code: 3,
                name: "Unauthorized".into(),
                message: "Caller is not authorised to perform the operation.".into(),
                severity: Some("error".into()),
            },
        );
        codes.insert(
            4,
            ErrorEntry {
                code: 4,
                name: "CampaignEnded".into(),
                message: "The campaign deadline has already passed.".into(),
                severity: None,
            },
        );
        codes.insert(
            5,
            ErrorEntry {
                code: 5,
                name: "CampaignNotActive".into(),
                message: "Operation requires the campaign to be Active or GoalReached.".into(),
                severity: None,
            },
        );
        codes.insert(
            6,
            ErrorEntry {
                code: 6,
                name: "AssetNotAccepted".into(),
                message: "Donated asset is not in the campaign's accepted assets list.".into(),
                severity: None,
            },
        );
        codes.insert(
            7,
            ErrorEntry {
                code: 7,
                name: "DonationTooSmall".into(),
                message: "Donation amount is below the campaign's minimum threshold.".into(),
                severity: None,
            },
        );
        codes.insert(
            8,
            ErrorEntry {
                code: 8,
                name: "MilestoneNotFound".into(),
                message: "Milestone index is out of range for this campaign.".into(),
                severity: None,
            },
        );
        codes.insert(
            9,
            ErrorEntry {
                code: 9,
                name: "MilestoneNotUnlocked".into(),
                message: "Milestone has not been unlocked yet and cannot be released.".into(),
                severity: None,
            },
        );
        codes.insert(
            10,
            ErrorEntry {
                code: 10,
                name: "PreviousMilestoneNotReleased".into(),
                message: "A previous milestone must be released before this one can be released."
                    .into(),
                severity: None,
            },
        );
        codes.insert(
            11,
            ErrorEntry {
                code: 11,
                name: "CannotCancelWithFunds".into(),
                message: "Cannot cancel the campaign while it still holds funds.".into(),
                severity: None,
            },
        );
        codes.insert(
            12,
            ErrorEntry {
                code: 12,
                name: "RefundWindowClosed".into(),
                message: "Refunds are no longer permitted for this campaign.".into(),
                severity: None,
            },
        );
        codes.insert(
            13,
            ErrorEntry {
                code: 13,
                name: "InvalidGoalAmount".into(),
                message: "goal_amount must be strictly positive.".into(),
                severity: None,
            },
        );
        codes.insert(
            14,
            ErrorEntry {
                code: 14,
                name: "InvalidEndTime".into(),
                message: "end_time must be strictly greater than the current ledger timestamp."
                    .into(),
                severity: None,
            },
        );
        codes.insert(
            15,
            ErrorEntry {
                code: 15,
                name: "InvalidMilestones".into(),
                message:
                    "Milestones must be strictly ascending and the last must equal goal_amount."
                        .into(),
                severity: None,
            },
        );
        codes.insert(
            16,
            ErrorEntry {
                code: 16,
                name: "InsufficientContractBalance".into(),
                message: "Contract does not hold enough funds to fulfil the requested transfer."
                    .into(),
                severity: Some("error".into()),
            },
        );
        codes.insert(
            17,
            ErrorEntry {
                code: 17,
                name: "Overflow".into(),
                message: "A checked arithmetic operation overflowed.".into(),
                severity: Some("error".into()),
            },
        );

        // ── Additional contract errors ────────────────────────────────────
        codes.insert(
            18,
            ErrorEntry {
                code: 18,
                name: "InvalidAssets".into(),
                message: "accepted_assets must be non-empty.".into(),
                severity: None,
            },
        );
        codes.insert(
            19,
            ErrorEntry {
                code: 19,
                name: "InvalidAssetCode".into(),
                message: "asset_code must be non-empty and ≤ 12 characters (Stellar limit).".into(),
                severity: None,
            },
        );
        codes.insert(
            20,
            ErrorEntry {
                code: 20,
                name: "MilestoneMismatch".into(),
                message: "Last milestone target_amount does not equal goal_amount.".into(),
                severity: None,
            },
        );
        codes.insert(
            21,
            ErrorEntry {
                code: 21,
                name: "InvalidMilestoneCount".into(),
                message: "Milestone count must be in the range [1, MAX_MILESTONES].".into(),
                severity: None,
            },
        );
        codes.insert(
            22,
            ErrorEntry {
                code: 22,
                name: "InvalidCampaignTransition".into(),
                message: "The requested campaign status transition is not permitted.".into(),
                severity: None,
            },
        );
        codes.insert(
            23,
            ErrorEntry {
                code: 23,
                name: "InvalidMilestoneTransition".into(),
                message: "The requested milestone status transition is not permitted.".into(),
                severity: None,
            },
        );
        codes.insert(
            24,
            ErrorEntry {
                code: 24,
                name: "GoalNotReached".into(),
                message: "Cannot transition to GoalReached — raised amount < goal.".into(),
                severity: None,
            },
        );
        codes.insert(
            25,
            ErrorEntry {
                code: 25,
                name: "InvalidStorageValue".into(),
                message: "A storage read returned an unexpectedly invalid value.".into(),
                severity: Some("error".into()),
            },
        );
        codes.insert(
            26,
            ErrorEntry {
                code: 26,
                name: "StorageWriteError".into(),
                message: "A storage write failed (entry too large, quota exceeded, etc.).".into(),
                severity: Some("error".into()),
            },
        );

        // ── Asset / transfer ──────────────────────────────────────────────
        codes.insert(
            30,
            ErrorEntry {
                code: 30,
                name: "InvalidRecipient".into(),
                message: "Recipient address is the contract itself — would lock funds permanently."
                    .into(),
                severity: None,
            },
        );
        codes.insert(
            31,
            ErrorEntry {
                code: 31,
                name: "MissingIssuerAddress".into(),
                message:
                    "The asset has no issuer address; transfers require a token contract address."
                        .into(),
                severity: None,
            },
        );
        codes.insert(
            32,
            ErrorEntry {
                code: 32,
                name: "ZeroReleaseAmount".into(),
                message: "Computed release amount is zero after proportional rounding.".into(),
                severity: None,
            },
        );
        codes.insert(
            33,
            ErrorEntry {
                code: 33,
                name: "NothingToRelease".into(),
                message: "released_amount already equals target_amount; nothing left to release."
                    .into(),
                severity: None,
            },
        );
        codes.insert(
            34,
            ErrorEntry {
                code: 34,
                name: "MilestoneReleasedExceedsTarget".into(),
                message: "released_amount would exceed target_amount after this operation.".into(),
                severity: None,
            },
        );

        // ── Milestone ─────────────────────────────────────────────────────
        codes.insert(
            40,
            ErrorEntry {
                code: 40,
                name: "MilestoneAlreadyReleased".into(),
                message: "Milestone is already in the Released state.".into(),
                severity: None,
            },
        );
        codes.insert(
            41,
            ErrorEntry {
                code: 41,
                name: "UnreleasedMilestonesExist".into(),
                message: "All milestones must be Released before the campaign can be concluded."
                    .into(),
                severity: None,
            },
        );

        // ── Refunds ───────────────────────────────────────────────────────
        codes.insert(50, ErrorEntry {
            code: 50,
            name: "RefundNotPermitted".into(),
            message: "Refunds are only permitted when the campaign is Cancelled or Ended without reaching the goal.".into(),
            severity: None,
        });
        codes.insert(
            51,
            ErrorEntry {
                code: 51,
                name: "NoDonorRecord".into(),
                message: "No donor record found for the requesting address.".into(),
                severity: None,
            },
        );
        codes.insert(
            52,
            ErrorEntry {
                code: 52,
                name: "RefundAlreadyClaimed".into(),
                message: "Donor has already claimed a refund for this campaign.".into(),
                severity: None,
            },
        );

        // ── Re-entrancy / concurrency ─────────────────────────────────────
        codes.insert(
            60,
            ErrorEntry {
                code: 60,
                name: "ReentrantCall".into(),
                message: "A re-entrant call was detected; operation aborted.".into(),
                severity: Some("error".into()),
            },
        );

        // ── Amount validation ─────────────────────────────────────────────
        codes.insert(
            70,
            ErrorEntry {
                code: 70,
                name: "InvalidAmount".into(),
                message: "A generic negative or otherwise invalid amount was supplied.".into(),
                severity: None,
            },
        );

        // ── Upgrade / freeze ──────────────────────────────────────────────
        codes.insert(
            80,
            ErrorEntry {
                code: 80,
                name: "ContractFrozen".into(),
                message: "Contract is frozen; all mutating operations are blocked.".into(),
                severity: Some("error".into()),
            },
        );

        Self { codes }
    }

    /// Look up an error code and return a formatted string.
    #[must_use]
    pub fn lookup(&self, code: u32) -> String {
        match self.codes.get(&code) {
            Some(entry) => format!("{}: {}", entry.name, entry.message),
            None => format!("Unknown error code: {}", code),
        }
    }

    /// Look up an error code and return the full `ErrorEntry` if it exists.
    #[must_use]
    pub fn get(&self, code: u32) -> Option<&ErrorEntry> {
        self.codes.get(&code)
    }

    /// Return all error entries as a sorted `Vec`.
    #[must_use]
    pub fn all_entries(&self) -> Vec<&ErrorEntry> {
        self.codes.values().collect()
    }

    /// Serialise the full error map to pretty-printed JSON.
    ///
    /// # Errors
    ///
    /// Returns `serde_json::Error` if serialisation fails.
    pub fn to_json_pretty(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(&self.codes)
    }

    /// Serialise the full error map as a JSON array of entries (flat format).
    ///
    /// # Errors
    ///
    /// Returns `serde_json::Error` if serialisation fails.
    pub fn to_json_array(&self) -> Result<String, serde_json::Error> {
        let entries: Vec<&ErrorEntry> = self.codes.values().collect();
        serde_json::to_string_pretty(&entries)
    }

    /// Return the total number of mapped error codes.
    #[must_use]
    pub fn count(&self) -> usize {
        self.codes.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_builtin_has_all_campaign_errors() {
        let mapper = ErrorMapper::load_builtin();
        // 39 variants in types.rs: 1-17, 18-26, 30-34, 40-41, 50-52, 60, 70, 80
        assert_eq!(mapper.count(), 39, "should have 39 error entries");
    }

    #[test]
    fn test_lookup_known_code() {
        let mapper = ErrorMapper::load_builtin();
        let msg = mapper.lookup(1);
        assert!(msg.contains("AlreadyInitialized"));
        assert!(msg.contains("already-initialised"));
    }

    #[test]
    fn test_lookup_unknown_code() {
        let mapper = ErrorMapper::load_builtin();
        assert_eq!(mapper.lookup(999), "Unknown error code: 999");
    }

    #[test]
    fn test_to_json_pretty_is_valid() {
        let mapper = ErrorMapper::load_builtin();
        let json = mapper.to_json_pretty().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed.as_object().unwrap().contains_key("1"));
    }

    #[test]
    fn test_to_json_array_contains_all_entries() {
        let mapper = ErrorMapper::load_builtin();
        let json = mapper.to_json_array().unwrap();
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.len(), 39);
    }

    #[test]
    fn test_get_returns_none_for_missing() {
        let mapper = ErrorMapper::load_builtin();
        assert!(mapper.get(999).is_none());
    }

    #[test]
    fn test_all_entries_are_sorted() {
        let mapper = ErrorMapper::load_builtin();
        let entries = mapper.all_entries();
        for window in entries.windows(2) {
            assert!(window[0].code < window[1].code, "entries must be sorted");
        }
    }
}
