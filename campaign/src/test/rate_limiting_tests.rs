//! Tests for per-donor rate limiting and dust-attack protection (issue #91).
//!
//! Covers:
//! - `MAX_DONATIONS_PER_BLOCK` burst cap: back-to-back donations within the
//!   same ledger sequence number are rejected at the threshold.
//! - `max_donations_per_donor`: lifetime donation cap per donor address.
//! - `min_donation_interval_seconds`: cooldown between consecutive donations.
//! - Default `None` values preserve the existing unlimited behaviour (no
//!   breaking change).
//! - First-ever donation always succeeds regardless of interval setting.

#![cfg(test)]

use soroban_sdk::testutils::{Address as AddressTestUtils, Ledger as _};
use soroban_sdk::{Address, BytesN, Env, String, Vec};

use super::with_contract;
use crate::storage::{get_donor, set_donor};
use crate::types::{AssetInfo, DonorRecord, MilestoneData, MilestoneStatus, StellarAsset};
use crate::{CampaignContract, MAX_DONATIONS_PER_BLOCK};

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn default_milestones(env: &Env, goal: i128) -> Vec<MilestoneData> {
    let mut ms: Vec<MilestoneData> = Vec::new(env);
    ms.push_back(MilestoneData {
        index: 0,
        target_amount: goal,
        released_amount: 0,
        description_hash: BytesN::from_array(env, &[1u8; 32]),
        status: MilestoneStatus::Locked,
        released_at: None,
        released_at_ledger: None,
        release_tx: None,
        released_to: None,
    });
    ms
}

/// Initialize a campaign with the given rate-limiting settings.
/// Returns `(creator, asset_issuer_address)`.
fn init_campaign(
    env: &Env,
    max_donations_per_donor: Option<u32>,
    min_donation_interval_seconds: Option<u64>,
) -> (Address, Address) {
    let creator = Address::generate(env);
    let issuer = Address::generate(env);
    let asset = StellarAsset {
        asset_code: String::from_str(env, "XLM"),
        issuer: Some(issuer.clone()),
    };
    let mut assets: Vec<StellarAsset> = Vec::new(env);
    assets.push_back(asset);
    let goal: i128 = 100_000;
    let milestones = default_milestones(env, goal);
    let end_time = env.ledger().timestamp() + 1_000_000;

    CampaignContract::initialize(
        env.clone(),
        creator.clone(),
        goal,
        end_time,
        assets,
        milestones,
        1, // min_donation_amount = 1
        max_donations_per_donor,
        min_donation_interval_seconds,
    )
    .expect("initialize should succeed");

    (creator, issuer)
}

// ─── Default behaviour (None, None) — no breaking change ─────────────────────

/// When both rate-limit settings are `None` the existing unlimited behaviour is
/// preserved: donations from the same donor succeed.  The test seeds a donor
/// record at count 1 and verifies a single subsequent `donate` call brings the
/// count to 2 — two live donations in one frame would hit the
/// "frame is already authorized" host constraint.
#[test]
fn test_rate_limit_defaults_none_allow_multiple_donations() {
    let env = Env::default();
    env.ledger().with_mut(|li| li.timestamp = 1000);
    env.mock_all_auths();
    with_contract(&env, || {
        init_campaign(&env, None, None);
        let donor = Address::generate(&env);

        // Seed donor at count 1 to avoid double-auth in one frame.
        let seq = env.ledger().sequence();
        set_donor(
            &env,
            &donor,
            &DonorRecord {
                donor: donor.clone(),
                total_donated: 100,
                asset: AssetInfo::Native,
                last_donation_time: env.ledger().timestamp(),
                last_donation_ledger: seq,
                donation_count: 1,
                refund_claimed: false,
            },
        );

        // One more donation should bring count to 2.
        CampaignContract::donate(env.clone(), donor.clone(), 100, AssetInfo::Native);

        let record = get_donor(&env, &donor).expect("donor record must exist");
        assert_eq!(record.donation_count, 2);
    });
}

// ─── MAX_DONATIONS_PER_BLOCK burst cap ───────────────────────────────────────

/// The (MAX - 1)th donation in a ledger is accepted; one more donation that
/// reaches MAX is also accepted (the check fires at > MAX, not at = MAX).
#[test]
fn test_burst_cap_allows_exactly_max_donations_per_block() {
    let env = Env::default();
    env.mock_all_auths();
    with_contract(&env, || {
        init_campaign(&env, None, None);
        let donor = Address::generate(&env);

        // Seed donor at MAX - 1 donations in the current ledger.
        let seq = env.ledger().sequence();
        set_donor(
            &env,
            &donor,
            &DonorRecord {
                donor: donor.clone(),
                total_donated: (MAX_DONATIONS_PER_BLOCK as i128 - 1) * 100,
                asset: AssetInfo::Native,
                last_donation_time: env.ledger().timestamp(),
                last_donation_ledger: seq,
                donation_count: MAX_DONATIONS_PER_BLOCK - 1,
                refund_claimed: false,
            },
        );

        // The MAX-th donation in the same ledger should succeed.
        CampaignContract::donate(env.clone(), donor.clone(), 100, AssetInfo::Native);
        let updated = get_donor(&env, &donor).expect("donor record must exist");
        assert_eq!(updated.donation_count, MAX_DONATIONS_PER_BLOCK);
    });
}

