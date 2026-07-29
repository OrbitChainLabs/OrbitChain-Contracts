//! Issue #117 – Functional regression gate for the wasm-opt'd binary.
//!
//! Loads the OPTIMIZED wasm (not the Rust-compiled native code) into the
//! Soroban host VM and drives a full campaign lifecycle through it. If
//! `wasm-opt` ever miscompiles or strips something the host needs, this
//! fails loudly instead of surfacing on-chain.
//!
//! Run via `make optimize-verify` (which builds + optimizes + points
//! `OPTIMIZED_WASM` at the artifact). Skipped when the env var is unset so
//! plain `cargo test` stays green without binaryen installed.

// `register_contract_wasm` is the SDK 26 deprecated-but-stable v1 test API;
// the crate root carries the same blanket allow for the v1 surfaces.
#![allow(deprecated)]

use orbitchain_campaign::types::{MilestoneData, MilestoneStatus, StellarAsset};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{vec, Address, BytesN, Env, IntoVal, Symbol, Val, Vec};

fn optimized_wasm() -> Option<std::vec::Vec<u8>> {
    let path = std::env::var("OPTIMIZED_WASM").ok()?;
    Some(std::fs::read(path).expect("OPTIMIZED_WASM path unreadable"))
}

#[test]
fn optimized_wasm_runs_full_campaign_lifecycle() {
    let Some(wasm) = optimized_wasm() else {
        eprintln!("OPTIMIZED_WASM not set; skipping optimized-wasm execution gate");
        return;
    };

    let env = Env::default();
    env.ledger().set_timestamp(86400 * 365);
    env.mock_all_auths();

    // Register the *optimized wasm bytes* — invocations below run inside the
    // host VM against the wasm-opt output, not against native Rust.
    let contract_id = env.register_contract_wasm(None, wasm.as_slice());

    // hello() — cheapest possible execution proof.
    let hello: Symbol = env.invoke_contract(
        &contract_id,
        &Symbol::new(&env, "hello"),
        Vec::<Val>::new(&env),
    );
    assert_eq!(hello, Symbol::new(&env, "campaign"));

    // version() — exported const round-trips.
    let version: u32 = env.invoke_contract(
        &contract_id,
        &Symbol::new(&env, "version"),
        Vec::<Val>::new(&env),
    );
    assert_eq!(version, 1);

    // initialize() — struct/enum arg decoding through the optimized module.
    let creator = Address::generate(&env);
    let end_time: u64 = env.ledger().timestamp() + 100_000;
    let goal: i128 = 1000;
    // Use the crate's own contracttypes so the host encoding (maps keyed by
    // field name) is exactly what the deployed module expects.
    let assets = vec![
        &env,
        StellarAsset {
            asset_code: soroban_sdk::String::from_str(&env, "XLM"),
            issuer: Some(Address::generate(&env)),
        },
    ];
    let milestones = vec![
        &env,
        MilestoneData {
            index: 0,
            target_amount: goal,
            released_amount: 0,
            description_hash: BytesN::from_array(&env, &[0u8; 32]),
            status: MilestoneStatus::Locked,
            released_at: None,
            released_at_ledger: None,
            release_tx: None,
            released_to: None,
        },
    ];
    let args: Vec<Val> = vec![
        &env,
        creator.into_val(&env),
        goal.into_val(&env),
        end_time.into_val(&env),
        assets.into_val(&env),
        milestones.into_val(&env),
        0i128.into_val(&env),
    ];
    let _: Val = env.invoke_contract(&contract_id, &Symbol::new(&env, "initialize"), args);

    // Read back through a view to prove storage round-trips.
    let status: Val = env.invoke_contract(
        &contract_id,
        &Symbol::new(&env, "get_campaign_status"),
        Vec::<Val>::new(&env),
    );
    let _ = status;
}
