use cosmwasm_std::{Addr, Decimal, Decimal256, Order, StdResult, Storage, Timestamp, Uint128};

use cosmwasm_schema::cw_serde;
use cw_storage_plus::{Bound, Item, Map};

use crate::ContractError;

#[cw_serde]
pub struct State {
    pub global_index: Decimal256,
    pub total_staked: Uint128,
    pub total_weight: Decimal256,
    pub total_reward_claimed: Uint128,
    pub last_updated_block: u64,
}

#[cw_serde]
pub enum Denom {
    Native(String),
    Cw20(Addr),
}
impl Denom {
    pub fn to_string(&self) -> String {
        match self {
            Denom::Native(string) => string.to_string(),
            Denom::Cw20(addr) => addr.to_string(),
        }
    }
}

pub const STATE: Item<State> = Item::new("state");

#[cw_serde]
pub struct Claim {
    pub amount: Uint128,
    pub release_at: Timestamp,
    pub unbond_at: Timestamp,
}

pub const CLAIMS_KEY: &str = "claim";
// Claims is a wrapper around map of (address, release_at, id) -> Claim
pub struct Claims<'a>(Map<'a, (Addr, u64, u16), Claim>);

impl<'a> Claims<'a> {
    pub const fn new(storage_key: &'a str) -> Self {
        Claims(Map::new(storage_key))
    }

    pub fn save(
        &self,
        store: &mut dyn Storage,
        address: Addr,
        release_at: u64,
        claim: &Claim,
    ) -> StdResult<()> {
        let last_id = self
            .0
            .prefix((address.clone(), release_at))
            .range(store, None, None, Order::Descending)
            .next()
            .transpose()?
            .map(|(id, _)| id)
            .unwrap_or(0);

        self.0
            .save(store, (address, release_at, last_id + 1), claim)
    }

    pub fn load(
        &self,
        store: &dyn Storage,
        address: Addr,
        release_at: u64,
    ) -> StdResult<Vec<Claim>> {
        self.0
            .prefix((address, release_at))
            .range(store, None, None, Order::Ascending)
            .map(|x| x.map(|(_, v)| v))
            .collect()
    }

    pub fn load_all(&self, store: &dyn Storage, address: Addr) -> StdResult<Vec<Claim>> {
        self.0
            .sub_prefix(address)
            .range(store, None, None, Order::Ascending)
            .map(|x| x.map(|(_, v)| v))
            .collect()
    }

    pub fn load_mature_claims(
        &self,
        store: &dyn Storage,
        address: Addr,
        now_time: u64,
    ) -> StdResult<Vec<Claim>> {
        self.0
            .sub_prefix(address)
            .range(
                store,
                None,
                Some(Bound::exclusive((now_time + 1, 0))),
                Order::Ascending,
            )
            .map(|x| x.map(|(_, v)| v))
            .collect::<StdResult<Vec<_>>>()
    }

    pub fn remove_mature_claims(
        &self,
        store: &mut dyn Storage,
        address: Addr,
        now_time: u64,
    ) -> Result<(), ContractError> {
        self.0
            .sub_prefix(address.clone())
            .range(
                store,
                None,
                Some(Bound::exclusive((now_time + 1, 0))),
                Order::Ascending,
            )
            .map(|x| x.map(|(k, _v)| k))
            .collect::<StdResult<Vec<_>>>()?
            .into_iter()
            .for_each(|k| self.0.remove(store, (address.clone(), k.0, k.1)));
        Ok(())
    }
    pub fn remove_for_release_at(
        &self,
        store: &mut dyn Storage,
        address: Addr,
        release_at_time: u64,
    ) -> Result<(), ContractError> {
        self.0
            .sub_prefix(address.clone())
            .range(
                store,
                Some(Bound::inclusive((release_at_time, 0))),
                Some(Bound::exclusive((release_at_time + 1, 0))),
                Order::Ascending,
            )
            .map(|x| x.map(|(k, _v)| k))
            .collect::<StdResult<Vec<_>>>()?
            .into_iter()
            .for_each(|k| self.0.remove(store, (address.clone(), k.0, k.1)));
        Ok(())
    }
}
#[cw_serde]
pub struct Config {
    pub admin: Addr,
    // Stake token denom must be cw20
    pub stake_token_address: Addr,
    // Reward token denom can be both cw20 or native token
    pub reward_token_denom: Denom,
    pub force_claim_ratio: Decimal,
    pub fee_collector: Addr,
    pub max_bond_duration: u128,
    pub reward_per_block: Uint128,
    pub total_reward: Uint128,
    pub reward_end_block: u64,
}

pub struct CW20Balance {
    pub denom: Addr,
    pub amount: Uint128,
    pub sender: Addr,
}

pub const CONFIG: Item<Config> = Item::new("config");

#[cw_serde]
pub struct StakePosition {
    pub staked_amount: Uint128,
    pub index: Decimal256,
    pub bond_time_block: u64,
    pub unbond_duration_as_days: u128,
    pub pending_rewards: Uint128,
    pub dec_rewards: Decimal256,
    pub last_claimed: Timestamp,
    pub position_weight: Decimal256,
}

// REWARDS (holder_addr, cw20_addr) -> Holder
pub const STAKERS: Map<(&Addr, u128), StakePosition> = Map::new("stakers");
