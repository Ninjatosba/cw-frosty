use cosmwasm_schema::cw_serde;
use cw20::Cw20ReceiveMsg;

use cosmwasm_std::{Decimal, Decimal256, Timestamp, Uint128};

use crate::state::Denom;

#[cw_serde]
pub struct InstantiateMsg {
    pub stake_token_address: String,
    pub reward_token_cw20: Option<String>,
    pub reward_token_native: Option<String>,
    pub admin: Option<String>,
    pub force_claim_ratio: Decimal,
    pub fee_collector: String,
    pub max_bond_duration: u128,
}

#[cw_serde]

pub enum ExecuteMsg {
    Receive(Cw20ReceiveMsg),
    UpdateRewardIndex {},
    SetRewardPerSecond {
        reward_per_second: Uint128,
    },
    ForceClaim {
        release_at: Timestamp,
    },
    UpdateStakerRewards {
        address: Option<String>,
    },
    UnbondStake {
        amount: Option<Uint128>,
        duration_as_days: u128,
    },

    ClaimUnbonded {},

    ReceiveReward {},

    //Update config
    UpdateConfig {
        admin: Option<String>,
        fee_collector: Option<String>,
        force_claim_ratio: Option<Decimal>,
    },
}

#[cw_serde]

pub enum ReceiveMsg {
    Bond { duration_day: u128 },
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
    pub total_reward_claimed: Uint128,
    pub last_updated: Timestamp,
}

#[cw_serde]
pub struct ConfigResponse {
    pub stake_token_address: String,
    pub reward_token_address: Denom,
    pub admin: String,
    pub fee_collector: String,
    pub force_claim_ratio: String,
    pub reward_per_second: Uint128,
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
    pub position_weight: Decimal256,
}

#[cw_serde]
pub struct StakerForAllDurationResponse {
    pub positions: Vec<StakerResponse>,
}

#[cw_serde]
pub struct MigrateMsg {}
