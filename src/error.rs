use cosmwasm_std::{OverflowError, StdError, Uint128};
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

    #[error("No rewards accrued")]
    NoRewards {},

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Do not send native funds")]
    DoNotSendFunds {},

    #[error("Amount required")]
    AmountRequired {},

    #[error("Decrease amount exceeds user balance: {0}")]
    DecreaseAmountExceeds(Uint128),

    #[error("Wait for the unbonding")]
    WaitUnbonding {},

    #[error("No claim for user")]
    NoClaim {},

    #[error("No claim for sent timestamp")]
    NoClaimForTimestamp {},

    #[error("No bond")]
    NoBond {},

    #[error("Please send right denom and funds")]
    NoFund {},

    #[error("Address validation failed")]
    InvalidAddress {},

    #[error("Invalid token type")]
    InvalidTokenType {},

    #[error("Invalid config")]
    InvalidConfig {},

    #[error("Invalid cw20 token address")]
    InvalidCw20TokenAddress {},

    #[error("Denom not supperted")]
    DenomNotSupported {},

    #[error("Multiple tokens sent")]
    MultipleTokensSent {},

    #[error("No Bond for duration sent")]
    NoBondForThisDuration {},

    #[error("Withdraw amount is higher than the bonded amount")]
    InsufficientStakedAmount {},
}
