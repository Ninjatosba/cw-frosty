#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use cosmwasm_std::testing::{
        mock_dependencies, mock_dependencies_with_balance, mock_env, mock_info,
    };
    use cosmwasm_std::{
        from_binary, to_binary, Addr, BankMsg, Coin, CosmosMsg, Decimal, Decimal256, MessageInfo,
        Response, Timestamp, Uint128, Uint256, WasmMsg,
    };
    use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};
    use cw_utils::PaymentError;

    use crate::contract::{
        execute, instantiate, query, query_staker_for_all_duration, query_staker_for_duration,
        query_state, update_staker_rewards,
    };
    use crate::msg::{
        ConfigResponse, ExecuteMsg, InstantiateMsg, QueryMsg, ReceiveMsg,
        StakerForAllDurationResponse, StakerResponse, StateResponse,
    };
    use crate::ContractError;

    fn default_init() -> InstantiateMsg {
        InstantiateMsg {
            stake_token_address: "stake_token_address".to_string(),
            reward_token_address: "reward_token_address".to_string(),
            admin: None,
            force_claim_ratio: Decimal::from_str("0.1").unwrap(),
            fee_collector: "fee_collector".to_string(),
        }
    }

    #[test]
    fn proper_init() {
        let mut deps = mock_dependencies();
        let init_msg = default_init();
        let env = mock_env();
        let info = MessageInfo {
            sender: Addr::unchecked("creator"),
            funds: vec![],
        };
        //instantiate without admin
        let res = instantiate(deps.as_mut(), env.clone(), info, init_msg).unwrap();
        //default response attributes is empty

        assert_eq!(
            res,
            Response::default()
                .add_attribute("method", "instantiate")
                .add_attribute("admin", "creator")
                .add_attribute("stake_token_address", "stake_token_address")
                .add_attribute("reward_token_address", "reward_token_address")
                .add_attribute("force_claim_ratio", "0.1")
                .add_attribute("fee_collector", "fee_collector")
        );

        // instantiate with admin
        let mut deps = mock_dependencies();
        let init_msg = InstantiateMsg {
            stake_token_address: "stake_token_address".to_string(),
            reward_token_address: "reward_token_address".to_string(),
            admin: Some("admin".to_string()),
            force_claim_ratio: Decimal::from_str("0.1").unwrap(),
            fee_collector: "fee_collector".to_string(),
        };
        let env = mock_env();
        let info = MessageInfo {
            sender: Addr::unchecked("creator"),
            funds: vec![],
        };
        let res = instantiate(deps.as_mut(), env.clone(), info, init_msg).unwrap();
        assert_eq!(
            res,
            Response::default()
                .add_attribute("method", "instantiate")
                .add_attribute("admin", "admin")
                .add_attribute("stake_token_address", "stake_token_address")
                .add_attribute("reward_token_address", "reward_token_address")
                .add_attribute("force_claim_ratio", "0.1")
                .add_attribute("fee_collector", "fee_collector")
        );
    }

    #[test]
    pub fn test_bond() {
        //instantiate
        let mut deps = mock_dependencies();
        let init_msg = default_init();
        let env = mock_env();
        let info = mock_info("creator", &[]);
        instantiate(deps.as_mut(), env.clone(), info.clone(), init_msg);

        //bond with no funds
        let info = mock_info("stake_token_address", &[]);
        let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender: "staker1".to_string(),
            amount: Uint128::zero(),
            msg: to_binary(&ReceiveMsg::Bond { duration_day: 10 }).unwrap(),
        });
        let res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap_err();
        assert_eq!(res, ContractError::NoFund {});

        //bond with funds
        let info = mock_info(
            "stake_token_address",
            &vec![Coin {
                denom: "staked".to_string(),
                amount: Uint128::new(100),
            }],
        );
        let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender: "staker1".to_string(),
            amount: Uint128::new(100),
            msg: to_binary(&ReceiveMsg::Bond { duration_day: 10 }).unwrap(),
        });

        let res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

        // query  staker
        let res = query_staker_for_all_duration(deps.as_ref(), env.clone(), "staker1".to_string())
            .unwrap();
        assert_eq!(
            res,
            StakerForAllDurationResponse {
                positions: vec![StakerResponse {
                    staked_amount: Uint128::new(100),
                    index: Decimal::zero().into(),
                    bond_time: Timestamp::from_nanos(1571797419879305533),
                    unbond_duration_as_days: 10,
                    pending_rewards: Uint128::zero(),
                    dec_rewards: Decimal::zero().into(),
                    last_claimed: Timestamp::from_nanos(1571797419879305533),
                    position_weight: Decimal256::from_str(
                        "316.2277660168379331".to_string().as_str()
                    )
                    .unwrap(),
                }]
            }
        );

        // bond again with same duration and address
        let info = mock_info("stake_token_address", &vec![]);
        let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender: "staker1".to_string(),
            amount: Uint128::new(100),
            msg: to_binary(&ReceiveMsg::Bond { duration_day: 10 }).unwrap(),
        });

        let _res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

        // query  staker
        let res = query_staker_for_all_duration(deps.as_ref(), env, "staker1".to_string()).unwrap();
        // new weight shuld be 200*(10.sqrt()) = 200*3.16 = 632
        assert_eq!(
            res.positions[0].position_weight,
            Decimal256::from_str("632.4555320336758662").unwrap()
        );
        assert_eq!(res.positions[0].staked_amount, Uint128::new(200));
        assert_eq!(res.positions[0].unbond_duration_as_days, 10);
        assert_eq!(res.positions[0].index, Decimal256::zero());
    }

    #[test]
    pub fn test_fund_reward() {
        //instantiation
        let mut deps = mock_dependencies();
        let init_msg = default_init();
        let env = mock_env();
        let info = MessageInfo {
            sender: Addr::unchecked("creator"),
            funds: vec![],
        };
        let _res = instantiate(deps.as_mut(), env.clone(), info, init_msg).unwrap();

        // fund reward with wrong end_time
        let info = mock_info("reward_token_address", &[]);
        let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender: "creator".to_string(),
            amount: Uint128::new(100_000_000),
            msg: to_binary(&ReceiveMsg::RewardUpdate {
                reward_end_time: env.block.time.minus_seconds(100_000),
            })
            .unwrap(),
        });
        let res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap_err();
        assert_eq!(res, ContractError::InvalidRewardEndTime {});

        // update_reward_index before fund_reward
        let info = mock_info("creator", &[]);
        let msg = ExecuteMsg::UpdateRewardIndex {};
        let res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
        assert_eq!(res.attributes[1].value, "0".to_string());

        //fund reward
        let info = mock_info("reward_token_address", &[]);
        let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender: "creator".to_string(),
            amount: Uint128::new(100_000_000),
            msg: to_binary(&ReceiveMsg::RewardUpdate {
                reward_end_time: env.block.time.plus_seconds(100_000),
            })
            .unwrap(),
        });
        let res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

        // update reward index after fund_reward but without any bond
        let info = mock_info("creator", &[]);
        let msg = ExecuteMsg::UpdateRewardIndex {};
        let res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
        assert_eq!(res.attributes[1].value, "0".to_string());

        // bond
        let info = mock_info("stake_token_address", &vec![]);
        let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender: "staker1".to_string(),
            amount: Uint128::new(100),
            msg: to_binary(&ReceiveMsg::Bond { duration_day: 10 }).unwrap(),
        });
        let res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

        // update reward index after fund_reward and bond
        let info = mock_info("creator", &[]);
        let msg = ExecuteMsg::UpdateRewardIndex {};
        let mut env = mock_env();
        env.block.time = env.block.time.plus_seconds(100);
        let res = execute(deps.as_mut(), env, info.clone(), msg).unwrap();

        assert_eq!(
            res.attributes[1].value,
            "316.227766016837933299".to_string()
        );

        // change reward end time without any fund
        let info = mock_info("reward_token_address", &[]);
        let mut env = mock_env();
        env.block.time = env.block.time.plus_seconds(200);

        let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender: "creator".to_string(),
            amount: Uint128::new(0),
            msg: to_binary(&ReceiveMsg::RewardUpdate {
                reward_end_time: env.block.time.plus_seconds(100_000_000),
            })
            .unwrap(),
        });
        let _res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

        // query  state
        let res = query_state(deps.as_ref(), env.clone(), QueryMsg::State {}).unwrap();
        assert_eq!(
            res.reward_end_time,
            Timestamp::from_nanos(1671797619879305533)
        );

        // change reward end time with fund
        let info = mock_info("reward_token_address", &[]);
        let mut env = mock_env();
        env.block.time = env.block.time.plus_seconds(200);

        let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender: "creator".to_string(),
            amount: Uint128::new(100_000_000),
            msg: to_binary(&ReceiveMsg::RewardUpdate {
                reward_end_time: env.block.time.plus_seconds(100_000_000),
            })
            .unwrap(),
        });
        let _res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

        // query  state
        let res = query_state(deps.as_ref(), env.clone(), QueryMsg::State {}).unwrap();
        assert_eq!(
            res.reward_end_time,
            Timestamp::from_nanos(1671797619879305533)
        );
        assert_eq!(
            res.global_index,
            Decimal256::from_str("632.455532033675866598").unwrap()
        );
        assert_eq!(res.total_reward_supply, Uint128::new(199800000));
    }

    #[test]
    pub fn test_update_reward_index() {
        // instantiate
        let mut deps = mock_dependencies();
        let init_msg = default_init();
        let env = mock_env();
        instantiate(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            init_msg,
        )
        .unwrap();

        // update reward index no index update because no bond and rewards
        let info = mock_info("creator", &[]);
        let msg = ExecuteMsg::UpdateRewardIndex {};
        let res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
        assert_eq!(res.attributes[1].value, "0".to_string());

        // bond
        let info = mock_info("stake_token_address", &vec![]);
        let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender: "staker1".to_string(),
            amount: Uint128::new(100),
            msg: to_binary(&ReceiveMsg::Bond { duration_day: 10 }).unwrap(),
        });
        let res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

        // update reward index after bond
        let info = mock_info("creator", &[]);
        let msg = ExecuteMsg::UpdateRewardIndex {};
        let mut env = mock_env();
        env.block.time = env.block.time.plus_seconds(100);
        let res = execute(deps.as_mut(), env, info.clone(), msg).unwrap();
        // still zero because no rewards is supplied
        assert_eq!(res.attributes[1].value, "0".to_string());

        // fund reward
        let info = mock_info("reward_token_address", &[]);
        let mut env = mock_env();
        env.block.time = env.block.time.plus_seconds(200);
        let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender: "creator".to_string(),
            amount: Uint128::new(100_000_000),
            msg: to_binary(&ReceiveMsg::RewardUpdate {
                reward_end_time: env.block.time.plus_seconds(100_000),
            })
            .unwrap(),
        });
        let _res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

        // update reward index after fund reward
        let info = mock_info("creator", &[]);
        let msg = ExecuteMsg::UpdateRewardIndex {};
        let mut env = mock_env();
        env.block.time = env.block.time.plus_seconds(100);
        let _res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

        // query  state
        let res = query_state(deps.as_ref(), env.clone(), QueryMsg::State {}).unwrap();
        assert_eq!(
            res.global_index,
            Decimal256::from_str("313.065488356669553966").unwrap()
        );
        //
        assert_eq!(res.total_reward_claimed, Uint128::new(99000));
    }

    #[test]
    pub fn test_update_staker_rewards() {
        // instantiate
        let mut deps = mock_dependencies();
        let init_msg = default_init();
        let env = mock_env();
        instantiate(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            init_msg,
        )
        .unwrap();

        // bond
        let env = mock_env();
        let info = mock_info("stake_token_address", &vec![]);
        let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender: "staker1".to_string(),
            amount: Uint128::new(100),
            msg: to_binary(&ReceiveMsg::Bond { duration_day: 10 }).unwrap(),
        });
        let res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

        // fund reward
        let info = mock_info("reward_token_address", &[]);
        let mut env = mock_env();
        let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender: "creator".to_string(),
            amount: Uint128::new(100_000_000),
            msg: to_binary(&ReceiveMsg::RewardUpdate {
                reward_end_time: env.block.time.plus_seconds(100_000),
            })
            .unwrap(),
        });

        let _res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

        // update staker rewards
        let info = mock_info("staker1", &[]);
        let msg = ExecuteMsg::UpdateStakersReward { address: None };
        let mut env = mock_env();
        env.block.time = env.block.time.plus_seconds(1000);
        let _res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
        // query  staker
        let res = query_staker_for_duration(env.clone(), deps.as_ref(), "staker1".to_string(), 10)
            .unwrap();
        // checking if the reward distrubuted is same as pending rewards of staker
        let reward_to_staker1 = res.pending_rewards;
        let rounded_reward =
            Uint128::from_str(res.dec_rewards.to_uint_ceil().to_string().as_str()).unwrap();

        // query  state
        let res = query_state(deps.as_ref(), env.clone(), QueryMsg::State {}).unwrap();
        let reward_distrubuted = res.total_reward_claimed;
        assert_eq!(reward_to_staker1 + rounded_reward, reward_distrubuted);

        // update one staker with multiple durations
        // second bond
        //first 1000000 is for first bond
        let mut env = mock_env();
        env.block.time = env.block.time.plus_seconds(1000);
        let info = mock_info("stake_token_address", &vec![]);
        let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender: "staker1".to_string(),
            amount: Uint128::new(100),
            msg: to_binary(&ReceiveMsg::Bond { duration_day: 20 }).unwrap(),
        });
        let _res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

        // update staker rewards
        let info = mock_info("staker1", &[]);
        let msg = ExecuteMsg::UpdateStakersReward { address: None };
        let mut env = mock_env();
        env.block.time = env.block.time.plus_seconds(2000);
        let res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
        let rewards = res.attributes[2].value.parse::<u128>().unwrap();

        // query  staker for all durations
        let res = query_state(deps.as_ref(), env.clone(), QueryMsg::State {}).unwrap();

        let res = query_staker_for_all_duration(deps.as_ref(), env.clone(), "staker1".to_string())
            .unwrap();

        // query state
        // checking if the reward distrubuted is same as pending rewards of staker
        let reward_to_staker1 = res.positions[0].pending_rewards + res.positions[1].pending_rewards;
        let rounded_reward = Uint128::from_str(
            (res.positions[0].dec_rewards + res.positions[1].dec_rewards)
                .to_uint_ceil()
                .to_string()
                .as_str(),
        );
        // query  state
        let res = query_state(deps.as_ref(), env.clone(), QueryMsg::State {}).unwrap();

        let reward_distrubuted = res.total_reward_claimed;
        assert_eq!(
            reward_to_staker1 + rounded_reward.unwrap(),
            reward_distrubuted
        );
    }

    #[test]
    pub fn test_scenario() {
        //init
        let mut deps = mock_dependencies_with_balance(&[]);
        let init_msg = default_init();
        let env = mock_env();
        instantiate(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            init_msg,
        )
        .unwrap();

        //first bond
        let info = mock_info("stake_token_address", &vec![]);
        let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender: "staker1".to_string(),
            amount: Uint128::new(100),
            msg: to_binary(&ReceiveMsg::Bond { duration_day: 16 }).unwrap(),
        });
        let res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

        //second bond
        let info = mock_info("stake_token_address", &vec![]);
        let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender: "staker2".to_string(),
            amount: Uint128::new(100),
            msg: to_binary(&ReceiveMsg::Bond { duration_day: 25 }).unwrap(),
        });
        let res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

        //third bond
        let info = mock_info("stake_token_address", &vec![]);
        let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender: "staker3".to_string(),
            amount: Uint128::new(100),
            msg: to_binary(&ReceiveMsg::Bond { duration_day: 36 }).unwrap(),
        });
        let _res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
        // fund rewards
        // reward amount 100_000_000
        // distrubuted in 100_000 seconds
        let info = mock_info("reward_token_address", &vec![]);
        let env = mock_env();
        let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender: "creator".to_string(),
            amount: Uint128::new(100_000_000),
            msg: to_binary(&ReceiveMsg::RewardUpdate {
                reward_end_time: env.block.time.plus_seconds(100_000),
            })
            .unwrap(),
        });
        let _res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

        // update staker rewards at 1000 seconds all reweards should be 100_000_000/100=1_000_000
        // staker1 amount 100 -- duration 16 -- weight 100*4=400
        // staker2 amount 100 -- duration 25 -- weight 100*5=500
        // staker3 amount 100 -- duration 36 -- weight 100*6=600
        // total weight 1500
        // staker1 reward 400/1500*1_000_000= 266_666
        // staker2 reward 500/1500*1_000_000= 333_333
        // staker3 reward 600/1500*1_000_000= 400_000

        let info = mock_info("staker1", &[]);
        let mut env = mock_env();
        env.block.time = env.block.time.plus_seconds(1000);
        let msg = ExecuteMsg::UpdateStakersReward { address: None };
        let res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

        // update staker 2
        let info = mock_info("staker2", &[]);
        let msg = ExecuteMsg::UpdateStakersReward { address: None };
        let res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
        // update staker 3
        let info = mock_info("staker3", &[]);
        let msg = ExecuteMsg::UpdateStakersReward { address: None };
        let res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

        // query staker 1
        let res = query_staker_for_all_duration(deps.as_ref(), env.clone(), "staker1".to_string())
            .unwrap();
        assert_eq!(res.positions[0].pending_rewards, Uint128::new(266_666));
        // query staker 2
        let res = query_staker_for_all_duration(deps.as_ref(), env.clone(), "staker2".to_string())
            .unwrap();
        assert_eq!(res.positions[0].pending_rewards, Uint128::new(333_333));
        // query staker 3
        let res = query_staker_for_all_duration(deps.as_ref(), env.clone(), "staker3".to_string())
            .unwrap();
        assert_eq!(res.positions[0].pending_rewards, Uint128::new(399_999));
    }

    #[test]
    pub fn test_recieve_rewards() {
        //init
        let mut deps = mock_dependencies_with_balance(&[]);
        let init_msg = default_init();
        let env = mock_env();
        instantiate(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            init_msg,
        )
        .unwrap();

        //fund rewards
        //reward amount 100_000_000
        //distrubuted in 100_000 seconds
        let info = mock_info("reward_token_address", &vec![]);
        let env = mock_env();
        let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender: "creator".to_string(),
            amount: Uint128::new(100_000_000),
            msg: to_binary(&ReceiveMsg::RewardUpdate {
                reward_end_time: env.block.time.plus_seconds(100_000),
            })
            .unwrap(),
        });
        let _res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

        //bond
        let info = mock_info("stake_token_address", &vec![]);
        let env = mock_env();
        let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender: "staker1".to_string(),
            amount: Uint128::new(100),
            msg: to_binary(&ReceiveMsg::Bond { duration_day: 16 }).unwrap(),
        });
        let _res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

        // staker 1 recieve rewards at 1000 seconds all rewards should be 100_000_000/100=1_000_000
        let info = mock_info("staker1", &[]);
        let mut env = mock_env();
        env.block.time = env.block.time.plus_seconds(1000);
        let msg = ExecuteMsg::ReceiveReward {};
        let res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
        assert_eq!(res.attributes[2].value, "1000000".to_string());
        assert_eq!(
            res.messages[0].msg,
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: "reward_token_address".to_string(),
                funds: vec![],
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: "staker1".to_string(),
                    amount: Uint128::new(1_000_000),
                })
                .unwrap(),
            })
        );

        // staker 1 bond with diffirent duration at 2000 seconds
        let info = mock_info("stake_token_address", &vec![]);
        let mut env = mock_env();
        env.block.time = env.block.time.plus_seconds(2000);
        let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender: "staker1".to_string(),
            amount: Uint128::new(100),
            msg: to_binary(&ReceiveMsg::Bond { duration_day: 36 }).unwrap(),
        });
        let _res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

        // staker 1 recieve rewards at 3000 seconds all rewards should be 3_000_000
        // for duration 16 it should be 2000000+400(but recieved 1000000+400)
        // for duration 36 it should be 600
        // total 2000000
        let info = mock_info("staker1", &[]);
        let mut env = mock_env();
        env.block.time = env.block.time.plus_seconds(3000);
        let msg = ExecuteMsg::ReceiveReward {};
        let res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
        assert_eq!(res.attributes[2].value, "2000000".to_string());
        assert_eq!(
            res.messages[0].msg,
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: "reward_token_address".to_string(),
                funds: vec![],
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: "staker1".to_string(),
                    amount: Uint128::new(2_000_000),
                })
                .unwrap(),
            })
        );
    }

    // #[test]
    // pub fn test_update_holders_rewards() {
    //     let mut deps = mock_dependencies_with_balance(&[]);
    //     let init_msg = default_init();
    //     let env = mock_env();

    //     instantiate(
    //         deps.as_mut(),
    //         env.clone(),
    //         mock_info("creator", &[]),
    //         init_msg,
    //     )
    //     .unwrap();

    //     //update_stakers_rewards by random address
    //     let info = mock_info("random", &[]);
    //     let msg = ExecuteMsg::UpdateHoldersReward { address: None };
    //     let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    //     assert_eq!(res.unwrap_err(), ContractError::NoBond {});

    //     //first bond
    //     let info = mock_info(
    //         "staker1",
    //         &vec![Coin {
    //             denom: "staked".to_string(),
    //             amount: Uint128::new(100),
    //         }],
    //     );

    //     let msg = ExecuteMsg::BondStake {};
    //     let _res = execute(deps.as_mut(), env.clone(), info.clone(), msg);

    //     //second bond
    //     let info = mock_info(
    //         "staker2",
    //         &vec![Coin {
    //             denom: "staked".to_string(),
    //             amount: Uint128::new(200),
    //         }],
    //     );
    //     let msg = ExecuteMsg::BondStake {};
    //     let _res = execute(deps.as_mut(), env.clone(), info.clone(), msg);

    //     //update balance
    //     deps.querier.update_balance(
    //         env.contract.address.as_str(),
    //         vec![Coin {
    //             denom: "rewards".to_string(),
    //             amount: Uint128::new(100),
    //         }],
    //     );

    //     //update first stakers rewards
    //     let info: MessageInfo = mock_info("staker1", &[]);
    //     let msg = ExecuteMsg::UpdateHoldersReward { address: None };
    //     let _res = execute(deps.as_mut(), env.clone(), info.clone(), msg);

    //     //check first stakers rewards
    //     let res = query(
    //         deps.as_ref(),
    //         env.clone(),
    //         QueryMsg::Holder {
    //             address: "staker1".to_string(),
    //         },
    //     )
    //     .unwrap();
    //     let holder_response: HolderResponse = from_binary(&res).unwrap();
    //     assert_eq!(holder_response.pending_rewards, Uint128::new(33));
    //     assert_eq!(
    //         holder_response.dec_rewards,
    //         Decimal256::new(Uint256::from_str("333333333333333300").unwrap())
    //     );

    //     //update second stakers rewards
    //     let info: MessageInfo = mock_info("staker2", &[]);
    //     let msg = ExecuteMsg::UpdateHoldersReward { address: None };
    //     let _res = execute(deps.as_mut(), env.clone(), info.clone(), msg);

    //     //check second stakers rewards
    //     let res = query(
    //         deps.as_ref(),
    //         env.clone(),
    //         QueryMsg::Holder {
    //             address: "staker2".to_string(),
    //         },
    //     )
    //     .unwrap();
    //     let holder_response: HolderResponse = from_binary(&res).unwrap();

    //     assert_eq!(holder_response.pending_rewards, Uint128::new(66));
    // }

    // #[test]
    // pub fn test_withdraw() {
    //     let mut deps = mock_dependencies_with_balance(&[]);
    //     let init_msg = default_init();
    //     let env = mock_env();

    //     instantiate(
    //         deps.as_mut(),
    //         env.clone(),
    //         mock_info("creator", &[]),
    //         init_msg,
    //     )
    //     .unwrap();

    //     //first bond
    //     let info = mock_info(
    //         "staker1",
    //         &vec![Coin {
    //             denom: "staked".to_string(),
    //             amount: Uint128::new(100),
    //         }],
    //     );

    //     let msg = ExecuteMsg::BondStake {};
    //     let _res = execute(deps.as_mut(), env.clone(), info.clone(), msg);

    //     //second bond
    //     let info = mock_info(
    //         "staker2",
    //         &vec![Coin {
    //             denom: "staked".to_string(),
    //             amount: Uint128::new(200),
    //         }],
    //     );
    //     let msg = ExecuteMsg::BondStake {};
    //     let _res = execute(deps.as_mut(), env.clone(), info.clone(), msg);

    //     //update balance
    //     deps.querier.update_balance(
    //         env.contract.address.as_str(),
    //         vec![Coin {
    //             denom: "rewards".to_string(),
    //             amount: Uint128::new(100),
    //         }],
    //     );

    //     //withdraw staker1's stake without cap
    //     let _info: MessageInfo = mock_info("staker1", &[]);
    //     let _msg = ExecuteMsg::WithdrawStake { amount: None };
    //     let res = execute(deps.as_mut(), env.clone(), _info.clone(), _msg).unwrap();
    //     assert_eq!(
    //         res.messages[0].msg,
    //         CosmosMsg::Bank(BankMsg::Send {
    //             to_address: "staker1".to_string(),
    //             amount: vec![Coin {
    //                 denom: "rewards".to_string(),
    //                 amount: Uint128::new(33),
    //             }],
    //         }),
    //     );
    //     assert_eq!(
    //         res.messages[1].msg,
    //         CosmosMsg::Bank(BankMsg::Send {
    //             to_address: "staker1".to_string(),
    //             amount: vec![Coin {
    //                 denom: "staked".to_string(),
    //                 amount: Uint128::new(100),
    //             }],
    //         }),
    //     );

    //     //check state for total staked
    //     let res = query(deps.as_ref(), env.clone(), QueryMsg::State {}).unwrap();
    //     let state: StateResponse = from_binary(&res).unwrap();
    //     assert_eq!(state.total_staked, Uint128::new(200));
    // }

    // #[test]
    // pub fn test_update_config() {
    //     let mut deps = mock_dependencies_with_balance(&[]);
    //     let init_msg = default_init();
    //     let env = mock_env();

    //     instantiate(
    //         deps.as_mut(),
    //         env.clone(),
    //         mock_info("creator", &[]),
    //         init_msg,
    //     )
    //     .unwrap();
    //     //random can't update config
    //     let info: MessageInfo = mock_info("random", &[]);
    //     let msg = ExecuteMsg::UpdateConfig {
    //         reward_token_address: Some("new_reward_token_address".to_string()),
    //         staked_token_denom: Some("new_staked_token_denom".to_string()),
    //         admin: Some("new_admin".to_string()),
    //     };
    //     let res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap_err();
    //     assert_eq!(res, ContractError::Unauthorized {});

    //     //creator can update config
    //     let info: MessageInfo = mock_info("creator", &[]);
    //     let msg = ExecuteMsg::UpdateConfig {
    //         reward_token_address: Some("new_reward_token_address".to_string()),
    //         staked_token_denom: Some("new_staked_token_denom".to_string()),
    //         admin: Some("new_admin".to_string()),
    //     };
    //     let _res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

    //     //check config
    //     let res = query(deps.as_ref(), env.clone(), QueryMsg::Config {}).unwrap();
    //     let config_response: ConfigResponse = from_binary(&res).unwrap();
    //     assert_eq!(config_response.admin, "new_admin".to_string());
    //     assert_eq!(config_response.reward_token_address, "new_reward_token_address".to_string());
    //     assert_eq!(
    //         config_response.staked_token_denom,
    //         "new_staked_token_denom".to_string()
    //     );
}

