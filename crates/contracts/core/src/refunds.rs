use crate::{assets::AssetConfig, donation, donation::Donation, rbac::Rbac, storage_optimized::hash_string};
use soroban_sdk::{contracttype, token, Address, BytesN, Env, String, Vec};

const FULL_REFUND_BPS: u32 = 10_000;

#[contracttype]
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum CampaignStatus {
    Active,
    Cancelled,
    Successful,
    Expired,
}

#[contracttype]
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum RefundEligibility {
    Eligible,
    NotEligible,
    TimeLocked,
    PartiallyEligible,
}

#[contracttype]
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum RefundStatus {
    Pending,
    Approved,
    Completed,
    Rejected,
    Expired,
}

#[contracttype]
#[derive(Clone)]
pub struct Campaign {
    pub beneficiary: Address,
    pub goal_amount: i128,
    pub end_timestamp: u64,
    pub allow_donor_refunds: bool,
    pub refund_bps: u32,
    pub refund_deadline: u64,
    pub status: CampaignStatus,
    pub total_raised: i128,
    pub cancelled_at: u64,
    pub cancel_reason: String,
}

#[contracttype]
#[derive(Clone)]
pub struct RefundRequest {
    pub project_id: String,
    pub donation_index: u32,
    pub donor: Address,
    pub amount: i128,
    pub refundable_amount: i128,
    pub asset: String,
    pub status: RefundStatus,
    pub requested_at: u64,
    pub updated_at: u64,
    pub rejection_reason: String,
}

#[contracttype]
#[derive(Clone)]
pub struct EligibleRefund {
    pub donation_index: u32,
    pub amount: i128,
    pub refundable_amount: i128,
    pub asset: String,
    pub eligibility: RefundEligibility,
    pub has_request: bool,
    pub status: RefundStatus,
}

#[contracttype]
enum RefundStorageKey {
    Campaign(BytesN<32>),
    Refund(BytesN<32>, u32),
}

pub fn upsert_campaign(
    env: &Env,
    caller: &Address,
    project_id: &String,
    beneficiary: &Address,
    goal_amount: i128,
    end_timestamp: u64,
    allow_donor_refunds: bool,
    refund_bps: u32,
    refund_deadline: u64,
) -> Result<Campaign, &'static str> {
    Rbac::require_admin_auth(env, caller);

    if goal_amount <= 0 {
        return Err("Campaign goal must be positive");
    }
    if end_timestamp == 0 {
        return Err("Campaign end timestamp must be set");
    }
    if refund_bps == 0 || refund_bps > FULL_REFUND_BPS {
        return Err("Refund basis points must be between 1 and 10000");
    }
    if refund_deadline < end_timestamp {
        return Err("Refund deadline must be at or after the campaign end");
    }

    let existing = load_campaign_record(env, project_id);
    let campaign = Campaign {
        beneficiary: beneficiary.clone(),
        goal_amount,
        end_timestamp,
        allow_donor_refunds,
        refund_bps,
        refund_deadline,
        status: CampaignStatus::Active,
        total_raised: existing.map(|campaign| campaign.total_raised).unwrap_or(0),
        cancelled_at: 0,
        cancel_reason: String::from_str(env, ""),
    };

    store_campaign(env, project_id, &campaign);
    Ok(campaign)
}

pub fn get_campaign(env: &Env, project_id: &String) -> Option<Campaign> {
    load_campaign_record(env, project_id).map(|mut campaign| {
        campaign.status = resolve_campaign_status(env, &campaign);
        campaign
    })
}

pub fn register_donation(env: &Env, project_id: &String, amount: i128) -> Result<(), &'static str> {
    let Some(mut campaign) = load_campaign_record(env, project_id) else {
        return Ok(());
    };

    if resolve_campaign_status(env, &campaign) != CampaignStatus::Active {
        return Err("Campaign is not accepting donations");
    }

    campaign.total_raised = campaign
        .total_raised
        .checked_add(amount)
        .ok_or("Campaign total overflow")?;
    store_campaign(env, project_id, &campaign);
    Ok(())
}

