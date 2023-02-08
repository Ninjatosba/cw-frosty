use cosmwasm_std::{from_slice, Api};
use cosmwasm_std::{
    to_binary, Addr, BankMsg, Binary, Coin, CosmosMsg, Decimal, Decimal256, Deps, DepsMut, Env,
    Fraction, Isqrt, MessageInfo, Order, Response, StdError, StdResult, Timestamp, Uint128,
    Uint256,
};
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
    AccruedRewardsResponse, ClaimResponse, ConfigResponse, ExecuteMsg, InstantiateMsg,
    ListClaimsResponse, MigrateMsg, QueryMsg, ReceiveMsg, StakerForAllDurationResponse,
    StakerResponse, StateResponse,
};
use crate::state::{
    self, CW20Balance, Claim, Config, StakePosition, State, CLAIMS, CONFIG, STAKERS, STATE,
};
use crate::ContractError;
use cosmwasm_std;
use std::convert::TryInto;
use std::ops::Add;
use std::str::FromStr;

#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    let admin = maybe_addr(deps.api, msg.admin)?.unwrap_or_else(|| info.sender.clone());
    // validate fee_collector address
    let fee_collector_address = deps.api.addr_validate(&msg.fee_collector)?;

    let stake_token_address = deps.api.addr_validate(&msg.stake_token_address)?;

    let reward_token_address = deps.api.addr_validate(&msg.reward_token_address)?;

    let config = Config {
        admin: admin.clone(),
        stake_token_address: stake_token_address,
        reward_token_address: reward_token_address,
        force_claim_ratio: msg.force_claim_ratio,
        fee_collector: fee_collector_address,
    };
    CONFIG.save(deps.storage, &config)?;
    //set state
    let state = State {
        global_index: Decimal256::zero(),
        total_staked: Uint128::zero(),
        total_weight: Decimal256::zero(),
        reward_end_time: env.block.time.plus_seconds(1),
        total_reward_supply: Uint128::zero(),
        total_reward_claimed: Uint128::zero(),
        start_time: env.block.time,
        last_updated: env.block.time,
    };
    STATE.save(deps.storage, &state)?;
    let res = Response::default()
        .add_attribute("method", "instantiate")
        .add_attribute("admin", admin.clone())
        .add_attribute(
            "stake_token_address",
            config.stake_token_address.to_string(),
        )
        .add_attribute(
            "reward_token_address",
            config.reward_token_address.to_string(),
        )
        .add_attribute("force_claim_ratio", config.force_claim_ratio.to_string())
        .add_attribute("fee_collector", config.fee_collector);
    Ok(res)
}

#[cfg(not(feature = "library"))]

pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Receive(receive_message) => execute_receive(deps, env, info, receive_message),
        ExecuteMsg::UpdateRewardIndex {} => execute_update_reward_index(deps, env),
        ExecuteMsg::UpdateStakersReward { address } => {
            execute_update_staker_rewards(deps, env, info, address)
        }
        ExecuteMsg::UnbondStake { amount, duration } => {
            execute_unbond(deps, env, info, amount, duration)
        }
        ExecuteMsg::ClaimUnbounded {} => execute_claim(deps, env, info),
        ExecuteMsg::ReceiveReward {} => execute_receive_reward(deps, env, info),
        ExecuteMsg::UpdateConfig {
            stake_token_address,
            reward_token_address,
            admin,
            fee_collector,
        } => execute_update_config(
            deps,
            env,
            info,
            stake_token_address,
            reward_token_address,
            fee_collector,
            admin,
        ),
        ExecuteMsg::ForceClaim { unbond_time } => execute_force_claim(deps, env, info, unbond_time),
    }
}

