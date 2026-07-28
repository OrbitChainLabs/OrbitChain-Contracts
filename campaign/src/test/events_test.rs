use crate::{event, CampaignContract};
use soroban_sdk::{
    testutils::{Address as _, Events as _}, vec, Address, Env, IntoVal, Map, Symbol, Val,
};

/// The macro derives the first topic from the event struct name. Pin this
/// wire-format contract so indexers cannot be broken by an accidental rename.
#[test]
fn contractevent_topics_are_stable() {
    let env = Env::default();
    let contract_id = env.register(CampaignContract, ());
    let donor = Address::generate(&env);

    env.as_contract(&contract_id, || {
        event::DonationReceived {
            donor: donor.clone(),
            amount: 100,
            asset_code: soroban_sdk::String::from_str(&env, "XLM"),
            raised_total: 100,
            timestamp: 42,
        }
        .publish(&env);
    });

    assert_eq!(
        env.events().all(),
        vec![&env, (
            contract_id,
            (Symbol::new(&env, "donation_received"), donor).into_val(&env),
            Map::<Symbol, Val>::from_array(&env, [
                (Symbol::new(&env, "amount"), 100_i128.into_val(&env)),
                (
                    Symbol::new(&env, "asset_code"),
                    soroban_sdk::String::from_str(&env, "XLM").into_val(&env),
                ),
                (Symbol::new(&env, "raised_total"), 100_i128.into_val(&env)),
                (Symbol::new(&env, "timestamp"), 42_u64.into_val(&env)),
            ])
            .into_val(&env),
        )]
    );
}