pub fn cancel_campaign(
    env: &Env,
    caller: &Address,
    project_id: &String,
    reason: &String,
) -> Result<Campaign, &'static str> {
    Rbac::require_admin_auth(env, caller);

    let Some(mut campaign) = load_campaign_record(env, project_id) else {
        return Err("Campaign not configured");
    };

    campaign.status = CampaignStatus::Cancelled;
    campaign.cancelled_at = env.ledger().timestamp();
    campaign.cancel_reason = reason.clone();
    store_campaign(env, project_id, &campaign);
    Ok(campaign)
}

pub fn request_refund(
    env: &Env,
    donor: &Address,
    project_id: &String,
    donation_index: u32,
) -> Result<RefundRequest, &'static str> {
    donor.require_auth();

    let campaign = load_campaign_record(env, project_id).ok_or("Campaign not configured")?;
    let donation = load_donation(project_id, donation_index, env)?;
    if donation.donor != *donor {
        return Err("Donation does not belong to the requesting donor");
    }

    if let Some(existing) = load_refund_record(env, project_id, donation_index) {
        return match effective_refund_status(env, &campaign, &existing) {
            RefundStatus::Pending => Ok(existing),
            RefundStatus::Completed => Err("Refund already completed"),
            RefundStatus::Rejected => Err("Refund request was rejected"),
            RefundStatus::Expired => Err("Refund request expired"),
            RefundStatus::Approved => Err("Refund is already being processed"),
        };
    }

    match refund_eligibility(env, &campaign) {
        RefundEligibility::Eligible | RefundEligibility::PartiallyEligible => {
            let request = new_pending_request(env, project_id, donation_index, &donation, &campaign);
            store_refund(env, project_id, donation_index, &request);
            Ok(request)
        }
        RefundEligibility::TimeLocked => Err("Refund request is time locked until campaign completion"),
        RefundEligibility::NotEligible => Err("Donation is not eligible for refund"),
    }
}

pub fn process_refund(
    env: &Env,
    caller: &Address,
    project_id: &String,
    donation_index: u32,
    approve: bool,
    rejection_reason: &String,
) -> Result<RefundRequest, &'static str> {
    let project_id_hash = hash_string(env, &project_id.to_string());
    let campaign = load_campaign_record(env, project_id).ok_or("Campaign not configured")?;
    require_campaign_operator(env, caller, &campaign)?;

    process_refund_after_auth(
        env,
        caller,
        project_id,
        &project_id_hash,
        &campaign,
        donation_index,
        approve,
        rejection_reason,
        true,
    )
}

pub fn batch_refund(
    env: &Env,
    caller: &Address,
    project_id: &String,
    donation_indices: &Vec<u32>,
) -> Result<u32, &'static str> {
    let project_id_hash = hash_string(env, &project_id.to_string());
    let campaign = load_campaign_record(env, project_id).ok_or("Campaign not configured")?;
    require_campaign_operator(env, caller, &campaign)?;

    if resolve_campaign_status(env, &campaign) != CampaignStatus::Cancelled {
        return Err("Batch refund is only available for cancelled campaigns");
    }

    let mut processed = 0u32;
    for donation_index in donation_indices.iter() {
        let result = process_refund_after_auth(
            env,
            caller,
            project_id,
            &project_id_hash,
            &campaign,
            donation_index,
            true,
            &String::from_str(env, ""),
            true,
        );

        if result.is_ok() {
            processed += 1;
        }
    }

    Ok(processed)
}

pub fn get_refund_status(env: &Env, project_id: &String, donation_index: u32) -> Option<RefundStatus> {
    let campaign = load_campaign_record(env, project_id)?;
    let request = load_refund_record(env, project_id, donation_index)?;
    Some(effective_refund_status(env, &campaign, &request))
}

pub fn get_eligible_refunds(env: &Env, donor: &Address, project_id: &String) -> Vec<EligibleRefund> {
    let Some(campaign) = load_campaign_record(env, project_id) else {
        return Vec::new(env);
    };

    let donation_count = donation::get_donation_count(env, project_id);
    let mut refunds = Vec::new(env);

    for donation_index in 0..donation_count {
        let Some(donation) = Donation::load(env, project_id, donation_index) else {
            continue;
        };
        if donation.donor != *donor {
            continue;
        }

        let existing = load_refund_record(env, project_id, donation_index);
        let eligibility = if existing.is_some() {
            RefundEligibility::NotEligible
        } else {
            refund_eligibility(env, &campaign)
        };
        let refundable_amount = refund_amount(donation.amount, campaign.refund_bps);
        let status = existing
            .as_ref()
            .map(|request| effective_refund_status(env, &campaign, request))
            .unwrap_or(RefundStatus::Pending);

        refunds.push_back(EligibleRefund {
            donation_index,
            amount: donation.amount,
            refundable_amount,
            asset: donation.asset.clone(),
            eligibility,
            has_request: existing.is_some(),
            status,
        });
    }

    refunds
}

