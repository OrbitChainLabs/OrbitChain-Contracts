#![no_main]
use libfuzzer_sys::fuzz_target;
use arbitrary::Arbitrary;
use soroban_sdk::{testutils::Address as _, Address, Env, Vec as SVec};
use orbitchain_campaign::CampaignContract;
use orbitchain_campaign::types::{
    AssetInfo, MilestoneData, MilestoneStatus, StellarAsset,
};

#[derive(Arbitrary, Debug)]
struct DonateFuzzInput {
    goal_amount: i128,
    end_time_offset: u64,
    donation_amount: i128,
    min_donation: i128,
    milestone_count: u8,
}

fuzz_target!(|input: DonateFuzzInput| {
    let goal_amount = input.goal_amount.abs().max(1) % 1_000_000_000;
    let end_time_offset = (input.end_time_offset % 86400) + 60;
    let milestone_count = (input.milestone_count % 5).max(1);
    let min_donation = input.min_donation.abs().max(0) % 1000;

    let env = Env::default();
    env.mock_all_auths();

    let creator = Address::generate(&env);
    let donor = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(token_admin);
    let token_addr = sac.address();
    let token_sac = soroban_sdk::token::StellarAssetClient::new(&env, &token_addr);

    token_sac.mint(&donor, &(input.donation_amount.abs().max(1) * 10));

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
        assets.push_back(StellarAsset {
            asset_code: soroban_sdk::String::from_str(&env, "XLM"),
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

    let safe_amount = input.donation_amount.abs().max(1);
    token_sac.mint(&donor, &safe_amount);
    let _ = env.as_contract(&contract_id, || {
        CampaignContract::donate(env.clone(), donor, safe_amount, AssetInfo::Native)
    });
});