/// The (MAX + 1)th donation in the same ledger sequence number is rejected
/// with a panic (mapped to `Error::DonationRateLimited` on-chain).
#[test]
#[should_panic]
fn test_burst_cap_rejects_donation_exceeding_max_per_block() {
    let env = Env::default();
    env.mock_all_auths();
    with_contract(&env, || {
        init_campaign(&env, None, None);
        let donor = Address::generate(&env);

        // Seed donor already at the cap in the current ledger.
        let seq = env.ledger().sequence();
        set_donor(
            &env,
            &donor,
            &DonorRecord {
                donor: donor.clone(),
                total_donated: MAX_DONATIONS_PER_BLOCK as i128 * 100,
                asset: AssetInfo::Native,
                last_donation_time: env.ledger().timestamp(),
                last_donation_ledger: seq,
                donation_count: MAX_DONATIONS_PER_BLOCK,
                refund_claimed: false,
            },
        );

        // Next donation in the same ledger must panic.
        CampaignContract::donate(env.clone(), donor.clone(), 100, AssetInfo::Native);
    });
}

/// After the ledger sequence number advances the per-block cap resets and the
/// donor can donate again.
#[test]
fn test_burst_cap_resets_after_ledger_advance() {
    let env = Env::default();
    env.mock_all_auths();
    with_contract(&env, || {
        init_campaign(&env, None, None);
        let donor = Address::generate(&env);

        let seq = env.ledger().sequence();
        set_donor(
            &env,
            &donor,
            &DonorRecord {
                donor: donor.clone(),
                total_donated: MAX_DONATIONS_PER_BLOCK as i128 * 100,
                asset: AssetInfo::Native,
                last_donation_time: env.ledger().timestamp(),
                last_donation_ledger: seq,
                donation_count: MAX_DONATIONS_PER_BLOCK,
                refund_claimed: false,
            },
        );

        // Move to the next ledger — the burst cap is per ledger sequence number.
        env.ledger().with_mut(|li| {
            li.sequence_number = seq + 1;
            li.timestamp += 5;
        });

        // Donation in the new ledger should succeed.
        CampaignContract::donate(env.clone(), donor.clone(), 100, AssetInfo::Native);
        let updated = get_donor(&env, &donor).expect("donor record must exist");
        assert_eq!(updated.donation_count, MAX_DONATIONS_PER_BLOCK + 1);
    });
}

// ─── max_donations_per_donor lifetime cap ────────────────────────────────────

/// A donor whose lifetime `donation_count` has reached `max_donations_per_donor`
/// is rejected on the next attempt.
#[test]
#[should_panic]
fn test_lifetime_cap_rejects_at_limit() {
    let env = Env::default();
    env.mock_all_auths();
    with_contract(&env, || {
        init_campaign(&env, Some(2), None);
        let donor = Address::generate(&env);

        // Seed at cap; use ledger 1 so the burst guard won't fire.
        set_donor(
            &env,
            &donor,
            &DonorRecord {
                donor: donor.clone(),
                total_donated: 200,
                asset: AssetInfo::Native,
                last_donation_time: 1_000,
                last_donation_ledger: 1,
                donation_count: 2,
                refund_claimed: false,
            },
        );

        // Third donation must panic.
        CampaignContract::donate(env.clone(), donor.clone(), 100, AssetInfo::Native);
    });
}

/// Donations strictly below the lifetime cap succeed normally.
#[test]
fn test_lifetime_cap_allows_donations_below_limit() {
    let env = Env::default();
    env.mock_all_auths();
    with_contract(&env, || {
        init_campaign(&env, Some(5), None);
        let donor = Address::generate(&env);

        CampaignContract::donate(env.clone(), donor.clone(), 100, AssetInfo::Native);
        let record = get_donor(&env, &donor).expect("donor record must exist");
        assert_eq!(record.donation_count, 1);
    });
}

// ─── min_donation_interval_seconds cooldown ──────────────────────────────────

/// A donation attempted before the cooldown window has elapsed is rejected.
#[test]
#[should_panic]
fn test_interval_rejects_donation_within_cooldown() {
    let env = Env::default();
    env.ledger().with_mut(|li| li.timestamp = 1000);
    env.mock_all_auths();
    with_contract(&env, || {
        init_campaign(&env, None, Some(60));
        let donor = Address::generate(&env);
        let now = env.ledger().timestamp();

        // Last donation was 30 s ago — cooldown not yet satisfied.
        set_donor(
            &env,
            &donor,
            &DonorRecord {
                donor: donor.clone(),
                total_donated: 100,
                asset: AssetInfo::Native,
                last_donation_time: now - 30,
                last_donation_ledger: 1,
                donation_count: 1,
                refund_claimed: false,
            },
        );

        CampaignContract::donate(env.clone(), donor.clone(), 100, AssetInfo::Native);
    });
}

