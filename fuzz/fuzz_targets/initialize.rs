#![no_main]
use libfuzzer_sys::fuzz_target;
use arbitrary::Arbitrary;
use soroban_sdk::{testutils::Address as _, Address, Env, Vec as SVec};
use orbitchain_campaign::CampaignContract;
use orbitchain_campaign::types::{MilestoneData, MilestoneStatus, StellarAsset, MAX_MILESTONES};

#[derive(Arbitrary, Debug)]
struct InitializeFuzzInput {
    goal_amount: i128,
    end_time_offset: u64,
    milestone_count: u8,
    min_donation: i128,
    asset_code_len: u8,
}

fuzz_target!(|input: InitializeFuzzInput| {
    let goal_amount = input.goal_amount.abs().max(1) % 1_000_000_000;
    let end_time_offset = (input.end_time_offset % 31_536_000) + 60;
    let milestone_count = (input.milestone_count % MAX_MILESTONES as u8).max(1);
    let min_donation = input.min_donation.abs().max(0) % 1000;

    let env = Env::default();
    env.mock_all_auths();

    let creator = Address::generate(&env);

    #[allow(deprecated)]
    let contract_id = env.register_contract(None, CampaignContract);
    env.as_contract(&contract_id, || {
        let mut milestones: SVec<MilestoneData> = SVec::new(&env);
        let description_hash = soroban_sdk::BytesN::from_array(&env, &[0u8; 32]);
        let increment = goal_amount / milestone_count as i128;
        for i in 0..milestone_count as u32 {
            let target = if i == milestone_count as u32 - 1 {
                goal_amount
            } else {
                increment * (i as i128 + 1)
            };
            milestones.push_back(MilestoneData {
                index: i,
                target_amount: target,
                released_amount: 0,
                description_hash: description_hash.clone(),
                status: MilestoneStatus::Locked,
                released_at: None,
                released_at_ledger: None,
                release_tx: None,
                released_to: None,
            });
        }

        let mut assets: SVec<StellarAsset> = SVec::new(&env);
        let code_len = (input.asset_code_len % 12).max(1) as usize;
        let code_vec = vec![b'A'; code_len];
        assets.push_back(StellarAsset {
            asset_code: soroban_sdk::String::from_bytes(&env, &code_vec),
            issuer: None,
        });

        let current_time = env.ledger().timestamp();
        let _ = CampaignContract::initialize(
            env.clone(),
            creator,
            goal_amount,
            current_time + end_time_offset,
            assets,
            milestones,
            min_donation,
        );
    });
});
