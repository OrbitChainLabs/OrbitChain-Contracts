//! Common types shared across the OrbitChain workspace.
//!
//! This crate provides canonical definitions for `CampaignStatus`, `MilestoneStatus`,
//! and `AssetInfo` used by both campaign and core contracts.
//!
//! This crate intentionally does **not** define a `#[contracterror]` enum.
//! Contract-specific crates own their typed error spaces so shared data types
//! cannot accidentally collide with stable on-chain error discriminants.
//! Campaign errors live in `orbitchain-campaign::types::Error`; the deprecated
//! reference core contract keeps its separate `CoreError` until it is retired.

#![no_std]
use soroban_sdk::contracttype;

#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum CampaignStatus {
    /// Campaign is still being configured; not yet live.
    Draft,
    /// Campaign is live and accepting operations.
    Active,
    /// Campaign has successfully completed.
    Completed,
    /// Campaign was cancelled by the creator.
    Cancelled,
}

#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum MilestoneStatus {
    /// Milestone has not yet been reached.
    Pending,
    /// Milestone has been reached and released.
    Completed,
    /// Milestone was not reached within the timeline.
    Failed,
}

#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct AssetInfo {
    pub code: u32,
    pub issuer: u32,
}
