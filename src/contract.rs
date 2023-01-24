use cosmwasm_std::testing::MockQuerier;
use cosmwasm_std::{
    entry_point, to_binary, Addr, BankMsg, Binary, Coin, CosmosMsg, Decimal256, Deps, DepsMut, Env,
    Fraction, Isqrt, MessageInfo, Order, Response, StdError, StdResult, Timestamp, Uint128,
    Uint256,
};
use cosmwasm_std::{from_slice, Api};
use cw0::{maybe_addr, PaymentError};
use cw20::{Balance, Cw20CoinVerified, Cw20Contract, Denom};
use cw20::{Cw20QueryMsg, Cw20ReceiveMsg};
use cw_asset::Asset;
use cw_storage_plus::Bound;

use cw_utils::must_pay;

use serde::de;
use std::time::Duration;

use crate::msg::{
    AccruedRewardsResponse, ConfigResponse, ExecuteMsg, HolderResponse, HoldersResponse,
    InstantiateMsg, MigrateMsg, QueryMsg, ReceiveMsg, StateResponse,
};
use crate::state::{
    self, Claim, Config, StakePosition, State, CLAIMS, CONFIG, STAKEPOSITIONS, STATE,
};
use crate::ContractError;
use cosmwasm_std;
use std::convert::TryInto;
use std::fmt::{format, Debug, Display};
use std::ops::Add;
use std::str::FromStr;
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    let admin = match msg.admin {
        Some(admin) => deps.api.addr_validate(&admin)?,
        None => info.sender.clone(),
    };
    let config = Config {
        admin: admin.clone(),
        stake_denom: msg.stake_denom,
        reward_denom: msg.reward_denom,
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

    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::FundReward { end_time } => {
            fund_reward(deps, env, Balance::from(info.funds), end_time)
        }
        ExecuteMsg::Receive(receive_message) => execute_receive(deps, env, info, receive_message),
        ExecuteMsg::Bond { unbonding_duration } => execute_bond(
            deps,
            env,
            Balance::from(info.funds),
            info.sender,
            unbonding_duration,
        ),
        ExecuteMsg::UpdateRewardIndex {} => execute_update_reward_index(deps, env),
        ExecuteMsg::UpdateHoldersReward { address } => {
            execute_update_holders_rewards(deps, env, info, address)
        }
        ExecuteMsg::UnboundStake {} => execute_claim_reward(deps, env, info),
        ExecuteMsg::WithdrawUnboundedStake { amount } => execute_withdraw(deps, env, info, amount),
        ExecuteMsg::ReceiveReward {} => execute_receive_reward(deps, env, info),
        // ExecuteMsg::UpdateConfig {
        //     staked_token_denom,
        //     reward_denom,
        //     admin,
        // } => execute_update_config(deps, env, info, staked_token_denom, reward_denom, admin),
    }
}

