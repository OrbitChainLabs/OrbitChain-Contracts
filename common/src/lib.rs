//! Common types shared across the OrbitChain workspace.
//!
//! This crate provides canonical definitions for `CampaignStatus`, `MilestoneStatus`,
//! `AssetInfo`, and the shared error-code range used by both campaign and core
//! contracts.
//!
//! # Versioning
//! All discriminants are stable — never renumber existing variants. Shared
//! workspace errors must stay in the `1000..=1099` range so they cannot collide
//! with contract-local error enums such as `campaign::types::Error`.

#![no_std]
use soroban_sdk::{contracterror, contracttype};

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

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ErrorCode {
    /// Contract has not been initialized yet.
    NotInitialized = 1000,
    /// Contract has already been initialized.
    AlreadyInitialized = 1001,
    /// Caller is not authorized to perform this operation.
    Unauthorized = 1002,
    /// The amount supplied is invalid (zero, negative, or out of range).
    InvalidAmount = 1003,
}