// /// Increase global_index according to claimed rewards amount
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
        ReceiveMsg::RewardUpdate { reward_end_time } => {
            fund_reward(deps, env, balance, reward_end_time)
        }
    }
}
pub fn fund_reward(
    deps: DepsMut,
    env: Env,
    balance: CW20Balance,
    reward_end_time: Timestamp,
) -> Result<Response, ContractError> {
    let cfg = CONFIG.load(deps.storage)?;

    if reward_end_time <= env.block.time {
        return Err(ContractError::InvalidRewardEndTime {});
    }

    // check denom
    if balance.denom != cfg.reward_token_address {
        return Err(ContractError::InvalidCw20TokenAddress {});
    }
    let amount = balance.amount;

    let mut state = STATE.load(deps.storage)?;

    // update reward index so that we distrubute latest reward.

    update_reward_index(&mut state, env.block.time)?;
    let unclaimed_reward = state
        .total_reward_supply
        .checked_sub(state.total_reward_claimed)?;
    let new_reward_supply = unclaimed_reward + amount;

    state.total_reward_supply = new_reward_supply;
    state.total_reward_claimed = Uint128::zero();

    state.reward_end_time = reward_end_time;
    // every time we fund reward we reset start time.
    state.start_time = env.block.time;
    STATE.save(deps.storage, &state)?;
    //TODO add responses
    let res = Response::new()
        .add_attribute("action", "fund_reward")
        .add_attribute("amount", amount.to_string())
        .add_attribute("reward_end_time", reward_end_time.to_string());

    Ok(res)
}

pub fn execute_bond(
    deps: DepsMut,
    env: Env,
    balance: CW20Balance,
    sender: Addr,
    duration: u128,
) -> Result<Response, ContractError> {
    let cfg = CONFIG.load(deps.storage)?;
    // check denom
    if balance.denom != cfg.stake_token_address {
        return Err(ContractError::InvalidCw20TokenAddress {});
    }

    let amount = balance.amount;

    if amount.is_zero() {
        return Err(ContractError::NoFund {});
    }
    let mut state = STATE.load(deps.storage)?;
    // look for this address and desired duration in STAKERS
    let mut staker = STAKERS.may_load(deps.storage, (&sender, duration))?;
    match staker {
        Some(mut staker) => {
            update_reward_index(&mut state, env.block.time);
            update_staker_rewards(&mut state, env.block.time, &mut staker)?;
            // when adding to existing staker best way to do is calculate the new weight and add it to total weight after removing old weight.
            staker.staked_amount = staker.staked_amount.add(amount);
            state.total_weight = state
                .total_weight
                .checked_sub(staker.position_weight)?
                .checked_add(
                    Decimal256::from_ratio(duration, Uint128::one())
                        .sqrt()
                        .checked_mul(Decimal256::from_ratio(
                            staker.staked_amount,
                            Uint128::one(),
                        ))?,
                )?;

            STAKERS.save(deps.storage, (&sender, duration), &staker)?;
        }
        None => {
            // create new staker
            update_reward_index(&mut state, env.block.time);
            let position_weight = Decimal256::from_ratio(duration, Uint128::one())
                .sqrt()
                .checked_mul(Decimal256::from_ratio(amount, Uint128::one()))?;

            let staker = StakePosition {
                staked_amount: amount,
                index: state.global_index,
                bond_time: env.block.time,
                unbond_duration_as_days: duration,
                pending_rewards: Uint128::zero(),
                dec_rewards: Decimal256::zero(),
                last_claimed: env.block.time,
                position_weight: position_weight,
            };
            state.total_weight = state.total_weight.add(position_weight);

            STAKERS.save(deps.storage, (&sender, duration), &staker)?;
        }
    }
    state.total_staked = state.total_staked.add(amount);
    STATE.save(deps.storage, &state)?;

    let res = Response::new()
        .add_attribute("action", "bond")
        .add_attribute("sender", sender)
        .add_attribute("amount", amount)
        .add_attribute("duration_day", duration.to_string());

    Ok(res)
}