/// Increase global_index according to claimed rewards amount
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute_receive(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    wrapper: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    // info.sender is the address of the cw20 contract (that re-sent this message).
    // wrapper.sender is the address of the user that requested the cw20 contract to send this.
    // This cannot be fully trusted (the cw20 contract can fake it), so only use it for actions
    // in the address's favor (like paying/bonding tokens, not withdrawls)
    let msg = from_slice::<ReceiveMsg>(&wrapper.msg)?;
    let config = CONFIG.load(deps.storage)?;

    let api = deps.api;
    let balance = Balance::Cw20(Cw20CoinVerified {
        address: info.sender,
        amount: wrapper.amount,
    });
    match msg {
        ReceiveMsg::Bond { duration } => execute_bond(
            deps,
            env,
            balance,
            api.addr_validate(&wrapper.sender)?,
            duration,
        ),
        ReceiveMsg::RewardUpdate { duration } => fund_reward(deps, env, balance, duration),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn fund_reward(
    deps: DepsMut,
    env: Env,
    amount: Balance,
    duration: Duration,
) -> Result<Response, ContractError> {
    let cfg = CONFIG.load(deps.storage)?;

    let amount = match (&cfg.reward_denom, &amount) {
        (Denom::Cw20(want), Balance::Cw20(have)) => {
            if want == &have.address {
                Ok(have.amount)
            } else {
                Err(ContractError::DenomNotSupported {})
            }
        }
        (Denom::Native(want), Balance::Native(have)) => {
            if have.into_vec().len() != 1 {
                return Err(ContractError::MultipleTokensSent {});
            }
            if want == &have.into_vec()[0].denom.to_string() {
                Ok(have.into_vec()[0].amount)
            } else {
                Err(ContractError::DenomNotSupported {})
            }
        }
        _ => Err(ContractError::DenomNotSupported {}),
    }?;

    let mut state = STATE.load(deps.storage)?;

    let unclaimed_reward = state.remaining_reward_supply;

    state.total_reward_supply = unclaimed_reward + amount;

    state.remaining_reward_supply = unclaimed_reward + amount;

    state.reward_end_time = state
        .reward_end_time
        .plus_nanos(duration.as_nanos().try_into().unwrap());

    STATE.save(deps.storage, &state)?;
    //TODO add responses
    let res = Response::new().add_attribute("action", "fund_reward");

    Ok(res)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute_bond(
    mut deps: DepsMut,
    env: Env,
    amount: Balance,
    sender: Addr,
    duration: Duration,
) -> Result<Response, ContractError> {
    let cfg = CONFIG.load(deps.storage)?;

    let amount = match (&cfg.stake_denom, &amount) {
        (Denom::Cw20(want), Balance::Cw20(have)) => {
            if want == &have.address {
                Ok(have.amount)
            } else {
                Err(ContractError::DenomNotSupported {})
            }
        }
        (Denom::Native(want), Balance::Native(have)) => {
            if have.into_vec().len() != 1 {
                return Err(ContractError::MultipleTokensSent {});
            }
            if want == &have.into_vec()[0].denom.to_string() {
                Ok(have.into_vec()[0].amount)
            } else {
                Err(ContractError::DenomNotSupported {})
            }
        }
        _ => Err(ContractError::DenomNotSupported {}),
    }?;

    let mut state = STATE.load(deps.storage)?;
    let mut staker = STAKEPOSITIONS.may_load(deps.storage, &sender)?;

    //Match if any staker in storage else define new staker

    match staker {
        None => {
            update_reward_index(&mut state, cfg, deps, env);
            let staker = StakePosition {
                staked_amount: amount,
                pending_rewards: Uint128::zero(),
                index: state.global_index,
                dec_rewards: Decimal256::zero(),
                unbond_duration: duration,
                bond_time: env.block.time,
                last_claimed: env.block.time,
            };
            let mut staker = vec![staker];
            state.total_weight +=
                Decimal256::from_str(&(duration.as_nanos() as f64).sqrt().to_string())?;
            STAKEPOSITIONS.save(deps.storage, &sender, &staker)?;
        }
        Some(mut staker) => {
            //filter stakeposition vector for given duration
            if let Some(stakeposition) = staker
                .iter_mut()
                .find(|stakeposition| stakeposition.unbond_duration == duration)
            {
                update_reward_index(&mut state, cfg, deps, env)?;
                update_stakers_rewards(deps, &mut state, env, staker)?;
                stakeposition.staked_amount = stakeposition.staked_amount.add(amount);
                stakeposition.index = state.global_index;
                STAKEPOSITIONS.save(deps.storage, &sender, &staker)?;
            } else {
                let stakeposition = StakePosition {
                    staked_amount: amount,
                    pending_rewards: Uint128::zero(),
                    index: state.global_index,
                    dec_rewards: Decimal256::zero(),
                    unbond_duration: duration,
                    bond_time: env.block.time,
                    last_claimed: env.block.time,
                };
                staker.push(stakeposition);
                STAKEPOSITIONS.save(deps.storage, &sender, &staker)?;
                //Isqrt returns integer we cant use that
                state.total_weight +=
                    Decimal256::from_str(&(duration.as_nanos() as f64).sqrt().to_string())?;
            }
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
    let config = CONFIG.load(deps.storage)?;

    // Zero staking check
    if state.total_staked.is_zero() {
        return Err(ContractError::NoBond {});
    }

    update_reward_index(&mut state, config, deps, env)?;

    let res = Response::new()
        .add_attribute("action", "update_reward_index")
        .add_attribute("new_index", state.global_index.to_string());
    Ok(res)
}

pub fn update_reward_index(
    state: &mut State,
    config: Config,
    mut deps: DepsMut,
    env: Env,
) -> Result<(), ContractError> {
    // Zero staking check
    if state.total_staked.is_zero() {
        return Err(ContractError::NoBond {});
    }
    let state = STATE.load(deps.storage)?;

    //TODO: Check if denomination is correct
    let numerator = env.block.time.nanos() - state.last_updated.nanos();
    let denominator = state.reward_end_time.nanos() - state.start_time.nanos();
    let remaining_ratio = Decimal256::from_ratio(numerator, denominator);

    let new_dist_balance = state
        .total_reward_supply
        .multiply_ratio(numerator, denominator);
    let total_weight = state.total_weight;
    // TODO check if its good to use this way
    let adding_index = Decimal256::from_ratio(
        state
            .total_weight
            .numerator()
            .checked_mul(state.total_staked.into())
            .unwrap()
            .checked_mul(new_dist_balance.into())
            .unwrap(),
        state.total_weight.denominator(),
    );

    state.global_index += adding_index;
    state.last_updated = env.block.time;
    STATE.save(deps.storage, &state)?;
    Ok(())
}

pub fn execute_update_stakers_rewards(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    address: Option<String>,
) -> Result<Response, ContractError> {
    let mut state = STATE.load(deps.storage)?;

    // Zero staking check
    if state.total_staked.is_zero() {
        return Err(ContractError::NoBond {});
    }
    //validate address
    let addr = maybe_addr(deps.api, address)?.unwrap_or(info.sender);
    let mut staker = STAKEPOSITIONS.load(deps.storage, &Addr::unchecked(addr.clone()))?;
    let rewards = update_stakers_rewards(deps.branch(), &mut state, env, staker)?;

    STATE.save(deps.storage, &state)?;
    STAKEPOSITIONS.save(deps.storage, &Addr::unchecked(addr.clone()), &staker)?;

    let res = Response::new()
        .add_attribute("action", "update_stakers_rewards")
        .add_attribute("address", addr)
        .add_attribute("rewards", rewards);
    Ok(res)
}

pub fn update_stakers_rewards(
    mut deps: DepsMut,
    state: &mut State,
    env: Env,
    staker: Vec<StakePosition>,
) -> Result<Uint128, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    //update reward index
    update_reward_index(state, config, deps.branch(), env)?;

    let mut total_rewards: Vec<Uint128> = vec![];

    for mut stakeposition in staker {
        let position_weight = Decimal256::from_str(
            &(stakeposition.unbond_duration.as_nanos() as f64)
                .sqrt()
                .to_string(),
        )?;
        let index_diff = state.global_index - stakeposition.index;

        let multiplier = index_diff.checked_mul(position_weight).unwrap();

        let new_distrubuted_reward = (multiplier
            .checked_mul(Decimal256::from_ratio(stakeposition.staked_amount, 1)))
        .unwrap()
        .checked_add(stakeposition.dec_rewards)
        .unwrap();

        let decimals = get_decimals(new_distrubuted_reward).unwrap();

        let rewards_uint128 = (new_distrubuted_reward * Uint256::one())
            .try_into()
            .unwrap_or(Uint128::zero());

        total_rewards.push(rewards_uint128);
        stakeposition.dec_rewards = decimals;
        stakeposition.pending_rewards += rewards_uint128;
        stakeposition.index = state.global_index;
    }

    Ok(total_rewards.iter().sum())
}

pub fn execute_receive_reward(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    let mut state = STATE.load(deps.storage)?;
    let config = CONFIG.load(deps.storage)?;

    let staker = STAKEPOSITIONS.load(deps.storage, &info.sender)?;
    if staker.is_empty() {
        return Err(ContractError::NoBond {});
    }

    let rewards = update_stakers_rewards(deps, &mut state, env, staker)?;

    //iter every stakeposition and update pending to zero
    for mut stakeposition in staker {
        stakeposition.pending_rewards = Uint128::zero();
    }

    STAKEPOSITIONS.save(deps.storage, &info.sender, &staker)?;

    //match config.denom to Native or cw20

    let asset = match (&config.reward_denom) {
        Denom::Native(denom) => Asset::native(denom, rewards),

        Denom::Cw20(address) => Asset::cw20(*address, rewards),
    };
    let msg = asset.transfer_msg(info.sender)?;

    let res = Response::new()
        .add_message(msg)
        .add_attribute("action", "receive_reward")
        .add_attribute("rewards", rewards.to_string());
    Ok(res)
}

pub fn execute_withdraw(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    amount: Option<Uint128>,
    duration: Duration,
) -> Result<Response, ContractError> {
    let mut state = STATE.load(deps.storage)?;
    let config = CONFIG.load(deps.storage)?;

    let mut staker = STAKEPOSITIONS.load(deps.storage, &info.sender)?;

    if staker.is_empty() {
        return Err(ContractError::NoBond {});
    }
    //update rewards for stakers every position event though only one position is being withdrawn
    update_stakers_rewards(deps, &mut state, env, staker)?;

    //get the index of the position to be withdrawn
    let searchedPositionIndex = staker
        .iter()
        .position(|stakeposition| stakeposition.unbond_duration == duration);
    //If no position found return error
    if searchedPositionIndex.is_none() {
        return Err(ContractError::NoBondForThisDuration {});
    }

    let rewards = staker[searchedPositionIndex.unwrap()].pending_rewards;

    //if amount is none withdraw all
    let withdraw_amount = match amount {
        Some(amount) => {
            if staker[searchedPositionIndex.unwrap()].staked_amount < amount {
                return Err(ContractError::InsufficientStakedAmount {});
            }
            amount
        }
        None => staker[searchedPositionIndex.unwrap()].staked_amount,
    };

    //create claim
    let claim = Claim {
        amount: withdraw_amount,
        release_at: env.block.time.plus_nanos(duration.as_nanos() as u64),
    };

    //Load claims and save new one
    let mut claims = CLAIMS.load(deps.storage, &info.sender).unwrap_or(vec![]);
    claims.push(claim);
    CLAIMS.save(deps.storage, &info.sender, &claims)?;

    //update stakeposition
    staker[searchedPositionIndex.unwrap()].staked_amount -= withdraw_amount;
    staker[searchedPositionIndex.unwrap()].pending_rewards = Uint128::zero();
    staker[searchedPositionIndex.unwrap()].last_claimed = env.block.time;

    //if stakeposition.stakedamount is zero remove it
    if staker[searchedPositionIndex.unwrap()].staked_amount == Uint128::zero() {
        staker.remove(searchedPositionIndex.unwrap());
    }

    STAKEPOSITIONS.save(deps.storage, &info.sender, &staker)?;

    let reward_asset = match (&config.reward_denom) {
        Denom::Native(denom) => Asset::native(denom, rewards),

        Denom::Cw20(address) => Asset::cw20(*address, rewards),
    };
    let reward_message = reward_asset.transfer_msg(info.sender)?;

    let res = Response::new()
        .add_message(reward_message)
        .add_attribute("action", "withdraw")
        .add_attribute("amount", withdraw_amount.to_string())
        .add_attribute("unbond_time", claim.release_at.to_string())
        .add_attribute("rewards", rewards.to_string());
    Ok(res)
}

//update config
pub fn execute_update_config(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    config: Config,
) -> Result<Response, ContractError> {
    let mut state = STATE.load(deps.storage)?;
    let mut old_config = CONFIG.load(deps.storage)?;

    if info.sender != old_config.admin {
        return Err(ContractError::Unauthorized {});
    }

    //update config
    old_config.cw20_token_address = config.cw20_token_address;
    old_config.native_token = config.native_token;
    old_config.admin = config.admin;

    CONFIG.save(deps.storage, &old_config)?;

    //update state
    state.global_index = Uint128::zero();
    state.total_staked_amount = Uint128::zero();
    state.total_staked_amount_native = Uint128::zero();
    state.total_staked_amount_cw20 = Uint128::zero();

    STATE.save(deps.storage, &state)?;

    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::State {} => to_binary(&query_state(deps, env, msg)?),
        QueryMsg::AccruedRewards { address } => {
            to_binary(&query_accrued_rewards(env, deps, address)?)
        }
        QueryMsg::Holder { address } => to_binary(&query_holder(env, deps, address)?),
        QueryMsg::Config {} => to_binary(&query_config(deps, env, msg)?),
        QueryMsg::Holders { start_after, limit } => {
            to_binary(&query_holders(deps, env, start_after, limit)?)
        }
    }
}

pub fn query_state(deps: Deps, _env: Env, _msg: QueryMsg) -> StdResult<StateResponse> {
    let state = STATE.load(deps.storage)?;

    Ok(StateResponse {
        total_staked: state.total_staked,
        global_index: state.global_index,
        prev_reward_balance: state.prev_reward_balance,
    })
}

//query config
pub fn query_config(deps: Deps, _env: Env, _msg: QueryMsg) -> StdResult<ConfigResponse> {
    let config = CONFIG.load(deps.storage)?;

    Ok(ConfigResponse {
        staked_token_denom: config.staked_token_denom,
        reward_denom: config.reward_denom,
        admin: config.admin.into_string(),
    })
}

pub fn query_accrued_rewards(
    _env: Env,
    deps: Deps,
    address: String,
) -> StdResult<AccruedRewardsResponse> {
    let addr = deps.api.addr_validate(&address.as_str())?;
    let holder = HOLDERS.load(deps.storage, &addr)?;

    Ok(AccruedRewardsResponse {
        rewards: holder.pending_rewards,
    })
}

pub fn query_holder(_env: Env, deps: Deps, address: String) -> StdResult<HolderResponse> {
    let holder: Holder = HOLDERS.load(deps.storage, &deps.api.addr_validate(address.as_str())?)?;
    Ok(HolderResponse {
        address: address,
        balance: holder.balance,
        index: holder.index,
        pending_rewards: holder.pending_rewards,
        dec_rewards: holder.dec_rewards,
    })
}

// calculate the reward with decimal
pub fn get_decimals(value: Decimal256) -> StdResult<Decimal256> {
    let stringed: &str = &*value.to_string();
    let parts: &[&str] = &*stringed.split('.').collect::<Vec<&str>>();
    match parts.len() {
        1 => Ok(Decimal256::zero()),
        2 => {
            let decimals: Decimal256 = Decimal256::from_str(&*("0.".to_owned() + parts[1]))?;
            Ok(decimals)
        }
        _ => Err(StdError::generic_err("Unexpected number of dots")),
    }
}

const MAX_LIMIT: u32 = 30;
const DEFAULT_LIMIT: u32 = 10;
//query all holders list
pub fn query_holders(
    deps: Deps,
    _env: Env,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<HoldersResponse> {
    let addr = maybe_addr(deps.api, start_after)?;
    let start = addr.as_ref().map(Bound::exclusive);
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let holders: StdResult<Vec<HolderResponse>> = HOLDERS
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(|item| {
            let (addr, holder) = item?;
            let holder_response = HolderResponse {
                address: addr.to_string(),
                balance: holder.balance,
                index: holder.index,
                pending_rewards: holder.pending_rewards,
                dec_rewards: holder.dec_rewards,
            };
            Ok(holder_response)
        })
        .collect();

    Ok(HoldersResponse { holders: holders? })
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}
