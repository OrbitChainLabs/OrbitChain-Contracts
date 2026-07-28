//! Micro-benchmark suite for campaign hot paths.
//!
//! Issue #111 — Criterion-based benchmarks for the three highest-traffic
//! contract entrypoints: `donate`, `release_milestone`, and `claim_refund`.
//! Each benchmark sets up a fresh Soroban test environment, initialises a
//! campaign with a Stellar asset (real token contract), then measures the
//! target operation including actual token transfers.
//!
//! Run with:
//!   cargo bench -p orbitchain-campaign
//!
//! Save a baseline (on `main`):
//!   cargo bench -p orbitchain-campaign -- --save-baseline main
//!
//! Compare against a baseline (on PR):
//!   cargo bench -p orbitchain-campaign -- --baseline main

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use orbitchain_campaign::types::{AssetInfo, MilestoneData, MilestoneStatus, StellarAsset};
use orbitchain_campaign::CampaignContract;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::token::StellarAssetClient;
use soroban_sdk::{Address, Env, String, Vec};

// ─── Inlined helpers (avoid depending on `#[cfg(test)]`-gated items) ────────

/// Register the campaign contract and run the closure inside the contract
/// context so storage, ledger, and auth work correctly.
fn with_contract<F, R>(env: &Env, f: F) -> R
where
    F: FnOnce() -> R,
{
    let contract_id = env.register_contract(None, CampaignContract);
    env.as_contract(&contract_id, f)
}

/// Create a Stellar asset token and mint some funds to the contract address.
/// Returns the token issuer address and a StellarAsset descriptor.
fn create_token_asset(env: &Env, code: &str) -> (Address, StellarAsset) {
    let admin = Address::generate(env);
    let issuer = env.register_stellar_asset_contract(admin.clone());
    let token_admin = StellarAssetClient::new(env, &issuer);
    // Mint a generous balance so transfers never fail due to insufficient funds
    token_admin.mint(&env.current_contract_address(), &1_000_000_000);
    (
        issuer.clone(),
        StellarAsset {
            asset_code: String::from_str(env, code),
            issuer: Some(issuer),
        },
    )
}

/// Set up a campaign with one Stellar asset and one milestone, then return
/// (creator, donor, token_issuer).
fn setup_minimal_campaign(env: &Env) -> (Address, Address, Address) {
    let creator = Address::generate(env);
    let donor = Address::generate(env);
    let (token_issuer, token_asset) = create_token_asset(env, "USDC");

    let mut assets = Vec::new(env);
    assets.push_back(token_asset);

    let mut milestones = Vec::new(env);
    milestones.push_back(MilestoneData {
        index: 0,
        target_amount: 1_000_000,
        released_amount: 0,
        description_hash: soroban_sdk::BytesN::from_array(env, &[0; 32]),
        status: MilestoneStatus::Locked,
        released_at: None,
        released_at_ledger: None,
        release_tx: None,
        released_to: None,
    });

    env.mock_all_auths();
    with_contract(env, || {
        CampaignContract::initialize(
            env.clone(),
            creator.clone(),
            1_000_000,
            env.ledger().timestamp() + 86_400,
            assets,
            milestones,
            0,
        )
        .unwrap();
    });

    (creator, donor, token_issuer)
}

/// Donate enough to unlock the milestone, then cancel the campaign so refunds
/// are available.  Returns (creator, donor).
fn setup_refund_scenario(env: &Env) -> (Address, Address) {
    let (creator, donor, token_issuer) = setup_minimal_campaign(env);

    // Donate enough to reach the milestone target
    env.mock_all_auths();
    with_contract(env, || {
        CampaignContract::donate(
            env.clone(),
            donor.clone(),
            1_000_000,
            AssetInfo::Stellar(token_issuer),
        );
    });

    // Cancel so refunds are eligible
    env.mock_all_auths();
    with_contract(env, || {
        CampaignContract::cancel_campaign(env.clone());
    });

    (creator, donor)
}

// ─── Benchmarks ──────────────────────────────────────────────────────────────

fn bench_donate(c: &mut Criterion) {
    c.bench_function("donate (Stellar asset, 100 base-units)", |b| {
        let env = Env::default();
        let (_creator, donor, token_issuer) = setup_minimal_campaign(&env);

        b.iter(|| {
            env.mock_all_auths();
            with_contract(&env, || {
                CampaignContract::donate(
                    env.clone(),
                    donor.clone(),
                    100,
                    AssetInfo::Stellar(token_issuer.clone()),
                );
            });
        });
    });
}

fn bench_release_milestone(c: &mut Criterion) {
    c.bench_function("release_milestone (single Stellar asset)", |b| {
        let env = Env::default();
        let (_creator, donor, token_issuer) = setup_minimal_campaign(&env);
        let recipient = Address::generate(&env);

        // Donate enough to unlock the milestone
        env.mock_all_auths();
        with_contract(&env, || {
            CampaignContract::donate(
                env.clone(),
                donor.clone(),
                1_000_000,
                AssetInfo::Stellar(token_issuer),
            );
        });

        b.iter(|| {
            env.mock_all_auths();
            let idx = 0u32;
            with_contract(&env, || {
                CampaignContract::release_milestone(env.clone(), black_box(idx), recipient.clone());
            });
        });
    });
}

fn bench_claim_refund(c: &mut Criterion) {
    c.bench_function("claim_refund (single donor, full refund)", |b| {
        let env = Env::default();
        let (_creator, donor) = setup_refund_scenario(&env);

        b.iter(|| {
            env.mock_all_auths();
            with_contract(&env, || {
                CampaignContract::claim_refund(env.clone(), donor.clone());
            });
        });
    });
}

// ─── Criterion groups ────────────────────────────────────────────────────────

criterion_group!(
    benches,
    bench_donate,
    bench_release_milestone,
    bench_claim_refund
);
criterion_main!(benches);