/// A donation after the cooldown has fully elapsed is accepted.
#[test]
fn test_interval_allows_donation_after_cooldown_expires() {
    let env = Env::default();
    env.ledger().with_mut(|li| li.timestamp = 1000);
    env.mock_all_auths();
    with_contract(&env, || {
        init_campaign(&env, None, Some(60));
        let donor = Address::generate(&env);
        let now = env.ledger().timestamp();

        // Last donation was 61 s ago — cooldown satisfied.
        set_donor(
            &env,
            &donor,
            &DonorRecord {
                donor: donor.clone(),
                total_donated: 100,
                asset: AssetInfo::Native,
                last_donation_time: now - 61,
                last_donation_ledger: 1,
                donation_count: 1,
                refund_claimed: false,
            },
        );

        CampaignContract::donate(env.clone(), donor.clone(), 100, AssetInfo::Native);
        let updated = get_donor(&env, &donor).expect("donor record must exist");
        assert_eq!(updated.donation_count, 2);
    });
}

/// The very first donation from an address always succeeds even when
/// `min_donation_interval_seconds` is set, because `last_donation_time == 0`.
#[test]
fn test_interval_first_donation_always_allowed() {
    let env = Env::default();
    env.ledger().with_mut(|li| li.timestamp = 1000);
    env.mock_all_auths();
    with_contract(&env, || {
        init_campaign(&env, None, Some(3600));
        let donor = Address::generate(&env);

        // No prior record → first donation must succeed.
        CampaignContract::donate(env.clone(), donor.clone(), 100, AssetInfo::Native);
        let record = get_donor(&env, &donor).expect("donor record must exist");
        assert_eq!(record.donation_count, 1);
        assert!(record.last_donation_time > 0);
    });
}

/// Rejected at exactly (interval - 1) seconds elapsed — one second short.
#[test]
#[should_panic]
fn test_interval_boundary_one_second_short_rejected() {
    let env = Env::default();
    env.ledger().with_mut(|li| li.timestamp = 1000);
    env.mock_all_auths();
    with_contract(&env, || {
        let interval: u64 = 120;
        init_campaign(&env, None, Some(interval));
        let donor = Address::generate(&env);
        let now = env.ledger().timestamp();

        set_donor(
            &env,
            &donor,
            &DonorRecord {
                donor: donor.clone(),
                total_donated: 100,
                asset: AssetInfo::Native,
                last_donation_time: now - (interval - 1),
                last_donation_ledger: 1,
                donation_count: 1,
                refund_claimed: false,
            },
        );

        CampaignContract::donate(env.clone(), donor.clone(), 100, AssetInfo::Native);
    });
}

/// Accepted at exactly `interval` seconds elapsed — boundary is inclusive.
#[test]
fn test_interval_boundary_exact_expiry_accepted() {
    let env = Env::default();
    env.ledger().with_mut(|li| li.timestamp = 1000);
    env.mock_all_auths();
    with_contract(&env, || {
        let interval: u64 = 120;
        init_campaign(&env, None, Some(interval));
        let donor = Address::generate(&env);
        let now = env.ledger().timestamp();

        set_donor(
            &env,
            &donor,
            &DonorRecord {
                donor: donor.clone(),
                total_donated: 100,
                asset: AssetInfo::Native,
                last_donation_time: now - interval,
                last_donation_ledger: 1,
                donation_count: 1,
                refund_claimed: false,
            },
        );

        CampaignContract::donate(env.clone(), donor.clone(), 100, AssetInfo::Native);
        let updated = get_donor(&env, &donor).expect("donor record must exist");
        assert_eq!(updated.donation_count, 2);
    });
}

// ─── Combined guards ─────────────────────────────────────────────────────────

/// With both guards active, the lifetime cap fires (guard 2) even when the
/// interval has already passed (guard 3 would allow it).
#[test]
#[should_panic]
fn test_combined_guards_lifetime_cap_fires_independently() {
    let env = Env::default();
    env.ledger().with_mut(|li| li.timestamp = 1000);
    env.mock_all_auths();
    with_contract(&env, || {
        // Cap at 1 lifetime donation, 60 s cooldown.
        init_campaign(&env, Some(1), Some(60));
        let donor = Address::generate(&env);
        let now = env.ledger().timestamp();

        // Interval is satisfied (120 s elapsed) but donor is already at cap.
        set_donor(
            &env,
            &donor,
            &DonorRecord {
                donor: donor.clone(),
                total_donated: 100,
                asset: AssetInfo::Native,
                last_donation_time: now - 120,
                last_donation_ledger: 1,
                donation_count: 1,
                refund_claimed: false,
            },
        );

        // Lifetime cap must reject the donation.
        CampaignContract::donate(env.clone(), donor.clone(), 100, AssetInfo::Native);
    });
}
