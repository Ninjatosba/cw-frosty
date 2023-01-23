use cosmwasm_std::{Addr, Decimal256, Timestamp, Uint128};

use cw20::Denom;
use cw_controllers::Claims;
use cw_storage_plus::{Item, Map};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct State {
    pub global_index: Decimal256,
    pub total_staked: Uint128,
    pub total_weight: Decimal256,
    pub reward_end_time: Timestamp,
    pub total_reward_supply: Uint128,
    pub remaining_reward_supply: Uint128,
    pub start_time: Timestamp,
    pub last_updated: Timestamp,
}
pub const STATE: Item<State> = Item::new("state");

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub admin: Option<Addr>,
    pub stake_denom: Denom,
    pub reward_denom: Denom,
}

pub const CONFIG: Item<Config> = Item::new("config");

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct StakePosition {
    pub staked_amount: Uint128,
    pub index: Decimal256,
    pub bond_time: Timestamp,
    pub unbond_duration: Duration,
    pub pending_rewards: Uint128,
    pub dec_rewards: Decimal256,
    pub last_claimed: Timestamp,
}

// REWARDS (holder_addr, cw20_addr) -> Holder
pub const STAKEPOSITIONS: Map<&Addr, Vec<StakePosition>> = Map::new("stakers");

pub const CLAIMS: Claims = Claims::new("claims");

impl StakePosition {
    pub fn new(staked_amount: Uint128, bond_time: Timestamp, unbond_duration: Duration) -> Self {
        Self {
            staked_amount,
            index: Decimal256::zero(),
            bond_time,
            unbond_duration,
            pending_rewards: Uint128::zero(),
            dec_rewards: Decimal256::zero(),
            last_claimed: bond_time,
        }
    }
}
