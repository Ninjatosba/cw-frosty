use crate::{helper, state::StakePosition};
use cosmwasm_schema::cw_serde;
use cw20::Cw20ReceiveMsg;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{ops::Add, string, time::Duration};

use cosmwasm_std::{Addr, Decimal, Decimal256, Timestamp, Uint128};

#[cw_serde]
pub struct InstantiateMsg {
    pub stake_token_address: String,
    pub reward_token_address: String,
    pub admin: Option<String>,
    pub force_claim_ratio: Decimal,
    pub fee_collector: String,
}

#[cw_serde]

pub enum ExecuteMsg {
    ////////////////////
    /// Owner's operations
    ///////////////////
    Receive(Cw20ReceiveMsg),
    /// Update the reward index
    UpdateRewardIndex {},
    ForceClaim {
        unbond_time: Timestamp,
    },
    UpdateStakersReward {
        address: Option<String>,
    },
    UnbondStake {
        amount: Option<Uint128>,
        duration: u128,
    },

    ClaimUnbounded {},

    ReceiveReward {},

    //Update config
    UpdateConfig {
        stake_token_address: Option<String>,
        reward_token_address: Option<String>,
        admin: Option<String>,
        fee_collector: Option<String>,
    },
}

#[cw_serde]

pub enum ReceiveMsg {
    Bond { duration_day: u128 },
    RewardUpdate { reward_end_time: Timestamp },
}

#[cw_serde]
pub enum QueryMsg {
    State {},
    Config {},
    StakerForDuration { address: String, duration: u128 },
    StakerForAllDuration { address: String },

    ListClaims { address: String },
}

#[cw_serde]
pub struct StateResponse {
    pub global_index: Decimal256,
    pub total_staked: Uint128,
    pub total_weight: Decimal256,
    pub reward_end_time: Timestamp,
    pub total_reward_supply: Uint128,
    pub total_reward_claimed: Uint128,
    pub start_time: Timestamp,
    pub last_updated: Timestamp,
}

#[cw_serde]
pub struct ConfigResponse {
    pub stake_token_address: String,
    pub reward_token_address: String,
    pub admin: String,
    pub fee_collector: String,
    pub force_claim_ratio: String,
}

#[cw_serde]
pub struct AccruedRewardsResponse {
    pub rewards: Uint128,
}
#[cw_serde]
pub struct ClaimResponse {
    pub amount: Uint128,
    pub release_at: Timestamp,
    pub unbond_at: Timestamp,
}

#[cw_serde]
pub struct ListClaimsResponse {
    pub claims: Vec<ClaimResponse>,
}

#[cw_serde]
pub struct StakerResponse {
    pub staked_amount: Uint128,
    pub index: Decimal256,
    pub bond_time: Timestamp,
    pub unbond_duration_as_days: u128,
    pub pending_rewards: Uint128,
    pub dec_rewards: Decimal256,
    pub last_claimed: Timestamp,
}

#[cw_serde]
pub struct StakerForAllDurationResponse {
    pub positions: Vec<StakerResponse>,
}

#[cw_serde]
pub struct MigrateMsg {}
