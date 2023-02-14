use cosmwasm_std::{
    Addr, Decimal, Decimal256, Order, StdResult, Storage, Timestamp, Uint128, Uint64,
};

use cosmwasm_schema::cw_serde;
use cw_storage_plus::{Bound, Item, Map, PrefixBound};

use crate::ContractError;

#[cw_serde]
pub struct State {
    pub global_index: Decimal256,
    pub total_staked: Uint128,
    pub total_weight: Decimal256,
    pub reward_end_time: Timestamp,
    pub total_reward_supply: Uint128,
    pub total_reward_claimed: Uint128,
    pub start_time: Timestamp,
    pub last_updated: Timestamp,
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
        now: u64,
    ) -> StdResult<Vec<Claim>> {
        self.0
            .sub_prefix(address)
            .range(
                store,
                None,
                Some(Bound::inclusive((now + 1, 0))),
                Order::Ascending,
            )
            .map(|x| x.map(|(_, v)| v))
            .collect::<StdResult<Vec<_>>>()
    }

    pub fn remove_mature_claims(
        &self,
        store: &mut dyn Storage,
        address: Addr,
        release_at: u64,
    ) -> Result<(), ContractError> {
        self.0
            .sub_prefix(address.clone())
            .range(
                store,
                Some(Bound::inclusive((release_at, 0))),
                None,
                Order::Ascending,
            )
            .map(|x| x.map(|(k, v)| k))
            .collect::<StdResult<Vec<_>>>()?
            .into_iter()
            .for_each(|k| self.0.remove(store, (address.clone(), k.0, k.1)));
        Ok(())
    }
    pub fn remove_for_release_at(
        &self,
        store: &mut dyn Storage,
        address: Addr,
        release_at: u64,
    ) -> Result<(), ContractError> {
        self.0
            .sub_prefix(address.clone())
            .range(
                store,
                Some(Bound::inclusive((release_at, 0))),
                Some(Bound::exclusive((release_at + 1, 0))),
                Order::Ascending,
            )
            .map(|x| x.map(|(k, v)| k))
            .collect::<StdResult<Vec<_>>>()?
            .into_iter()
            .for_each(|k| self.0.remove(store, (address.clone(), k.0, k.1)));
        Ok(())
    }
}
#[cw_serde]
pub struct Config {
    pub admin: Addr,
    pub stake_token_address: Addr,
    pub reward_token_address: Addr,
    pub force_claim_ratio: Decimal,
    pub fee_collector: Addr,
    pub max_bond_duration: u128,
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
    pub bond_time: Timestamp,
    pub unbond_duration_as_days: u128,
    pub pending_rewards: Uint128,
    pub dec_rewards: Decimal256,
    pub last_claimed: Timestamp,
    pub position_weight: Decimal256,
}

// REWARDS (holder_addr, cw20_addr) -> Holder
pub const STAKERS: Map<(&Addr, u128), StakePosition> = Map::new("stakers");