// #[test]
// pub fn test_case_1() {
//     let mut deps = mock_dependencies_with_balance(&[]);
//     let init_msg = default_init();
//     let env = mock_env();

//     instantiate(
//         deps.as_mut(),
//         env.clone(),
//         mock_info("creator", &[]),
//         init_msg,
//     )
//     .unwrap();

//     //first bond
//     let info = mock_info(
//         "staker1",
//         &vec![Coin {
//             denom: "staked".to_string(),
//             amount: Uint128::new(10),
//         }],
//     );

//     let msg = ExecuteMsg::BondStake {};
//     let _res = execute(deps.as_mut(), env.clone(), info.clone(), msg);

//     //update balance
//     deps.querier.update_balance(
//         env.contract.address.as_str(),
//         vec![Coin {
//             denom: "rewards".to_string(),
//             amount: Uint128::new(100),
//         }],
//     );

//     //second bond
//     let info = mock_info(
//         "staker2",
//         &vec![Coin {
//             denom: "staked".to_string(),
//             amount: Uint128::new(20),
//         }],
//     );
//     let msg = ExecuteMsg::BondStake {};
//     let _res = execute(deps.as_mut(), env.clone(), info.clone(), msg);

//     //third bond
//     let info = mock_info(
//         "staker3",
//         &vec![Coin {
//             denom: "staked".to_string(),
//             amount: Uint128::new(30),
//         }],
//     );
//     let msg = ExecuteMsg::BondStake {};
//     let _res = execute(deps.as_mut(), env.clone(), info.clone(), msg);

