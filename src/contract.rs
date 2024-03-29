use cosmwasm_std::{
    entry_point, to_binary, Binary, Decimal, Decimal256, Deps, DepsMut, Env, Fraction, MessageInfo,
    Order, Response, StdResult, Timestamp, Uint128, Uint256,
};
use cosmwasm_std::{from_slice, CosmosMsg};
use cw0::maybe_addr;

use cw20::Cw20ReceiveMsg;
use cw_asset::Asset;

use crate::helper::{days_to_seconds, get_decimals};
use crate::msg::{
    ClaimResponse, ConfigResponse, ExecuteMsg, InstantiateMsg, ListClaimsResponse, MigrateMsg,
    QueryMsg, ReceiveMsg, StakerForAllDurationResponse, StakerResponse, StateResponse,
};
use crate::state::{
    CW20Balance, Claim, Claims, Config, StakePosition, State, CLAIMS_KEY, CONFIG, STAKERS, STATE,
};
use crate::ContractError;
use cosmwasm_std;

use std::convert::TryInto;
use std::ops::Add;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    // validate admin address
    let admin = maybe_addr(deps.api, msg.admin)?.unwrap_or_else(|| info.sender.clone());
    // validate fee_collector address
    let fee_collector_address = deps.api.addr_validate(&msg.fee_collector)?;
    // validate stake_token_address
    let stake_token_address = deps.api.addr_validate(&msg.stake_token_address)?;
    // validate reward_token_address
    let reward_token_address = deps.api.addr_validate(&msg.reward_token_address)?;
    // validate max_bond_duration
    if msg.max_bond_duration < 1 {
        return Err(ContractError::InvalidMaxBondDuration {});
    }
    if (msg.force_claim_ratio < Decimal::zero()) || (msg.force_claim_ratio >= Decimal::one()) {
        return Err(ContractError::InvalidForceClaimRatio {});
    }

    let config = Config {
        admin: admin.clone(),
        stake_token_address,
        reward_token_address,
        force_claim_ratio: msg.force_claim_ratio,
        fee_collector: fee_collector_address,
        max_bond_duration: msg.max_bond_duration,
        reward_per_second: Uint128::zero(),
    };
    CONFIG.save(deps.storage, &config)?;
    //set state
    let state = State {
        global_index: Decimal256::zero(),
        total_staked: Uint128::zero(),
        total_weight: Decimal256::zero(),
        total_reward_claimed: Uint128::zero(),
        last_updated: env.block.time,
    };
    STATE.save(deps.storage, &state)?;
    let res = Response::default()
        .add_attribute("method", "instantiate")
        .add_attribute("admin", admin)
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

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Receive(receive_message) => execute_receive(deps, env, info, receive_message),
        ExecuteMsg::UpdateRewardIndex {} => execute_update_reward_index(deps, env),
        ExecuteMsg::UpdateStakerRewards { address } => {
            execute_update_staker_rewards(deps, env, info, address)
        }
        ExecuteMsg::UnbondStake {
            amount,
            duration_as_days,
        } => execute_unbond(deps, env, info, amount, duration_as_days),
        ExecuteMsg::ClaimUnbonded {} => execute_claim(deps, env, info),
        ExecuteMsg::ReceiveReward {} => execute_receive_reward(deps, env, info),
        ExecuteMsg::UpdateConfig {
            admin,
            fee_collector,
            force_claim_ratio,
        } => execute_update_config(deps, env, info, force_claim_ratio, fee_collector, admin),
        ExecuteMsg::ForceClaim { release_at } => execute_force_claim(deps, env, info, release_at),
        ExecuteMsg::SetRewardPerSecond { reward_per_second } => {
            execute_set_reward_per_second(deps, env, info, reward_per_second)
        }
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
    let api = deps.api;
    let balance = CW20Balance {
        denom: info.sender,
        amount: wrapper.amount,
        sender: api.addr_validate(&wrapper.sender)?,
    };
    match msg {
        ReceiveMsg::Bond { duration_day } => execute_bond(deps, env, balance, duration_day),
    }
}

