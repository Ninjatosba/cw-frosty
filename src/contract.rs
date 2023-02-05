use cosmwasm_std::testing::MockQuerier;
use cosmwasm_std::{
    entry_point, to_binary, Addr, BankMsg, Binary, Coin, CosmosMsg, Decimal, Decimal256, Deps,
    DepsMut, Env, Fraction, Isqrt, MessageInfo, Order, Response, StdError, StdResult, Timestamp,
    Uint128, Uint256,
};
use cosmwasm_std::{from_slice, Api};
use cw0::{maybe_addr, PaymentError};
use cw20::{Cw20CoinVerified, Cw20Contract};
use cw20::{Cw20QueryMsg, Cw20ReceiveMsg};
use cw_asset::Asset;
use cw_storage_plus::Bound;

use cw_utils::must_pay;

use serde::de;
use std::time::Duration;
use std::vec;

use crate::helper::{self, days_to_seconds, get_decimals};
use crate::msg::{
    AccruedRewardsResponse, ConfigResponse, ExecuteMsg, HolderResponse, HoldersResponse,
    InstantiateMsg, MigrateMsg, QueryMsg, ReceiveMsg, StateResponse,
};
use crate::state::{
    self, CW20Balance, Claim, Config, StakePosition, State, CLAIMS, CONFIG, STAKERS, STATE,
};
use crate::ContractError;
use cosmwasm_std;
use std::convert::TryInto;
use std::ops::Add;
use std::str::FromStr;
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    let admin = maybe_addr(deps.api, msg.admin)?.unwrap_or_else(|| info.sender.clone());
    // validate fee_collector address
    let fee_collector_address = deps.api.addr_validate(&msg.fee_collector)?;

    let config = Config {
        admin: admin.clone(),
        stake_denom: msg.stake_denom,
        reward_denom: msg.reward_denom,
        force_claim_ratio: msg.force_claim_ratio,
        fee_collector: fee_collector_address,
    };
    CONFIG.save(deps.storage, &config)?;
    //set state
    let state = State {
        global_index: Decimal256::zero(),
        total_staked: Uint128::zero(),
        total_weight: Decimal256::zero(),
        reward_end_time: Timestamp::from_seconds(0),
        total_reward_supply: Uint128::zero(),
        remaining_reward_supply: Uint128::zero(),
        start_time: env.block.time,
        last_updated: env.block.time,
    };
    STATE.save(deps.storage, &state)?;
    let res = Response::default()
        .add_attribute("method", "instantiate")
        .add_attribute("admin", admin.clone())
        .add_attribute("stake_denom", config.stake_denom.to_string())
        .add_attribute("reward_denom", config.reward_denom.to_string())
        .add_attribute("force_claim_ratio", config.force_claim_ratio.to_string())
        .add_attribute("fee_collector", config.fee_collector);
    Ok(res)
}

// #[cfg_attr(not(feature = "library"), entry_point)]
// pub fn execute(
//     deps: DepsMut,
//     env: Env,
//     info: MessageInfo,
//     msg: ExecuteMsg,
// ) -> Result<Response, ContractError> {
//     match msg {
//         ExecuteMsg::FundReward { end_time } => {
//             fund_reward(deps, env, Balance::from(info.funds), end_time)
//         }
//         ExecuteMsg::Receive(receive_message) => execute_receive(deps, env, info, receive_message),
//         ExecuteMsg::Bond { unbonding_duration } => execute_bond(
//             deps,
//             env,
//             Balance::from(info.funds),
//             info.sender,
//             unbonding_duration,
//         ),
//         ExecuteMsg::UpdateRewardIndex {} => execute_update_reward_index(deps, env),
//         ExecuteMsg::UpdateStakersReward { address } => {
//             execute_update_stakers_rewards(deps, env, info, address)
//         }
//         ExecuteMsg::UnbondStake { amount, duration } => {
//             execute_unbond(deps, env, info, amount, duration)
//         }
//         ExecuteMsg::ClaimUnbounded {} => execute_claim(deps, env, info),
//         ExecuteMsg::ReceiveReward {} => execute_receive_reward(deps, env, info),
//         ExecuteMsg::UpdateConfig {
//             staked_token_denom,
//             reward_denom,
//             admin,
//         } => execute_update_config(deps, env, info, staked_token_denom, reward_denom, admin),
//         ExecuteMsg::ForceClaim { unbond_time } => execute_force_claim(deps, env, info, unbond_time),
//     }
// }