//     //fourth bond
//     let info = mock_info(
//         "staker4",
//         &vec![Coin {
//             denom: "staked".to_string(),
//             amount: Uint128::new(40),
//         }],
//     );
//     let msg = ExecuteMsg::BondStake {};
//     let _res = execute(deps.as_mut(), env.clone(), info.clone(), msg);

//     //every staker updates their reward
//     let info: MessageInfo = mock_info("staker1", &[]);
//     let msg = ExecuteMsg::UpdateHoldersReward { address: None };
//     let _res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

//     let info: MessageInfo = mock_info("staker2", &[]);
//     let msg = ExecuteMsg::UpdateHoldersReward { address: None };
//     let _res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

//     let info: MessageInfo = mock_info("staker3", &[]);
//     let msg = ExecuteMsg::UpdateHoldersReward { address: None };
//     let _res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

//     let info: MessageInfo = mock_info("staker4", &[]);
//     let msg = ExecuteMsg::UpdateHoldersReward { address: None };
//     let _res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

//     //check state
//     let res = query(deps.as_ref(), env.clone(), QueryMsg::State {}).unwrap();
//     let state: StateResponse = from_binary(&res).unwrap();

//     //check staker1
//     let res = query(
//         deps.as_ref(),
//         env.clone(),
//         QueryMsg::Holder {
//             address: "staker1".to_string(),
//         },
//     )
//     .unwrap();

