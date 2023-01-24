use cw20::{Cw20ReceiveMsg, Denom};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::time::Duration;

use cosmwasm_std::{Decimal256, Uint128};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    pub stake_denom: Denom,
    pub reward_denom: Denom,
    pub admin: Option<String>,
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
    UpdateHoldersReward {
        address: Option<String>,
    },
    ////////////////////
    /// Staking operations
    ///////////////////
    Bond {
        unbonding_duration: Duration,
    },
    /// Unbound user staking balance
    /// Withdraw rewards to pending rewards
    /// Set current reward index to global index
    UnbondStake {},

    WithdrawUnboundedStake {},

    ReceiveReward {},

    //Update config
    UpdateConfig {
        staked_token_denom: Option<String>,
        reward_denom: Option<String>,
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
    AccruedRewards {
        address: String,
    },
    Holder {
        address: String,
    },
    Config {},
    Holders {
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
    pub staked_token_denom: String,
    pub reward_denom: String,
    pub admin: String,
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