// /// Increase global_index according to claimed rewards amount
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute_receive(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    wrapper: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    let msg = from_slice::<ReceiveMsg>(&wrapper.msg)?;
    let config = CONFIG.load(deps.storage)?;
    // TODO: check sender any contract that send cw20 token to this contract can execute this function
    let api = deps.api;
    let balance = CW20Balance {
        denom: info.sender,
        amount: wrapper.amount,
    };
    match msg {
        ReceiveMsg::Bond { duration_day } => execute_bond(
            deps,
            env,
            balance,
            api.addr_validate(&wrapper.sender)?,
            duration_day,
        ),
        ReceiveMsg::RewardUpdate { duration } => fund_reward(deps, env, balance, duration),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn fund_reward(
    deps: DepsMut,
    env: Env,
    balance: CW20Balance,
    duration: Duration,
) -> Result<Response, ContractError> {
    let cfg = CONFIG.load(deps.storage)?;
    // check denom
    if balance.denom != cfg.reward_denom {
        return Err(ContractError::InvalidCw20TokenAddress {});
    }
    let amount = balance.amount;

    let mut state = STATE.load(deps.storage)?;

    let unclaimed_reward = state.remaining_reward_supply;

    state.total_reward_supply = unclaimed_reward + amount;

    state.remaining_reward_supply = state.total_reward_supply;

    state.reward_end_time = state
        .reward_end_time
        .plus_nanos(duration.as_nanos().try_into().unwrap());

    state.start_time = env.block.time;
    STATE.save(deps.storage, &state)?;
    //TODO add responses
    let res = Response::new().add_attribute("action", "fund_reward");

    Ok(res)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute_bond(
    deps: DepsMut,
    env: Env,
    balance: CW20Balance,
    sender: Addr,
    duration: u128,
) -> Result<Response, ContractError> {
    let cfg = CONFIG.load(deps.storage)?;
    // check denom
    if balance.denom != cfg.stake_denom {
        return Err(ContractError::InvalidCw20TokenAddress {});
    }
    let amount = balance.amount;
    let mut state = STATE.load(deps.storage)?;
    // look for this address and desired duration in STAKERS
    let mut staker = STAKERS.may_load(deps.storage, (&sender, duration))?;
    match staker {
        Some(mut staker) => {
            update_reward_index(&mut state, env.block.time);
            update_staker_rewards(&mut state, env.block.time, &mut staker)?;
            staker.staked_amount = staker.staked_amount.add(amount);
            STAKERS.save(deps.storage, (&sender, duration), &staker)?;
        }
        None => {
            // create new staker
            update_reward_index(&mut state, env.block.time);
            let staker = StakePosition {
                staked_amount: amount,
                index: state.global_index,
                bond_time: env.block.time,
                unbond_duration_as_days: duration,
                pending_rewards: Uint128::zero(),
                dec_rewards: Decimal256::zero(),
                last_claimed: env.block.time,
            };
            let weight = Decimal256::from_ratio(duration, Uint128::one()).sqrt();
            state.total_weight = state.total_weight.add(weight);
            STAKERS.save(deps.storage, (&sender, duration), &staker)?;
        }
    }
    state.total_staked = state.total_staked.add(amount);
    STATE.save(deps.storage, &state)?;

    let res = Response::new()
        .add_attribute("action", "bond")
        .add_attribute("sender", sender)
        .add_attribute("amount", amount);

    Ok(res)
}

pub fn execute_update_reward_index(deps: DepsMut, env: Env) -> Result<Response, ContractError> {
    let mut state = STATE.load(deps.storage)?;

    // Zero staking check
    if state.total_staked.is_zero() {
        return Err(ContractError::NoBond {});
    }

    update_reward_index(&mut state, env.block.time)?;

    STATE.save(deps.storage, &state)?;

    let res = Response::new()
        .add_attribute("action", "update_reward_index")
        .add_attribute("new_index", state.global_index.to_string());
    Ok(res)
}

pub fn update_reward_index(state: &mut State, mut now: Timestamp) -> Result<(), ContractError> {
    // If now is passed the end time, set it to the end time
    if now > state.reward_end_time {
        now = state.reward_end_time;
    }
    // Time elapsed since last update
    let numerator = now.minus_seconds(state.last_updated.seconds()).seconds();
    // Time elapsed since start
    let denominator = state.reward_end_time.seconds() - state.start_time.seconds();

    let new_dist_balance = state
        .total_reward_supply
        .multiply_ratio(numerator, denominator);

    let divider = state
        .total_weight
        .checked_mul(Decimal256::from_ratio(state.total_staked, Uint256::one()))?;

    let adding_index = Decimal256::from_ratio(new_dist_balance, Uint256::one())
        .checked_div(divider)
        .unwrap_or(Decimal256::zero());

    state.remaining_reward_supply = state
        .remaining_reward_supply
        .checked_sub(new_dist_balance)?;
    state.global_index = state.global_index.add(adding_index);
    state.last_updated = now;
    Ok(())
}

pub fn execute_update_staker_rewards(
    deps: DepsMut,
    mut env: Env,
    info: MessageInfo,
    address: Option<String>,
) -> Result<Response, ContractError> {
    let mut state = STATE.load(deps.storage)?;
    let addr = maybe_addr(deps.api, address)?.unwrap_or_else(|| info.sender.clone());
    // Zero staking check
    if state.total_staked.is_zero() {
        return Err(ContractError::NoBond {});
    }
    // TODO: Check is its OK
    let rewards: Uint128 = STAKERS
        .range(deps.storage, None, None, Order::Ascending)
        .collect::<StdResult<Vec<_>>>()?
        .into_iter()
        .map(|(_, mut staker)| {
            let reward = update_staker_rewards(&mut state, env.block.time, &mut staker)
                .unwrap_or(Uint128::zero());
            STAKERS
                .save(
                    deps.storage,
                    (&addr, staker.unbond_duration_as_days),
                    &staker,
                )
                .unwrap_or_default();
            reward
        })
        .sum();

    let res = Response::new()
        .add_attribute("action", "update_stakers_rewards")
        .add_attribute("address", addr)
        .add_attribute("rewards", rewards.to_string());
    Ok(res)
}

pub fn update_staker_rewards(
    state: &mut State,
    now: Timestamp,
    stake_position: &mut StakePosition,
) -> Result<Uint128, ContractError> {
    //update reward index
    update_reward_index(state, now)?;

    let position_weight =
        Decimal256::from_ratio(stake_position.unbond_duration_as_days, Uint128::one()).sqrt();

    let index_diff = state.global_index - stake_position.index;

    let multiplier = index_diff.checked_mul(position_weight)?;

    let new_distrubuted_reward =
        Decimal256::from_ratio(stake_position.staked_amount, Uint128::one())
            .checked_mul(multiplier)?
            .checked_add(stake_position.dec_rewards)?;

    let decimals = get_decimals(new_distrubuted_reward)?;

    let rewards_uint128 = (new_distrubuted_reward * Uint256::one())
        .try_into()
        .unwrap_or(Uint128::zero());

    stake_position.dec_rewards = decimals;
    stake_position.pending_rewards = stake_position
        .pending_rewards
        .checked_add(rewards_uint128)?;
    stake_position.index = state.global_index;
    stake_position.last_claimed = now;
    Ok(rewards_uint128)
}

pub fn execute_receive_reward(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    let mut state = STATE.load(deps.storage)?;
    let config = CONFIG.load(deps.storage)?;

    let rewards: Uint128 = STAKERS
        .range(deps.storage, None, None, Order::Ascending)
        .collect::<StdResult<Vec<_>>>()?
        .into_iter()
        .map(|(_, mut staker)| {
            let reward = update_staker_rewards(&mut state, env.block.time, &mut staker).unwrap();
            staker.pending_rewards = Uint128::zero();
            STAKERS
                .save(
                    deps.storage,
                    (&info.sender, staker.unbond_duration_as_days),
                    &staker,
                )
                .unwrap_or_default();
            reward
        })
        .sum();

    let reward_asset = Asset::cw20(config.reward_denom, rewards);
    let reward_msg = reward_asset.transfer_msg(info.sender.clone())?;
    let res = Response::new()
        .add_message(reward_msg)
        .add_attribute("action", "receive_reward")
        .add_attribute("address", info.sender)
        .add_attribute("rewards", rewards.to_string());
    Ok(res)
}

pub fn execute_unbond(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    amount: Option<Uint128>,
    duration: u128,
) -> Result<Response, ContractError> {
    let mut state = STATE.load(deps.storage)?;
    let config = CONFIG.load(deps.storage)?;
    // TODO return error if no position found
    let mut staker = STAKERS.load(deps.storage, (&info.sender, duration))?;

    let reward = update_staker_rewards(&mut state, env.block.time, &mut staker)?;

    staker.pending_rewards = Uint128::zero();

    STAKERS.save(deps.storage, (&info.sender, duration), &staker)?;

    let unbond_amount = match amount {
        Some(amount) => {
            if staker.staked_amount < amount {
                return Err(ContractError::InsufficientStakedAmount {});
            }
            staker.staked_amount = staker.staked_amount.checked_sub(amount)?;
            STAKERS.save(deps.storage, (&info.sender, duration), &staker);
            amount
        }
        None => {
            STAKERS.remove(deps.storage, (&info.sender, duration));
            staker.staked_amount
        }
    };
    let duration_as_sec = days_to_seconds(duration);

    let claim = vec![Claim {
        amount: unbond_amount,
        release_at: env.block.time.plus_seconds(duration_as_sec),
        unbond_at: env.block.time,
    }];
    CLAIMS.save(deps.storage, &info.sender, &claim)?;

    let reward_asset = Asset::cw20(config.reward_denom, reward);
    let reward_msg = reward_asset.transfer_msg(info.sender.clone())?;

    let res = Response::new()
        .add_message(reward_msg)
        .add_attribute("action", "unbond")
        .add_attribute("address", info.sender)
        .add_attribute("amount", amount.unwrap_or_default().to_string())
        .add_attribute("duration", duration.to_string());

    Ok(res)
}
//update config
pub fn execute_update_config(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    stake_denom: Option<String>,
    reward_denom: Option<String>,
    fee_collector: Option<String>,
    admin: Option<String>,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;
    if info.sender != config.admin {
        return Err(ContractError::Unauthorized {});
    }
    if let Some(stake_denom) = stake_denom {
        config.stake_denom = deps.api.addr_validate(&stake_denom)?;
    }
    if let Some(reward_denom) = reward_denom {
        config.reward_denom = deps.api.addr_validate(&reward_denom)?;
    }
    if let Some(fee_collector) = fee_collector {
        config.fee_collector = deps.api.addr_validate(&fee_collector)?;
    }
    if let Some(admin) = admin {
        config.admin = deps.api.addr_validate(&admin)?;
    }
    CONFIG.save(deps.storage, &config)?;
    Ok(Response::default())
}

pub fn execute_claim(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    let claim = CLAIMS.load(deps.storage, &info.sender)?;
    let config = CONFIG.load(deps.storage)?;
    if claim.is_empty() {
        return Err(ContractError::NoClaim {});
    }
    //filter claim vector and make another vector which contains only mature claims
    let mature_claims: Vec<Claim> = claim
        .clone()
        .into_iter()
        .filter(|claim| claim.release_at <= env.block.time)
        .collect();
    //if no mature claims return error
    if mature_claims.is_empty() {
        return Err(ContractError::WaitUnbonding {});
    }
    //sum mature claims
    let mut total_claim: Uint128 = Uint128::zero();
    for claim in mature_claims.iter() {
        total_claim += claim.amount;
    }
    //remove mature claims from claim vector
    let mut new_claims: Vec<Claim> = claim
        .into_iter()
        .filter(|claim| claim.release_at > env.block.time)
        .collect();

    //save new claim vector
    CLAIMS.save(deps.storage, &info.sender, &new_claims)?;

    let stake_asset = Asset::cw20(config.stake_denom, total_claim);
    let asset_message = stake_asset.transfer_msg(info.sender)?;

    let res = Response::new()
        .add_message(asset_message)
        .add_attribute("action", "claim")
        .add_attribute("amount", total_claim.to_string());
    Ok(res)
}

pub fn execute_force_claim(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    release_at: Timestamp,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let claim = CLAIMS.load(deps.storage, &info.sender)?;
    if claim.is_empty() {
        return Err(ContractError::NoClaim {});
    }
    //find desired claim if not found return error
    let desired_claim: Claim = claim
        .clone()
        .into_iter()
        .find(|claim| claim.release_at == release_at)
        .ok_or(ContractError::NoClaimForTimestamp {})?;

    //remove desired claim from claim vector
    let mut new_claims: Vec<Claim> = claim
        .clone()
        .into_iter()
        .filter(|claim| claim.release_at != release_at)
        .collect();
    //save new claim vector
    CLAIMS.save(deps.storage, &info.sender, &new_claims)?;

    let remaning_time = desired_claim
        .release_at
        .minus_seconds(env.block.time.seconds())
        .seconds();

    let total_unbond_duration = desired_claim
        .release_at
        .minus_seconds(desired_claim.unbond_at.seconds())
        .seconds();
    //cut_ratio = force_claim_ratio * (remaning_time / total_unbond_duration)
    let cut_ratio = config
        .force_claim_ratio
        .checked_mul(Decimal::from_ratio(remaning_time, total_unbond_duration))?;
    //cut_amount = desired_claim.amount * cut_ratio
    let cut_amount = desired_claim
        .amount
        .multiply_ratio(cut_ratio.numerator(), cut_ratio.denominator());
    //claim_amount = desired_claim.amount - cut_amount
    let claim_amount = desired_claim.amount.checked_sub(cut_amount)?;
    //send cut_amount to fee_collector
    let cut_asset = Asset::cw20(config.stake_denom.clone(), cut_amount);
    let cut_message = cut_asset.transfer_msg(config.fee_collector)?;
    //send claim_amount to user
    let claim_asset = Asset::cw20(config.stake_denom.clone(), claim_amount);
    let claim_message = claim_asset.transfer_msg(info.sender)?;

    let res = Response::new()
        .add_message(cut_message)
        .add_message(claim_message)
        .add_attribute("action", "force_claim")
        .add_attribute("amount", claim_amount.to_string())
        .add_attribute("cut_amount", cut_amount.to_string());
    Ok(res)
}
// // #[cfg_attr(not(feature = "library"), entry_point)]
// // pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
// //     match msg {
// //         QueryMsg::State {} => to_binary(&query_state(deps, env, msg)?),
// //         QueryMsg::AccruedRewards { address } => {
// //             to_binary(&query_accrued_rewards(env, deps, address)?)
// //         }
// //         QueryMsg::Holder { address } => to_binary(&query_holder(env, deps, address)?),
// //         QueryMsg::Config {} => to_binary(&query_config(deps, env, msg)?),
// //         QueryMsg::Holders { start_after, limit } => {
// //             to_binary(&query_holders(deps, env, start_after, limit)?)
// //         }
// //     }
// // }

// // pub fn query_state(deps: Deps, _env: Env, _msg: QueryMsg) -> StdResult<StateResponse> {
// //     let state = STATE.load(deps.storage)?;

// //     Ok(StateResponse {
// //         total_staked: state.total_staked,
// //         global_index: state.global_index,
// //         prev_reward_balance: state.prev_reward_balance,
// //     })
// // }

// // //query config
// // pub fn query_config(deps: Deps, _env: Env, _msg: QueryMsg) -> StdResult<ConfigResponse> {
// //     let config = CONFIG.load(deps.storage)?;

// //     Ok(ConfigResponse {
// //         staked_token_denom: config.staked_token_denom,
// //         reward_denom: config.reward_denom,
// //         admin: config.admin.into_string(),
// //     })
// // }

// // pub fn query_accrued_rewards(
// //     _env: Env,
// //     deps: Deps,
// //     address: String,
// // ) -> StdResult<AccruedRewardsResponse> {
// //     let addr = deps.api.addr_validate(&address.as_str())?;
// //     let holder = HOLDERS.load(deps.storage, &addr)?;

// //     Ok(AccruedRewardsResponse {
// //         rewards: holder.pending_rewards,
// //     })
// // }

// // pub fn query_holder(_env: Env, deps: Deps, address: String) -> StdResult<HolderResponse> {
// //     let holder: Holder = HOLDERS.load(deps.storage, &deps.api.addr_validate(address.as_str())?)?;
// //     Ok(HolderResponse {
// //         address: address,
// //         balance: holder.balance,
// //         index: holder.index,
// //         pending_rewards: holder.pending_rewards,
// //         dec_rewards: holder.dec_rewards,
// //     })
// // }

// // const MAX_LIMIT: u32 = 30;
// // const DEFAULT_LIMIT: u32 = 10;
// // //query all holders list
// // pub fn query_holders(
// //     deps: Deps,
// //     _env: Env,
// //     start_after: Option<String>,
// //     limit: Option<u32>,
// // ) -> StdResult<HoldersResponse> {
// //     let addr = maybe_addr(deps.api, start_after)?;
// //     let start = addr.as_ref().map(Bound::exclusive);
// //     let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
// //     let holders: StdResult<Vec<HolderResponse>> = HOLDERS
// //         .range(deps.storage, start, None, Order::Ascending)
// //         .take(limit)
// //         .map(|item| {
// //             let (addr, holder) = item?;
// //             let holder_response = HolderResponse {
// //                 address: addr.to_string(),
// //                 balance: holder.balance,
// //                 index: holder.index,
// //                 pending_rewards: holder.pending_rewards,
// //                 dec_rewards: holder.dec_rewards,
// //             };
// //             Ok(holder_response)
// //         })
// //         .collect();

// //     Ok(HoldersResponse { holders: holders? })
// // }

// // #[cfg_attr(not(feature = "library"), entry_point)]
// // pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
// //     Ok(Response::default())
// // }
