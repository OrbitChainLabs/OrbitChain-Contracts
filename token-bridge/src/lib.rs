#![no_std]
use soroban_sdk::{contract, contractimpl, Env};

#[contract]
pub struct TokenBridgeContract;

#[contractimpl]
impl TokenBridgeContract {
    pub fn hello(env: Env) -> soroban_sdk::Symbol {
        soroban_sdk::Symbol::new(&env, "token_bridge")
    }
}