pub fn execute_bond(
    deps: DepsMut,
    env: Env,
    balance: CW20Balance,
    duration: u128,
) -> Result<Response, ContractError> {
    let cfg = CONFIG.load(deps.storage)?;
    // check denom
    if balance.denom != cfg.stake_token_address {
        return Err(ContractError::InvalidCw20TokenAddress {});
    }
    // check duration
    if duration < 1 || duration > cfg.max_bond_duration {
        return Err(ContractError::InvalidBondDuration {});
    }

    let amount = balance.amount;

    if amount.is_zero() {
        return Err(ContractError::NoFund {});
    }
    let mut state = STATE.load(deps.storage)?;
    // look for this address and desired duration in STAKERS
    let staker = STAKERS.may_load(deps.storage, (&balance.sender, duration))?;
    match staker {
        Some(mut staker) => {
            update_staker_rewards(&mut state, env.block.time, &mut staker, cfg)?;
            // add to existing staker
            staker.staked_amount = staker.staked_amount.add(amount);
            // update total weight. Its a bit tricky to update total weight so i remove the old weight and add new weight.
            let _new_weight = Decimal256::from_ratio(duration, Uint128::one())
                .sqrt()
                .checked_mul(Decimal256::from_ratio(staker.staked_amount, Uint128::one()))?;
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
            staker.position_weight = Decimal256::from_ratio(duration, Uint128::one())
                .sqrt()
                .checked_mul(Decimal256::from_ratio(staker.staked_amount, Uint128::one()))?;

            STAKERS.save(deps.storage, (&balance.sender, duration), &staker)?;
        }
        None => {
            // create new staker
            update_reward_index(&mut state, env.block.time, cfg)?;
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
                position_weight,
            };
            state.total_weight = state.total_weight.add(position_weight);

            STAKERS.save(deps.storage, (&balance.sender, duration), &staker)?;
        }
    }
    state.total_staked = state.total_staked.add(amount);
    STATE.save(deps.storage, &state)?;

    let res = Response::new()
        .add_attribute("action", "bond")
        .add_attribute("sender", balance.sender)
        .add_attribute("amount", amount)
        .add_attribute("duration_day", duration.to_string());

    Ok(res)
}

pub fn execute_update_reward_index(deps: DepsMut, env: Env) -> Result<Response, ContractError> {
    let mut state = STATE.load(deps.storage)?;
    let config = CONFIG.load(deps.storage)?;

    update_reward_index(&mut state, env.block.time, config)?;

    STATE.save(deps.storage, &state)?;

    let res = Response::new()
        .add_attribute("action", "update_reward_index")
        .add_attribute("new_index", state.global_index.to_string())
        .add_attribute(
            "total_reward_claimed",
            state.total_reward_claimed.to_string(),
        );
    Ok(res)
}

pub fn update_reward_index(
    state: &mut State,
    now: Timestamp,
    config: Config,
) -> Result<(), ContractError> {
    // new distribution balance = (now - last_updated) * reward_per_second
    let seconds_since_last_updated =
        Uint128::from(now.seconds().saturating_sub(state.last_updated.seconds()));
    let new_dist_balance = seconds_since_last_updated.checked_mul(config.reward_per_second)?;

    let divider = state.total_weight;
    // adding index = new distribution balance / total weight
    let adding_index = Decimal256::from_ratio(new_dist_balance, Uint256::one())
        .checked_div(divider)
        .unwrap_or(Decimal256::zero());
    if !state.total_weight.is_zero() {
        state.total_reward_claimed = state.total_reward_claimed.checked_add(new_dist_balance)?;
        state.global_index = state.global_index.add(adding_index);
    }
    state.last_updated = now;
    Ok(())
}

