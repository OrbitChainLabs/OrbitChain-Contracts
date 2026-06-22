use crate::types::Error;
use common::ErrorCode;

#[test]
fn campaign_and_common_error_discriminants_do_not_collide() {
    let campaign_codes = [
        Error::AlreadyInitialized as u32,
        Error::NotInitialized as u32,
        Error::Unauthorized as u32,
        Error::CampaignEnded as u32,
        Error::CampaignNotActive as u32,
        Error::AssetNotAccepted as u32,
        Error::DonationTooSmall as u32,
        Error::MilestoneNotFound as u32,
        Error::MilestoneNotUnlocked as u32,
        Error::PreviousMilestoneNotReleased as u32,
        Error::CannotCancelWithFunds as u32,
        Error::RefundWindowClosed as u32,
        Error::InvalidGoalAmount as u32,
        Error::InvalidEndTime as u32,
        Error::InvalidMilestones as u32,
        Error::InsufficientContractBalance as u32,
        Error::Overflow as u32,
        Error::InvalidAssets as u32,
        Error::InvalidAssetCode as u32,
        Error::MilestoneMismatch as u32,
        Error::InvalidMilestoneCount as u32,
        Error::InvalidCampaignTransition as u32,
        Error::InvalidMilestoneTransition as u32,
        Error::GoalNotReached as u32,
        Error::InvalidStorageValue as u32,
        Error::StorageWriteError as u32,
        Error::InvalidRecipient as u32,
        Error::MissingIssuerAddress as u32,
        Error::ZeroReleaseAmount as u32,
        Error::NothingToRelease as u32,
        Error::MilestoneReleasedExceedsTarget as u32,
        Error::MilestoneAlreadyReleased as u32,
        Error::UnreleasedMilestonesExist as u32,
        Error::RefundNotPermitted as u32,
        Error::NoDonorRecord as u32,
        Error::RefundAlreadyClaimed as u32,
        Error::ReentrantCall as u32,
        Error::InvalidAmount as u32,
        Error::ContractFrozen as u32,
    ];

    let common_codes = [
        ErrorCode::NotInitialized as u32,
        ErrorCode::AlreadyInitialized as u32,
        ErrorCode::Unauthorized as u32,
        ErrorCode::InvalidAmount as u32,
    ];

    for common_code in common_codes {
        assert!(
            (1000..=1099).contains(&common_code),
            "common error code {common_code} must stay in the shared 1000..=1099 namespace"
        );
        assert!(
            !campaign_codes.contains(&common_code),
            "common error code {common_code} collides with campaign::Error"
        );
    }

    for campaign_code in campaign_codes {
        assert!(
            campaign_code < 1000,
            "campaign error code {campaign_code} must stay below the shared common namespace"
        );
    }
}
