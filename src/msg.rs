use crate::helper;
use cw20::Cw20ReceiveMsg;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{ops::Add, time::Duration};

use cosmwasm_std::{Addr, Decimal, Decimal256, Timestamp, Uint128};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    pub stake_denom: Addr,
    pub reward_denom: Addr,
    pub admin: Option<String>,
    pub force_claim_ratio: Decimal,
    pub fee_collector: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    ////////////////////
    /// Owner's operations
    ///////////////////
    Receive(Cw20ReceiveMsg),
    /// Update the reward index
    UpdateRewardIndex {},

    FundReward {
        end_time: Duration,
    },
    ForceClaim {
        unbond_time: Timestamp,
    },
    UpdateStakersReward {
        address: Option<String>,
    },
    ////////////////////
    /// Staking operations
    ///////////////////
    Bond {
        unbonding_duration: Duration,
    },
    /// Set current reward index to global index
    UnbondStake {
        amount: Option<Uint128>,
        duration: Duration,
    },

    ClaimUnbounded {},

    ReceiveReward {},

    //Update config
    UpdateConfig {
        staked_token_denom: Option<Addr>,
        reward_denom: Option<Addr>,
        admin: Option<String>,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReceiveMsg {
    Bond { duration: Duration },
    RewardUpdate { duration: Duration },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    State {},
    Config {},
    ClaimableRewards {
        address: String,
    },
    StakerInfo {
        address: String,
    },
    ListClaims {
        address: String,
    },
    ListStakers {
        start_after: Option<String>,
        limit: Option<u32>,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct StateResponse {
    pub global_index: Decimal256,
    pub total_staked: Uint128,
    pub prev_reward_balance: Uint128,
}
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigResponse {
    pub staked_denom: Addr,
    pub reward_denom: Addr,
    pub admin: Addr,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct AccruedRewardsResponse {
    pub rewards: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct HolderResponse {
    pub address: String,
    pub balance: Uint128,
    pub index: Decimal256,
    pub pending_rewards: Uint128,
    pub dec_rewards: Decimal256,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct HoldersResponse {
    pub holders: Vec<HolderResponse>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}
