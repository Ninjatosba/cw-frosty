use std::convert::Infallible;

use cosmwasm_std::{CheckedFromRatioError, DivideByZeroError, OverflowError, StdError, Uint128};
use cw_asset::AssetError;
use cw_utils::PaymentError;

use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    Overflow(#[from] OverflowError),

    #[error("{0}")]
    Payment(#[from] PaymentError),

    #[error("Reward end time cannot be in the past")]
    InvalidRewardEndTime {},

    #[error("Max bond duration cant be lower than 1 day")]
    InvalidMaxBondDuration {},

    #[error("Invalid bond duration as days")]
    InvalidBondDuration {},

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Decrease amount exceeds user balance: {0}")]
    DecreaseAmountExceeds(Uint128),

    #[error("No claim for user")]
    NoClaim {},

    #[error("No mature claim found for user")]
    NoMatureClaim {},

    #[error("Release time can not be in the past")]
    InvalidReleaseTime {},

    #[error("No claim for sent timestamp")]
    NoClaimForTimestamp {},

    #[error("No bond")]
    NoBond {},

    #[error("Please send right denom and funds")]
    NoFund {},

    #[error("Invalid cw20 token address")]
    InvalidCw20TokenAddress {},

    #[error("No Bond for duration sent")]
    NoBondForThisDuration {},

    #[error("Reward per second must be greater than 0")]
    InvalidRewardPerSecond {},

    #[error("Withdraw amount is higher than the bonded amount")]
    InsufficientStakedAmount {},

    #[error("Force claim ratio must be between 0 and 1")]
    InvalidForceClaimRatio {},

    #[error("Asset error")]
    AssetError {},

    #[error("Invalid reward token denom")]
    InvalidRewardTokenDenom {},

    #[error("Can not divide by zero")]
    DivideByZero {},

    #[error("Overflow error")]
    OverflowError {},

    
}

impl From<AssetError> for ContractError {
    fn from(_err: AssetError) -> Self {
        ContractError::AssetError {}
    }
}
impl From<CheckedFromRatioError> for ContractError {
    fn from(_err: CheckedFromRatioError) -> Self {
        ContractError::DivideByZero {}
    }
}

impl From<Infallible> for ContractError {
    fn from(_err: Infallible) -> Self {
        ContractError::OverflowError {}
    }
}