pub fn execute_update_reward_index(deps: DepsMut, env: Env) -> Result<Response, ContractError> {
    let mut state = STATE.load(deps.storage)?;

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
    let numerator = now
        .seconds()
        .checked_sub(state.last_updated.seconds())
        .unwrap();

    // Time elapsed since start
    let denominator = state
        .reward_end_time
        .seconds()
        .checked_sub(state.start_time.seconds())
        .unwrap_or(1u64);

    let new_dist_balance = state
        .total_reward_supply
        .multiply_ratio(numerator, denominator);

    let divider = state.total_weight;

    let adding_index = Decimal256::from_ratio(new_dist_balance, Uint256::one())
        .checked_div(divider)
        .unwrap_or(Decimal256::zero());

    state.total_reward_claimed = state
        .total_reward_claimed
        .checked_add(new_dist_balance)
        .unwrap_or(Uint128::zero());

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

    STATE.save(deps.storage, &state)?;
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
    println!("global_index: {:?}", state.global_index);
    update_reward_index(state, now)?;
    println!("now: {:?}", now.seconds());
    println!("global_index: {:?}", state.global_index);

    let index_diff = state.global_index - stake_position.index;
    println!("index_diff: {:?}", index_diff);

    let new_distrubuted_reward = index_diff
        .checked_mul(stake_position.position_weight)?
        .checked_add(stake_position.dec_rewards)?;
    let decimals = get_decimals(new_distrubuted_reward)?;

    let rewards_uint128 = (new_distrubuted_reward * Uint256::one())
        .try_into()
        .unwrap_or(Uint128::zero());
    println!("rewards_uint128: {}", rewards_uint128);
    stake_position.dec_rewards = decimals;
    stake_position.pending_rewards = stake_position
        .pending_rewards
        .checked_add(rewards_uint128)?;
    stake_position.index = state.global_index;
    stake_position.last_claimed = now;
    Ok(stake_position.pending_rewards)
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

    let reward_asset = Asset::cw20(config.reward_token_address, rewards);
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

    let unbond_amount = match amount {
        Some(amount) => {
            if staker.staked_amount < amount {
                return Err(ContractError::InsufficientStakedAmount {});
            }
            staker.staked_amount = staker.staked_amount.checked_sub(amount)?;
            state.total_weight = state.total_weight.checked_sub(staker.position_weight)?;
            let position_weight = Decimal256::from_ratio(duration, Uint128::one())
                .sqrt()
                .checked_mul(Decimal256::from_ratio(staker.staked_amount, Uint128::one()))?;
            staker.position_weight = position_weight;
            state.total_weight = state.total_weight.checked_add(staker.position_weight)?;
            STAKERS.save(deps.storage, (&info.sender, duration), &staker)?;
            amount
        }
        None => {
            state.total_weight = state.total_weight.checked_sub(staker.position_weight)?;
            STAKERS.remove(deps.storage, (&info.sender, duration));
            staker.staked_amount
        }
    };
    STATE.save(deps.storage, &state)?;
    let duration_as_sec = days_to_seconds(duration);

    let claim = vec![Claim {
        amount: unbond_amount,
        release_at: env.block.time.plus_seconds(duration_as_sec),
        unbond_at: env.block.time,
    }];
    CLAIMS.save(deps.storage, &info.sender, &claim)?;

    let reward_asset = Asset::cw20(config.reward_token_address, reward);
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
    stake_token_address: Option<String>,
    reward_token_address: Option<String>,
    fee_collector: Option<String>,
    admin: Option<String>,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;
    if info.sender != config.admin {
        return Err(ContractError::Unauthorized {});
    }
    if let Some(stake_token_address) = stake_token_address {
        config.stake_token_address = deps.api.addr_validate(&stake_token_address)?;
    }
    if let Some(reward_token_address) = reward_token_address {
        config.reward_token_address = deps.api.addr_validate(&reward_token_address)?;
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

    let stake_asset = Asset::cw20(config.stake_token_address, total_claim);
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
    let cut_asset = Asset::cw20(config.stake_token_address.clone(), cut_amount);
    let cut_message = cut_asset.transfer_msg(config.fee_collector)?;
    //send claim_amount to user
    let claim_asset = Asset::cw20(config.stake_token_address.clone(), claim_amount);
    let claim_message = claim_asset.transfer_msg(info.sender)?;

    let res = Response::new()
        .add_message(cut_message)
        .add_message(claim_message)
        .add_attribute("action", "force_claim")
        .add_attribute("amount", claim_amount.to_string())
        .add_attribute("cut_amount", cut_amount.to_string());
    Ok(res)
}
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::State {} => to_binary(&query_state(deps, env, msg)?),
        QueryMsg::Config {} => to_binary(&query_config(deps, env, msg)?),
        QueryMsg::StakerForDuration { address, duration } => {
            to_binary(&query_staker_for_duration(env, deps, address, duration)?)
        }
        QueryMsg::StakerForAllDuration { address } => {
            to_binary(&query_staker_for_all_duration(deps, env, address)?)
        }
        QueryMsg::ListClaims { address } => to_binary(&query_list_claims(env, deps, address)?),
    }
}