//     let holder: HolderResponse = from_binary(&res).unwrap();

//     //check staker2
//     let res = query(
//         deps.as_ref(),
//         env.clone(),
//         QueryMsg::Holder {
//             address: "staker2".to_string(),
//         },
//     )
//     .unwrap();

//     let holder: HolderResponse = from_binary(&res).unwrap();

//     //check staker3
//     let res = query(
//         deps.as_ref(),
//         env.clone(),
//         QueryMsg::Holder {
//             address: "staker3".to_string(),
//         },
//     )
//     .unwrap();

//     let holder: HolderResponse = from_binary(&res).unwrap();

//     //check staker4
//     let res = query(
//         deps.as_ref(),
//         env.clone(),
//         QueryMsg::Holder {
//             address: "staker4".to_string(),
//         },
//     )
//     .unwrap();

//     let holder: HolderResponse = from_binary(&res).unwrap();

//     //staker1 recieves reward
//     let info: MessageInfo = mock_info("staker1", &[]);
//     let msg = ExecuteMsg::ReceiveReward {};
//     let _res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

//     //update reward index
//     let info: MessageInfo = mock_info("staker1", &[]);
//     let msg = ExecuteMsg::UpdateRewardIndex {};
//     let _res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

//     //check state
//     let res = query(deps.as_ref(), env.clone(), QueryMsg::State {}).unwrap();
//     let state: StateResponse = from_binary(&res).unwrap();