pub fn execute_update_staker_rewards(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    address: Option<String>,
) -> Result<Response, ContractError> {
    let mut state = STATE.load(deps.storage)?;
    let config = CONFIG.load(deps.storage)?;
    let addr = maybe_addr(deps.api, address)?.unwrap_or_else(|| info.sender.clone());
    // Zero staking check
    if state.total_staked.is_zero() {
        return Err(ContractError::NoBond {});
    }
    // stakers rewards are updated for every duration and current rewards summed to return response
    let rewards: Uint128 = STAKERS
        .range(deps.storage, None, None, Order::Ascending)
        .collect::<StdResult<Vec<_>>>()?
        .into_iter()
        .filter(|(staker, _)| staker.0 == addr)
        .map(|(_, mut staker)| {
            let reward =
                update_staker_rewards(&mut state, env.block.time, &mut staker, config.clone())
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
    config: Config,
) -> Result<Uint128, ContractError> {
    //update reward index
    update_reward_index(state, now, config)?;

    let index_diff = state.global_index - stake_position.index;
    // new distributed reward = index diff * position weight + dec rewards
    let new_distributed_reward = index_diff
        .checked_mul(stake_position.position_weight)?
        .checked_add(stake_position.dec_rewards)?;
    // decimals are used to store the remainder of the division
    let decimals = get_decimals(new_distributed_reward)?;

    let rewards_uint128 = (new_distributed_reward * Uint256::one())
        .try_into()
        .unwrap_or(Uint128::zero());
    stake_position.dec_rewards = decimals;
    stake_position.pending_rewards = stake_position
        .pending_rewards
        .checked_add(rewards_uint128)?;
    // update stakers index
    stake_position.index = state.global_index;
    // update last claimed time. This is used to return data for the reward calculation
    stake_position.last_claimed = now;
    Ok(stake_position.pending_rewards)
}

pub fn execute_receive_reward(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    let mut state = STATE.load(deps.storage)?;
    let config = CONFIG.load(deps.storage)?;

    let rewards: Uint128 = STAKERS
        .prefix(&info.sender)
        .range(deps.storage, None, None, Order::Ascending)
        .collect::<StdResult<Vec<_>>>()?
        .into_iter()
        .map(|(_, mut staker)| {
            let reward =
                update_staker_rewards(&mut state, env.block.time, &mut staker, config.clone())
                    .unwrap();
            // set pending rewards to zero.
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
    STATE.save(deps.storage, &state)?;
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
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    amount: Option<Uint128>,
    duration_as_days: u128,
) -> Result<Response, ContractError> {
    let mut state = STATE.load(deps.storage)?;
    let config = CONFIG.load(deps.storage)?;

    let mut staker = STAKERS.load(deps.storage, (&info.sender, duration_as_days))?;
    // rewards for desired duration is updated and pending rewards are set to zero
    let reward = update_staker_rewards(&mut state, env.block.time, &mut staker, config.clone())?;
    staker.pending_rewards = Uint128::zero();

    let unbond_amount = match amount {
        Some(amount) => {
            if staker.staked_amount < amount {
                return Err(ContractError::InsufficientStakedAmount {});
            }
            staker.staked_amount = staker.staked_amount.checked_sub(amount)?;
            state.total_weight = state.total_weight.checked_sub(staker.position_weight)?;
            let position_weight = Decimal256::from_ratio(duration_as_days, Uint128::one())
                .sqrt()
                .checked_mul(Decimal256::from_ratio(staker.staked_amount, Uint128::one()))?;
            staker.position_weight = position_weight;
            state.total_weight = state.total_weight.checked_add(staker.position_weight)?;
            STAKERS.save(deps.storage, (&info.sender, duration_as_days), &staker)?;
            amount
        }
        None => {
            state.total_weight = state.total_weight.checked_sub(staker.position_weight)?;
            STAKERS.remove(deps.storage, (&info.sender, duration_as_days));
            staker.staked_amount
        }
    };
    state.total_staked = state.total_staked.checked_sub(unbond_amount)?;
    STATE.save(deps.storage, &state)?;
    let duration_as_sec = days_to_seconds(duration_as_days);

    let release_at = env.block.time.plus_seconds(duration_as_sec);
    let claim = Claim {
        amount: unbond_amount,
        release_at,
        unbond_at: env.block.time,
    };

    Claims::new(CLAIMS_KEY).save(
        deps.storage,
        info.sender.clone(),
        release_at.seconds(),
        &claim,
    )?;

    let mut messages: Vec<CosmosMsg> = vec![];
    let reward_asset = Asset::cw20(config.reward_token_address, reward);
    let reward_msg = reward_asset.transfer_msg(info.sender.clone())?;

    if reward > Uint128::zero() {
        messages.push(reward_msg);
    }

    let res = Response::new()
        .add_messages(messages)
        .add_attribute("action", "unbond")
        .add_attribute("address", info.sender)
        .add_attribute("amount", unbond_amount)
        .add_attribute("duration", duration_as_days.to_string());

    Ok(res)
}
//update config
pub fn execute_update_config(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    force_claim_ratio: Option<Decimal>,
    fee_collector: Option<String>,
    admin: Option<String>,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;
    if info.sender != config.admin {
        return Err(ContractError::Unauthorized {});
    }
    if let Some(force_claim_ratio) = force_claim_ratio {
        config.force_claim_ratio = force_claim_ratio;
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
    let config = CONFIG.load(deps.storage)?;
    let claims = Claims::new(CLAIMS_KEY);
    // load mature claims where release_at < now using second key.
    let mature_claims: Vec<Claim> =
        claims.load_mature_claims(deps.storage, info.sender.clone(), env.block.time.seconds())?;
    // if no mature claims return error
    if mature_claims.is_empty() {
        return Err(ContractError::NoMatureClaim {});
    }
    // sum mature claims
    let total_claim: Uint128 = mature_claims.into_iter().map(|c| c.amount).sum();

    // remove mature claims from storage
    claims.remove_mature_claims(deps.storage, info.sender.clone(), env.block.time.seconds())?;

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
    let mut claims =
        Claims::new(CLAIMS_KEY).load(deps.storage, info.sender.clone(), release_at.seconds())?;

    if claims.is_empty() {
        return Err(ContractError::NoClaimForTimestamp {});
    }

    if release_at.seconds() < env.block.time.seconds() {
        return Err(ContractError::InvalidReleaseTime {});
    }

    let remaining_time = release_at.minus_seconds(env.block.time.seconds()).seconds();
    let mut total_fee: Uint128 = Uint128::zero();
    let mut total_claim_amount: Uint128 = Uint128::zero();
    for c in claims.iter_mut() {
        let total_unbond_duration = c.release_at.minus_seconds(c.unbond_at.seconds()).seconds();
        let cut_ratio = config
            .force_claim_ratio
            .checked_mul(Decimal::from_ratio(remaining_time, total_unbond_duration))?;
        let cut_amount = c
            .amount
            .multiply_ratio(cut_ratio.numerator(), cut_ratio.denominator());

        let claim_amount = c.amount.checked_sub(cut_amount)?;
        total_fee = total_fee.checked_add(cut_amount)?;
        total_claim_amount = total_claim_amount.checked_add(claim_amount)?;
    }

    //send cut_amount to fee_collector
    let fee_asset = Asset::cw20(config.stake_token_address.clone(), total_fee);
    let fee_message = fee_asset.transfer_msg(config.fee_collector)?;
    //send claim_amount to user
    let claim_asset = Asset::cw20(config.stake_token_address, total_claim_amount);
    let claim_message = claim_asset.transfer_msg(info.sender.clone())?;

    //remove claim from storage
    Claims::new(CLAIMS_KEY).remove_for_release_at(
        deps.storage,
        info.sender,
        release_at.seconds(),
    )?;
    let res = Response::new()
        .add_message(fee_message)
        .add_message(claim_message)
        .add_attribute("action", "force_claim")
        .add_attribute("amount", total_claim_amount.to_string())
        .add_attribute("cut_amount", total_fee.to_string());
    Ok(res)
}

pub fn execute_set_reward_per_second(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    reward_per_second: Uint128,
) -> Result<Response, ContractError> {
    let mut state = STATE.load(deps.storage)?;
    let mut config = CONFIG.load(deps.storage)?;

    if info.sender != config.admin {
        return Err(ContractError::Unauthorized {});
    };
    if reward_per_second <= Uint128::zero() {
        return Err(ContractError::InvalidRewardPerSecond {});
    };
    update_reward_index(&mut state, env.block.time, config.clone())?;
    STATE.save(deps.storage, &state)?;
    config.reward_per_second = reward_per_second;
    CONFIG.save(deps.storage, &config)?;
    Ok(Response::default()
        .add_attribute("action", "set_reward_per_second".to_string())
        .add_attribute("reward_per_second", reward_per_second.to_string()))
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
        total_reward_claimed: state.total_reward_claimed,
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
        reward_per_second: config.reward_per_second,
    })
}

pub fn query_list_claims(_env: Env, deps: Deps, address: String) -> StdResult<ListClaimsResponse> {
    let addr = deps.api.addr_validate(&address)?;
    let claim = Claims::new(CLAIMS_KEY).load_all(deps.storage, addr)?;
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
    let addr = deps.api.addr_validate(address.as_str())?;
    let staker = STAKERS.load(deps.storage, (&addr, duration))?;

    Ok(StakerResponse {
        staked_amount: staker.staked_amount,
        index: staker.index,
        bond_time: staker.bond_time,
        unbond_duration_as_days: staker.unbond_duration_as_days,
        pending_rewards: staker.pending_rewards,
        dec_rewards: staker.dec_rewards,
        last_claimed: staker.last_claimed,
        position_weight: staker.position_weight,
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
            let (_key, value) = item.unwrap();

            StakerResponse {
                staked_amount: value.staked_amount,
                index: value.index,
                bond_time: value.bond_time,
                unbond_duration_as_days: value.unbond_duration_as_days,
                pending_rewards: value.pending_rewards,
                dec_rewards: value.dec_rewards,
                last_claimed: value.last_claimed,
                position_weight: value.position_weight,
            }
        })
        .collect();

    Ok(StakerForAllDurationResponse { positions })
}

#[cfg(not(feature = "library"))]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}