pub fn query_state(deps: Deps, _env: Env, _msg: QueryMsg) -> StdResult<StateResponse> {
    let state = STATE.load(deps.storage)?;

    Ok(StateResponse {
        global_index: state.global_index,
        total_staked: state.total_staked,
        total_weight: state.total_weight,
        reward_end_time: state.reward_end_time,
        total_reward_supply: state.total_reward_supply,
        total_reward_claimed: state.total_reward_claimed,
        start_time: state.start_time,
        last_updated: state.last_updated,
    })
}

//query config
pub fn query_config(deps: Deps, _env: Env, _msg: QueryMsg) -> StdResult<ConfigResponse> {
    let config = CONFIG.load(deps.storage)?;

    Ok(ConfigResponse {
        reward_token_address: config.reward_token_address.to_string(),
        stake_token_address: config.stake_token_address.to_string(),
        admin: config.admin.to_string(),
        fee_collector: config.fee_collector.to_string(),
        force_claim_ratio: config.force_claim_ratio.to_string(),
    })
}

pub fn query_list_claims(_env: Env, deps: Deps, address: String) -> StdResult<ListClaimsResponse> {
    let addr = deps.api.addr_validate(&address)?;
    let claim = CLAIMS.load(deps.storage, &addr)?;
    let claims: Vec<ClaimResponse> = claim
        .into_iter()
        .map(|claim| ClaimResponse {
            amount: claim.amount,
            release_at: claim.release_at,
            unbond_at: claim.unbond_at,
        })
        .collect();
    Ok(ListClaimsResponse { claims })
}

pub fn query_staker_for_duration(
    _env: Env,
    deps: Deps,
    address: String,
    duration: u128,
) -> StdResult<StakerResponse> {
    let addr = deps.api.addr_validate(&address.as_str())?;
    let staker = STAKERS.load(deps.storage, (&addr, duration))?;

    Ok(StakerResponse {
        staked_amount: staker.staked_amount,
        index: staker.index,
        bond_time: staker.bond_time,
        unbond_duration_as_days: staker.unbond_duration_as_days,
        pending_rewards: staker.pending_rewards,
        dec_rewards: staker.dec_rewards,
        last_claimed: staker.last_claimed,
    })
}
//query all holders list
pub fn query_staker_for_all_duration(
    deps: Deps,
    _env: Env,
    address: String,
) -> StdResult<StakerForAllDurationResponse> {
    let addr = deps.api.addr_validate(&address)?;
    //return all stakers of address
    let positions: Vec<StakerResponse> = STAKERS
        .prefix(&addr)
        .range(deps.storage, None, None, Order::Ascending)
        .map(|item| {
            let (key, value) = item.unwrap();
            let response = StakerResponse {
                staked_amount: value.staked_amount,
                index: value.index,
                bond_time: value.bond_time,
                unbond_duration_as_days: value.unbond_duration_as_days,
                pending_rewards: value.pending_rewards,
                dec_rewards: value.dec_rewards,
                last_claimed: value.last_claimed,
            };
            response
        })
        .collect();

    Ok(StakerForAllDurationResponse {
        positions: positions,
    })
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}