//     //check staker1
//     let res = query(
//         deps.as_ref(),
//         env.clone(),
//         QueryMsg::Holder {
//             address: "staker1".to_string(),
//         },
//     )
//     .unwrap();

//     let holder: HolderResponse = from_binary(&res).unwrap();

//     //update balance
//     deps.querier.update_balance(
//         env.contract.address.as_str(),
//         vec![Coin {
//             denom: "rewards".to_string(),
//             amount: Uint128::new(200),
//         }],
//     );

//     //staker5 bonds
//     let info = mock_info(
//         "staker5",
//         &vec![Coin {
//             denom: "staked".to_string(),
//             amount: Uint128::new(50),
//         }],
//     );
//     let msg = ExecuteMsg::BondStake {};
//     let _res = execute(deps.as_mut(), env.clone(), info.clone(), msg);

//     //staker6 bonds
//     let info = mock_info(
//         "staker6",
//         &vec![Coin {
//             denom: "staked".to_string(),
//             amount: Uint128::new(60),
//         }],
//     );
//     let msg = ExecuteMsg::BondStake {};
//     let _res = execute(deps.as_mut(), env.clone(), info.clone(), msg);

//     //staker5 updates reward
//     let info: MessageInfo = mock_info("staker5", &[]);
//     let msg = ExecuteMsg::UpdateHoldersReward { address: None };
//     let _res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