fn process_refund_after_auth(
    env: &Env,
    caller: &Address,
    project_id: &String,
    project_id_hash: &BytesN<32>,
    campaign: &Campaign,
    donation_index: u32,
    approve: bool,
    rejection_reason: &String,
    allow_implicit_request: bool,
) -> Result<RefundRequest, &'static str> {
    let donation = load_donation(project_id, donation_index, env)?;
    let now = env.ledger().timestamp();

    let mut request = match load_refund_by_hash(env, project_id_hash, donation_index) {
        Some(existing) => existing,
        None => {
            if !approve || !allow_implicit_request {
                return Err("Refund request not found");
            }

            match refund_eligibility(env, campaign) {
                RefundEligibility::Eligible | RefundEligibility::PartiallyEligible => {
                    new_pending_request(env, project_id, donation_index, &donation, campaign)
                }
                RefundEligibility::TimeLocked => {
                    return Err("Refund request is time locked until campaign completion");
                }
                RefundEligibility::NotEligible => {
                    return Err("Donation is not eligible for refund");
                }
            }
        }
    };

    match effective_refund_status(env, campaign, &request) {
        RefundStatus::Completed => return Err("Refund already completed"),
        RefundStatus::Rejected => return Err("Refund request was rejected"),
        RefundStatus::Expired => {
            request.status = RefundStatus::Expired;
            request.updated_at = now;
            store_refund_by_hash(env, project_id_hash, donation_index, &request);
            return Err("Refund request expired");
        }
        RefundStatus::Pending | RefundStatus::Approved => {}
    }

    if !approve {
        request.status = RefundStatus::Rejected;
        request.updated_at = now;
        request.rejection_reason = rejection_reason.clone();
        store_refund_by_hash(env, project_id_hash, donation_index, &request);
        return Ok(request);
    }

    request.status = RefundStatus::Approved;
    request.updated_at = now;
    store_refund_by_hash(env, project_id_hash, donation_index, &request);

    let asset_contract = AssetConfig::get_contract_address(env, &request.asset.to_string())
        .ok_or("Asset contract address not configured")?;
    let token_client = token::Client::new(env, &asset_contract);
    let contract_address = env.current_contract_address();

    if token_client.balance(&contract_address) < request.refundable_amount {
        return Err("Insufficient contract balance for refund");
    }

    token_client.transfer(&contract_address, &request.donor, &request.refundable_amount);

    request.status = RefundStatus::Completed;
    request.updated_at = now;
    store_refund_by_hash(env, project_id_hash, donation_index, &request);
    let _ = caller;
    Ok(request)
}

fn require_campaign_operator(env: &Env, caller: &Address, campaign: &Campaign) -> Result<(), &'static str> {
    if let Some(admin) = Rbac::get_admin(env) {
        if admin == *caller {
            caller.require_auth();
            return Ok(());
        }
    }

    if campaign.beneficiary == *caller {
        caller.require_auth();
        return Ok(());
    }

    Err("Unauthorized: caller cannot process refunds for this campaign")
}

