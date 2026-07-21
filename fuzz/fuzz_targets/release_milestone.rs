#![no_main]
use libfuzzer_sys::fuzz_target;
use arbitrary::Arbitrary;
use soroban_sdk::{testutils::Address as _, Address, Env, Vec as SVec};
use orbitchain_campaign::CampaignContract;
use orbitchain_campaign::types::{
    AssetInfo, MilestoneData, MilestoneStatus, StellarAsset,
};

#[derive(Arbitrary, Debug)]
struct ReleaseMilestoneFuzzInput {
    goal_amount: i128,
    milestone_index: u32,
    donation_amount: i128,
}

fuzz_target!(|input: ReleaseMilestoneFuzzInput| {
    let goal_amount = input.goal_amount.abs().max(100) % 1_000_000;
    let milestone_index = input.milestone_index % 3;
    let donation_amount = input.donation_amount.abs().max(1) % 100_000;

    let env = Env::default();
    env.mock_all_auths();

    let creator = Address::generate(&env);
    let donor = Address::generate(&env);
    let recipient = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(token_admin);
    let token_addr = sac.address();
    let token_sac = soroban_sdk::token::StellarAssetClient::new(&env, &token_addr);

    #[allow(deprecated)]
    let contract_id = env.register_contract(None, CampaignContract);
    env.as_contract(&contract_id, || {
        let mut milestones: SVec<MilestoneData> = SVec::new(&env);
        let description_hash = soroban_sdk::BytesN::from_array(&env, &[0u8; 32]);
        let third = goal_amount / 3;
        for i in 0..3u32 {
            let target = match i {
                0 => third,
                1 => third * 2,
                _ => goal_amount,
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
            creator.clone(),
            goal_amount,
            current_time + 86400,
            assets,
            milestones,
            0,
        );

        token_sac.mint(&donor, &(donation_amount * 10));
        let _ = CampaignContract::donate(
            env.clone(),
            donor,
            donation_amount,
            AssetInfo::Native,
        );

        let _ = CampaignContract::release_milestone(
            env.clone(),
            milestone_index,
            recipient,
        );
    });
});