//     //query staker5
//     let res = query(
//         deps.as_ref(),
//         env.clone(),
//         QueryMsg::Holder {
//             address: "staker5".to_string(),
//         },
//     )
//     .unwrap();

//     let holder: HolderResponse = from_binary(&res).unwrap();

//     //staker6 updates reward
//     let info: MessageInfo = mock_info("staker6", &[]);
//     let msg = ExecuteMsg::UpdateHoldersReward { address: None };
//     let _res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

//     //query staker6
//     let res = query(
//         deps.as_ref(),
//         env.clone(),
//         QueryMsg::Holder {
//             address: "staker6".to_string(),
//         },
//     )
//     .unwrap();

//     let holder: HolderResponse = from_binary(&res).unwrap();

//     //check state
//     let res = query(deps.as_ref(), env.clone(), QueryMsg::State {}).unwrap();
//     let state: StateResponse = from_binary(&res).unwrap();

//     //staker2 recieves reward
//     let info: MessageInfo = mock_info("staker2", &[]);
//     let msg = ExecuteMsg::ReceiveReward {};
//     let _res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

//     //check staker 2
//     let res = query(
//         deps.as_ref(),
//         env.clone(),
//         QueryMsg::Holder {
//             address: "staker2".to_string(),
//         },
//     )
//     .unwrap();

//     let holder: HolderResponse = from_binary(&res).unwrap();

//     //query all holders
//     let res = query(
//         deps.as_ref(),
//         env.clone(),
//         QueryMsg::Holders {
//             start_after: None,
//             limit: None,
//         },
//     )
//     .unwrap();
//     let holders: HoldersResponse = from_binary(&res).unwrap();
// }
// }