fn refund_eligibility(env: &Env, campaign: &Campaign) -> RefundEligibility {
    if refund_window_expired(env, campaign) {
        return RefundEligibility::NotEligible;
    }

    let status = resolve_campaign_status(env, campaign);
    let is_partial = campaign.refund_bps < FULL_REFUND_BPS;

    match status {
        CampaignStatus::Cancelled => {
            if is_partial {
                RefundEligibility::PartiallyEligible
            } else {
                RefundEligibility::Eligible
            }
        }
        CampaignStatus::Active => {
            if campaign.allow_donor_refunds {
                RefundEligibility::TimeLocked
            } else {
                RefundEligibility::NotEligible
            }
        }
        CampaignStatus::Expired => {
            if campaign.total_raised < campaign.goal_amount || campaign.allow_donor_refunds {
                if is_partial {
                    RefundEligibility::PartiallyEligible
                } else {
                    RefundEligibility::Eligible
                }
            } else {
                RefundEligibility::NotEligible
            }
        }
        CampaignStatus::Successful => {
            if campaign.allow_donor_refunds {
                if is_partial {
                    RefundEligibility::PartiallyEligible
                } else {
                    RefundEligibility::Eligible
                }
            } else {
                RefundEligibility::NotEligible
            }
        }
    }
}

fn resolve_campaign_status(env: &Env, campaign: &Campaign) -> CampaignStatus {
    if campaign.status == CampaignStatus::Cancelled {
        return CampaignStatus::Cancelled;
    }

    if env.ledger().timestamp() < campaign.end_timestamp {
        return CampaignStatus::Active;
    }

    if campaign.total_raised >= campaign.goal_amount {
        CampaignStatus::Successful
    } else {
        CampaignStatus::Expired
    }
}

fn effective_refund_status(env: &Env, campaign: &Campaign, request: &RefundRequest) -> RefundStatus {
    if request.status == RefundStatus::Pending && refund_window_expired(env, campaign) {
        RefundStatus::Expired
    } else {
        request.status
    }
}

fn refund_window_expired(env: &Env, campaign: &Campaign) -> bool {
    campaign.refund_deadline > 0 && env.ledger().timestamp() > campaign.refund_deadline
}

fn refund_amount(amount: i128, refund_bps: u32) -> i128 {
    amount
        .checked_mul(refund_bps as i128)
        .unwrap_or(0)
        / FULL_REFUND_BPS as i128
}

fn new_pending_request(
    env: &Env,
    project_id: &String,
    donation_index: u32,
    donation: &Donation,
    campaign: &Campaign,
) -> RefundRequest {
    let now = env.ledger().timestamp();
    RefundRequest {
        project_id: project_id.clone(),
        donation_index,
        donor: donation.donor.clone(),
        amount: donation.amount,
        refundable_amount: refund_amount(donation.amount, campaign.refund_bps),
        asset: donation.asset.clone(),
        status: RefundStatus::Pending,
        requested_at: now,
        updated_at: now,
        rejection_reason: String::from_str(env, ""),
    }
}

fn load_donation(project_id: &String, donation_index: u32, env: &Env) -> Result<Donation, &'static str> {
    Donation::load(env, project_id, donation_index).ok_or("Donation not found")
}

fn store_campaign(env: &Env, project_id: &String, campaign: &Campaign) {
    let project_id_hash = hash_string(env, &project_id.to_string());
    env.storage()
        .persistent()
        .set(&RefundStorageKey::Campaign(project_id_hash), campaign);
}

fn load_campaign_record(env: &Env, project_id: &String) -> Option<Campaign> {
    let project_id_hash = hash_string(env, &project_id.to_string());
    env.storage()
        .persistent()
        .get(&RefundStorageKey::Campaign(project_id_hash))
}

fn store_refund(env: &Env, project_id: &String, donation_index: u32, request: &RefundRequest) {
    let project_id_hash = hash_string(env, &project_id.to_string());
    store_refund_by_hash(env, &project_id_hash, donation_index, request);
}

fn store_refund_by_hash(
    env: &Env,
    project_id_hash: &BytesN<32>,
    donation_index: u32,
    request: &RefundRequest,
) {
    env.storage().persistent().set(
        &RefundStorageKey::Refund(project_id_hash.clone(), donation_index),
        request,
    );
}

fn load_refund_record(env: &Env, project_id: &String, donation_index: u32) -> Option<RefundRequest> {
    let project_id_hash = hash_string(env, &project_id.to_string());
    load_refund_by_hash(env, &project_id_hash, donation_index)
}

fn load_refund_by_hash(
    env: &Env,
    project_id_hash: &BytesN<32>,
    donation_index: u32,
) -> Option<RefundRequest> {
    env.storage()
        .persistent()
        .get(&RefundStorageKey::Refund(project_id_hash.clone(), donation_index))
}