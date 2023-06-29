#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use cosmwasm_std::testing::{
        mock_dependencies, mock_dependencies_with_balance, mock_env, mock_info,
    };
    use cosmwasm_std::{
        coin, from_binary, to_binary, Addr, BankMsg, Coin, CosmosMsg, Decimal, Decimal256, Env,
        MessageInfo, Response, StdError, Timestamp, Uint128, WasmMsg,
    };
    use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};

    use crate::contract::{
        execute, instantiate, query, query_staker_for_all_duration, query_staker_for_duration,
        query_state,
    };
    use crate::msg::{
        ClaimResponse, ConfigResponse, ExecuteMsg, InstantiateMsg, ListClaimsResponse, QueryMsg,
        ReceiveMsg, StakerForAllDurationResponse, StakerResponse, StateResponse,
    };
    use crate::ContractError;

    fn default_init() -> InstantiateMsg {
        InstantiateMsg {
            stake_token_address: "stake_token_address".to_string(),
            reward_token_cw20: Some("reward_token_address".to_string()),
            reward_token_native: None,
            admin: None,
            force_claim_ratio: Decimal::from_str("0.1").unwrap(),
            fee_collector: "fee_collector".to_string(),
            max_bond_duration: 100,
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
        let res = instantiate(deps.as_mut(), env, info, init_msg).unwrap();
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
            reward_token_cw20: Some("reward_token_address".to_string()),
            reward_token_native: None,
            admin: Some("admin".to_string()),
            force_claim_ratio: Decimal::from_str("0.1").unwrap(),
            fee_collector: "fee_collector".to_string(),
            max_bond_duration: 100,
        };
        let env = mock_env();
        let info = MessageInfo {
            sender: Addr::unchecked("creator"),
            funds: vec![],
        };
        let res = instantiate(deps.as_mut(), env, info, init_msg).unwrap();
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
        let mut env = mock_env();
        env.block.height = 10;
        let info = mock_info("creator", &[]);
        instantiate(deps.as_mut(), env.clone(), info, init_msg).unwrap();

        //bond with no funds
        let info = mock_info("stake_token_address", &[]);
        let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender: "staker1".to_string(),
            amount: Uint128::zero(),
            msg: to_binary(&ReceiveMsg::Bond { duration_day: 10 }).unwrap(),
        });
        let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap_err();
        assert_eq!(res, ContractError::NoFund {});

        // bond with wrong stake token
        let info = mock_info("wrong_stake_token_address", &[]);
        let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender: "staker1".to_string(),
            amount: Uint128::new(100),
            msg: to_binary(&ReceiveMsg::Bond { duration_day: 10 }).unwrap(),
        });
        let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap_err();
        assert_eq!(res, ContractError::InvalidCw20TokenAddress {});

        //bond with funds
        let mut env = mock_env();
        env.block.height = 11;
        let info = mock_info(
            "stake_token_address",
            &[Coin {
                denom: "staked".to_string(),
                amount: Uint128::new(100),
            }],
        );
        let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender: "staker1".to_string(),
            amount: Uint128::new(100),
            msg: to_binary(&ReceiveMsg::Bond { duration_day: 10 }).unwrap(),
        });

        let _res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();

        // query  staker
        let res = query_staker_for_all_duration(deps.as_ref(), env.clone(), "staker1".to_string())
            .unwrap();
        assert_eq!(
            res,
            StakerForAllDurationResponse {
                positions: vec![StakerResponse {
                    staked_amount: Uint128::new(100),
                    index: Decimal::zero().into(),
                    bond_block: 11,
                    unbond_duration_as_days: 10,
                    pending_rewards: Uint128::zero(),
                    dec_rewards: Decimal::zero().into(),
                    last_claimed_block: 11,
                    position_weight: Decimal256::from_str(
                        "316.2277660168379331".to_string().as_str()
                    )
                    .unwrap(),
                }]
            }
        );

        // bond again with same duration and address
        let info = mock_info("stake_token_address", &[]);
        let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender: "staker1".to_string(),
            amount: Uint128::new(100),
            msg: to_binary(&ReceiveMsg::Bond { duration_day: 10 }).unwrap(),
        });

        let _res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();

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
    pub fn test_update_reward_index() {
        // instantiate
        let mut deps = mock_dependencies();
        let init_msg = default_init();
        let mut env = mock_env();
        env.block.height = 10;
        instantiate(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            init_msg,
        )
        .unwrap();

        // update reward index no index update because no bond and reward per second is 0
        let info = mock_info("creator", &[]);
        let msg = ExecuteMsg::UpdateRewardIndex {};
        let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();
        assert_eq!(res.attributes[1].value, "0".to_string());

        // bond
        let info = mock_info("stake_token_address", &[]);
        let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender: "staker1".to_string(),
            amount: Uint128::new(100),
            msg: to_binary(&ReceiveMsg::Bond { duration_day: 10 }).unwrap(),
        });
        let _res = execute(deps.as_mut(), env, info, msg).unwrap();

        // update reward index after bond
        let info = mock_info("creator", &[]);
        let msg = ExecuteMsg::UpdateRewardIndex {};
        let mut env = mock_env();
        env.block.height = 10;
        let res = execute(deps.as_mut(), env, info, msg).unwrap();
        // still zero because no reward per second
        assert_eq!(res.attributes[2].value, "0".to_string());

        // set reward per block
        let info = mock_info("reward_token_address", &[]);
        let mut env = mock_env();
        env.block.height = 20;
        let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender: "creator".to_string(),
            amount: Uint128::new(100),
            msg: to_binary(&ExecuteMsg::SetRewardPerBlock {
                reward_per_block: Uint128::new(1),
            })
            .unwrap(),
        });
        let _res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();

        // update reward index after reward per block set
        let mut env = mock_env();
        env.block.height = 120;
        let info = mock_info("creator", &[]);
        let msg = ExecuteMsg::UpdateRewardIndex {};
        let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();

        // query  state
        let res = query_state(deps.as_ref(), env, QueryMsg::State {}).unwrap();
        assert_eq!(
            res.global_index,
            Decimal256::from_str("0.316227766016837933").unwrap()
        );
        assert_eq!(res.total_reward_claimed, Uint128::new(100));
    }

    #[test]
    pub fn test_update_staker_rewards() {
        // instantiate
        let mut deps = mock_dependencies();
        let init_msg = default_init();
        let mut env = mock_env();
        env.block.height = 100;
        instantiate(deps.as_mut(), env, mock_info("creator", &[]), init_msg).unwrap();

        // update staker rewards with no bond
        let info = mock_info("creator", &[]);
        let msg = ExecuteMsg::UpdateStakerRewards { address: None };
        let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert_eq!(res, ContractError::NoBond {});

        // bond
        let mut env = mock_env();
        env.block.height = 200;
        let info = mock_info("stake_token_address", &[]);
        let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender: "staker1".to_string(),
            amount: Uint128::new(100),
            msg: to_binary(&ReceiveMsg::Bond { duration_day: 10 }).unwrap(),
        });
        let _res = execute(deps.as_mut(), env, info, msg).unwrap();

        // set reward per block
        let info = mock_info("reward_token_address", &[]);
        let mut env = mock_env();
        env.block.height = 300;
        let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender: "creator".to_string(),
            amount: Uint128::new(100_000),
            msg: to_binary(&ExecuteMsg::SetRewardPerBlock {
                reward_per_block: Uint128::new(1),
            })
            .unwrap(),
        });
        let _res = execute(deps.as_mut(), env, info, msg).unwrap();

        // update staker rewards
        let info = mock_info("staker1", &[]);
        let msg = ExecuteMsg::UpdateStakerRewards { address: None };
        let mut env = mock_env();
        env.block.height = 400;
        let _res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();
        // query  staker
        let res = query_staker_for_duration(env.clone(), deps.as_ref(), "staker1".to_string(), 10)
            .unwrap();
        // checking if the reward distrubuted is same as pending rewards of staker
        let reward_to_staker1 = res.pending_rewards;
        let rounded_reward =
            Uint128::from_str(res.dec_rewards.to_uint_ceil().to_string().as_str()).unwrap();

        // query  state
        let res = query_state(deps.as_ref(), env, QueryMsg::State {}).unwrap();
        let reward_distrubuted = res.total_reward_claimed;
        assert_eq!(reward_to_staker1 + rounded_reward, reward_distrubuted);

        // update one staker with multiple durations
        // second bond
        // first 1000000 is for first bond
        let mut env = mock_env();
        env.block.height = 500;
        let info = mock_info("stake_token_address", &[]);
        let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender: "staker1".to_string(),
            amount: Uint128::new(100),
            msg: to_binary(&ReceiveMsg::Bond { duration_day: 20 }).unwrap(),
        });
        let _res = execute(deps.as_mut(), env, info, msg).unwrap();

        // update staker rewards
        let info = mock_info("staker1", &[]);
        let msg = ExecuteMsg::UpdateStakerRewards { address: None };
        let mut env = mock_env();
        env.block.height = 600;
        let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();
        let _rewards = res.attributes[2].value.parse::<u128>().unwrap();

        // query state
        let _res = query_state(deps.as_ref(), env.clone(), QueryMsg::State {}).unwrap();

        // query  staker for all durations
        let res = query_staker_for_all_duration(deps.as_ref(), env.clone(), "staker1".to_string())
            .unwrap();

        // checking if the reward distrubuted is same as pending rewards of staker
        let reward_to_staker1 = res.positions[0].pending_rewards + res.positions[1].pending_rewards;
        let rounded_reward = Uint128::from_str(
            (res.positions[0].dec_rewards + res.positions[1].dec_rewards)
                .to_uint_ceil()
                .to_string()
                .as_str(),
        );
        // query  state
        let res = query_state(deps.as_ref(), env, QueryMsg::State {}).unwrap();

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
        let mut env = mock_env();
        env.block.height = 100;
        instantiate(
            deps.as_mut(),
            env.clone(),
            mock_info("creator", &[]),
            init_msg,
        )
        .unwrap();

        //first bond
        let info = mock_info("stake_token_address", &[]);
        let mut env = mock_env();
        env.block.height = 200;
        let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender: "staker1".to_string(),
            amount: Uint128::new(100),
            msg: to_binary(&ReceiveMsg::Bond { duration_day: 16 }).unwrap(),
        });
        let _res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();

        //second bond
        let info = mock_info("stake_token_address", &[]);
        let mut env = mock_env();
        env.block.height = 300;
        let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender: "staker2".to_string(),
            amount: Uint128::new(100),
            msg: to_binary(&ReceiveMsg::Bond { duration_day: 25 }).unwrap(),
        });
        let _res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();

        //third bond
        let info = mock_info("stake_token_address", &[]);
        let mut env = mock_env();
        env.block.height = 400;
        let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender: "staker3".to_string(),
            amount: Uint128::new(100),
            msg: to_binary(&ReceiveMsg::Bond { duration_day: 36 }).unwrap(),
        });
        let _res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();
        // set reward per block
        let info = mock_info("reward_token_address", &[]);
        let mut env = mock_env();
        env.block.height = 500;
        let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender: "creator".to_string(),
            amount: Uint128::new(100_000_000),
            msg: to_binary(&ExecuteMsg::SetRewardPerBlock {
                reward_per_block: Uint128::new(100),
            })
            .unwrap(),
        });
        let _res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();

        // update staker rewards at 1000 blocks, total rewards are (1000-500)*100=50000
        // staker1 amount 100 -- duration 16 -- weight 100*4=400
        // staker2 amount 100 -- duration 25 -- weight 100*5=500
        // staker3 amount 100 -- duration 36 -- weight 100*6=600
        // total weight 1500
        // staker1 rewards 400/1500*50000=13333
        // staker2 rewards 500/1500*50000=16666
        // staker3 rewards 600/1500*50000=20000

        // update staker 1
        let info = mock_info("staker1", &[]);
        let mut env = mock_env();
        env.block.height = 1000;
        let msg = ExecuteMsg::UpdateStakerRewards { address: None };
        let _res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();
        // update staker 2
        let info = mock_info("staker2", &[]);
        let msg = ExecuteMsg::UpdateStakerRewards { address: None };
        let _res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();
        // update staker 3
        let info = mock_info("staker3", &[]);
        let msg = ExecuteMsg::UpdateStakerRewards { address: None };
        let _res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();

        // query staker 1
        let res = query_staker_for_all_duration(deps.as_ref(), env.clone(), "staker1".to_string())
            .unwrap();
        assert_eq!(res.positions[0].pending_rewards, Uint128::new(133_33));
        // query staker 2
        let res = query_staker_for_all_duration(deps.as_ref(), env.clone(), "staker2".to_string())
            .unwrap();
        assert_eq!(res.positions[0].pending_rewards, Uint128::new(166_66));
        // query staker 3
        let res = query_staker_for_all_duration(deps.as_ref(), env, "staker3".to_string()).unwrap();
        assert_eq!(res.positions[0].pending_rewards, Uint128::new(19999));
    }

    #[test]
    pub fn test_receive_rewards() {
        //init
        let mut deps = mock_dependencies_with_balance(&[]);
        let init_msg = default_init();
        let mut env = mock_env();
        env.block.height = 1000;
        instantiate(deps.as_mut(), env, mock_info("creator", &[]), init_msg).unwrap();

        // set reward per block
        let info = mock_info("reward_token_address", &[]);
        let mut env = mock_env();
        env.block.height = 1000;
        let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender: "creator".to_string(),
            amount: Uint128::new(1000000),
            msg: to_binary(&ExecuteMsg::SetRewardPerBlock {
                reward_per_block: Uint128::new(1),
            })
            .unwrap(),
        });
        let _res = execute(deps.as_mut(), env, info, msg).unwrap();

        //bond
        let info = mock_info("stake_token_address", &[]);
        let mut env = mock_env();
        env.block.height = 1000;
        let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender: "staker1".to_string(),
            amount: Uint128::new(100),
            msg: to_binary(&ReceiveMsg::Bond { duration_day: 16 }).unwrap(),
        });
        let _res = execute(deps.as_mut(), env, info, msg).unwrap();

        // Staker 1 receive rewards at 2000 blocks
        // Total rewards should be 1*2000-1000 = 1000
        let info = mock_info("staker1", &[]);
        let mut env = mock_env();
        env.block.height = 2000;
        let msg = ExecuteMsg::ReceiveReward {};
        let res = execute(deps.as_mut(), env, info, msg).unwrap();
        assert_eq!(res.attributes[2].value, "1000".to_string());
        assert_eq!(
            res.messages[0].msg,
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: "reward_token_address".to_string(),
                funds: vec![],
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: "staker1".to_string(),
                    amount: Uint128::new(1000),
                })
                .unwrap(),
            })
        );

        // staker 1 bond with diffirent duration at 2500th blocks
        let info = mock_info("stake_token_address", &[]);
        let mut env = mock_env();
        env.block.height = 2500;
        let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender: "staker1".to_string(),
            amount: Uint128::new(100),
            msg: to_binary(&ReceiveMsg::Bond { duration_day: 36 }).unwrap(),
        });
        let _res = execute(deps.as_mut(), env, info, msg).unwrap();

        // staker 1 recieve rewards at 3000 blocks all rewards should be 2000
        // staker 1 already received 1000 rewards
        // so total rewards should be 2000-1000 = 1000
        // This test looks unnecessary but actually calculation has been made for 2 different durations and sum is correct as expected
        let info = mock_info("staker1", &[]);
        let mut env = mock_env();
        env.block.height = 3000;
        let msg = ExecuteMsg::ReceiveReward {};
        let res = execute(deps.as_mut(), env, info, msg).unwrap();
        assert_eq!(res.attributes[2].value, "1000".to_string());
        assert_eq!(
            res.messages[0].msg,
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: "reward_token_address".to_string(),
                funds: vec![],
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: "staker1".to_string(),
                    amount: Uint128::new(1000),
                })
                .unwrap(),
            })
        );
    }

    #[test]
    pub fn test_unbond() {
        //init
        let mut deps = mock_dependencies_with_balance(&[]);
        let init_msg = default_init();
        let mut env = mock_env();
        env.block.height = 1000;
        instantiate(deps.as_mut(), env, mock_info("creator", &[]), init_msg).unwrap();

        // set reward per block
        let info = mock_info("reward_token_address", &[]);
        let mut env = mock_env();
        env.block.height = 1002;
        let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender: "creator".to_string(),
            amount: Uint128::new(100),
            msg: to_binary(&ExecuteMsg::SetRewardPerBlock {
                reward_per_block: Uint128::new(1),
            })
            .unwrap(),
        });
        let _res = execute(deps.as_mut(), env, info, msg).unwrap();

        // unbond without bond
        let info = mock_info("staker1", &[]);
        let env = mock_env();
        let msg = ExecuteMsg::UnbondStake {
            amount: None,
            duration_as_days: 16,
        };
        let res = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert!(matches!(res, ContractError::Std(StdError::NotFound { .. })));

        // bond
        let info = mock_info("stake_token_address", &[]);
        let mut env = mock_env();
        env.block.height = 1003;
        let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender: "staker1".to_string(),
            amount: Uint128::new(100),
            msg: to_binary(&ReceiveMsg::Bond { duration_day: 16 }).unwrap(),
        });
        let _res = execute(deps.as_mut(), env, info, msg).unwrap();

        // unbond with amount more than bond
        let info = mock_info("staker1", &[]);
        let env = mock_env();
        let msg = ExecuteMsg::UnbondStake {
            amount: Some(Uint128::new(200)),
            duration_as_days: 16,
        };
        let res = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert_eq!(res, ContractError::InsufficientStakedAmount {});

        // unbond wrong duration
        let info = mock_info("staker1", &[]);
        let env = mock_env();
        let msg = ExecuteMsg::UnbondStake {
            amount: Some(Uint128::new(100)),
            duration_as_days: 36,
        };
        let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap_err();
        assert!(matches!(res, ContractError::Std(StdError::NotFound { .. })));

        // query state before unbond
        let res = query(deps.as_ref(), env, QueryMsg::State {}).unwrap();
        let state: StateResponse = from_binary(&res).unwrap();
        assert_eq!(state.total_staked, Uint128::new(100));
        assert_eq!(
            state.total_weight,
            Decimal256::from_str("400".to_string().as_str()).unwrap()
        );

        // unbond
        let info = mock_info("staker1", &[]);
        let mut env = mock_env();
        env.block.height = 2000;
        env.block.time = env.block.time.plus_seconds(1000);
        let msg = ExecuteMsg::UnbondStake {
            amount: Some(Uint128::new(100)),
            duration_as_days: 16,
        };
        let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();
        // at unbond, rewards are recieved
        assert_eq!(
            res.messages[0].msg,
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: "reward_token_address".to_string(),
                funds: vec![],
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: "staker1".to_string(),
                    amount: Uint128::new(99),
                })
                .unwrap(),
            })
        );
        // query state after unbond
        let res = query(deps.as_ref(), env.clone(), QueryMsg::State {}).unwrap();
        let state: StateResponse = from_binary(&res).unwrap();
        assert_eq!(state.total_staked, Uint128::new(0));
        assert_eq!(
            state.total_weight,
            Decimal256::from_str("0".to_string().as_str()).unwrap()
        );

        // query claim after unbond
        let res = query(
            deps.as_ref(),
            env,
            QueryMsg::ListClaims {
                address: "staker1".to_string(),
            },
        );
        let claims: ListClaimsResponse = from_binary(&res.unwrap()).unwrap();
        assert_eq!(claims.claims.len(), 1);
        assert_eq!(claims.claims[0].amount, Uint128::new(100));
        assert_eq!(
            claims.claims[0].release_at,
            Timestamp::from_nanos(1573180819879305533)
        );
        assert_eq!(
            claims.claims[0].unbond_at,
            Timestamp::from_nanos(1571798419879305533)
        );
    }

    #[test]
    pub fn test_claim_unbond() {
        // init
        let mut deps = mock_dependencies_with_balance(&[]);
        let init_msg = default_init();
        let mut env = mock_env();
        env.block.height = 1000;
        let info = mock_info("creator", &[]);
        instantiate(deps.as_mut(), env, info, init_msg).unwrap();

        // bond
        let info = mock_info("stake_token_address", &[]);
        let mut env = mock_env();
        env.block.height = 1001;
        let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender: "staker1".to_string(),
            amount: Uint128::new(100),
            msg: to_binary(&ReceiveMsg::Bond { duration_day: 16 }).unwrap(),
        });
        let _res = execute(deps.as_mut(), env, info, msg).unwrap();

        // set reward per block
        let info = mock_info("reward_token_address", &[]);
        let mut env = mock_env();
        env.block.height = 1002;
        let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender: "creator".to_string(),
            amount: Uint128::new(100),
            msg: to_binary(&ExecuteMsg::SetRewardPerBlock {
                reward_per_block: Uint128::new(1),
            })
            .unwrap(),
        });
        let _res = execute(deps.as_mut(), env, info, msg).unwrap();

        // try claiming before unbond
        let info = mock_info("staker1", &[]);
        let mut env = mock_env();
        env.block.time = env.block.time.plus_seconds(1500);
        let msg = ExecuteMsg::ClaimUnbonded {};
        let res = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert_eq!(res, ContractError::NoMatureClaim {});

        // unbond
        let info = mock_info("staker1", &[]);
        let mut env = mock_env();
        env.block.time = env.block.time.plus_seconds(1000);

        let msg = ExecuteMsg::UnbondStake {
            amount: Some(Uint128::new(100)),
            duration_as_days: 16,
        };
        let _res = execute(deps.as_mut(), env, info, msg).unwrap();

        // try claiming before release time
        let info = mock_info("staker1", &[]);
        let mut env = mock_env();
        env.block.time = env.block.time.plus_seconds(2000);
        let msg = ExecuteMsg::ClaimUnbonded {};
        let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap_err();
        assert_eq!(res, ContractError::NoMatureClaim {});
        //list claims before claim
        let res = query(
            deps.as_ref(),
            env,
            QueryMsg::ListClaims {
                address: "staker1".to_string(),
            },
        );
        let claims: ListClaimsResponse = from_binary(&res.unwrap()).unwrap();
        assert_eq!(claims.claims.len(), 1);
        // claim
        let info = mock_info("staker1", &[]);
        let mut env = mock_env();
        env.block.time = env.block.time.plus_seconds(1382400 + 1000);
        let msg = ExecuteMsg::ClaimUnbonded {};

        let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();
        assert_eq!(res.messages.len(), 1);
        assert_eq!(
            res.messages[0].msg,
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: "stake_token_address".to_string(),
                funds: vec![],
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: "staker1".to_string(),
                    amount: Uint128::new(100),
                })
                .unwrap(),
            })
        );
        // list claim
        let res = query(
            deps.as_ref(),
            env,
            QueryMsg::ListClaims {
                address: "staker1".to_string(),
            },
        );
        let claims: ListClaimsResponse = from_binary(&res.unwrap()).unwrap();
        assert_eq!(claims.claims.len(), 0);
    }

    #[test]
    pub fn test_multiple_claims() {
        // init
        let mut deps = mock_dependencies_with_balance(&[]);
        let init_msg = default_init();
        let mut env = mock_env();
        env.block.height = 1000;
        let info = mock_info("creator", &[]);
        instantiate(deps.as_mut(), env, info, init_msg).unwrap();

        // bond
        let info = mock_info("stake_token_address", &[]);
        let mut env = mock_env();
        env.block.height = 1001;
        let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender: "staker1".to_string(),
            amount: Uint128::new(100),
            msg: to_binary(&ReceiveMsg::Bond { duration_day: 16 }).unwrap(),
        });
        let _res = execute(deps.as_mut(), env, info, msg).unwrap();

        // set reward per block
        let info = mock_info("reward_token_address", &[]);
        let mut env = mock_env();
        env.block.height = 1002;
        let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender: "creator".to_string(),
            amount: Uint128::new(100),
            msg: to_binary(&ExecuteMsg::SetRewardPerBlock {
                reward_per_block: Uint128::new(1),
            })
            .unwrap(),
        });
        let _res = execute(deps.as_mut(), env, info, msg).unwrap();

        // bond again
        let info = mock_info("stake_token_address", &[]);
        let env = mock_env();
        let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender: "staker1".to_string(),
            amount: Uint128::new(100),
            msg: to_binary(&ReceiveMsg::Bond { duration_day: 20 }).unwrap(),
        });
        let _res = execute(deps.as_mut(), env, info, msg).unwrap();

        // unbond for first bond
        let info = mock_info("staker1", &[]);
        let mut env = mock_env();
        env.block.time = env.block.time.plus_seconds(1000);
        let msg = ExecuteMsg::UnbondStake {
            amount: Some(Uint128::new(50)),
            duration_as_days: 16,
        };
        let _res = execute(deps.as_mut(), env, info, msg).unwrap();

        // unbond for second bond
        let info = mock_info("staker1", &[]);
        let mut env = mock_env();
        env.block.time = env.block.time.plus_seconds(1000);
        let msg = ExecuteMsg::UnbondStake {
            amount: Some(Uint128::new(100)),
            duration_as_days: 20,
        };
        let _res = execute(deps.as_mut(), env, info, msg).unwrap();

        // unbond remaining for first bond
        let info = mock_info("staker1", &[]);
        let mut env = mock_env();
        env.block.time = env.block.time.plus_seconds(2000);
        let msg = ExecuteMsg::UnbondStake {
            amount: Some(Uint128::new(50)),
            duration_as_days: 16,
        };
        let _res = execute(deps.as_mut(), env, info, msg).unwrap();

        // claim for first bond at day 18
        let info = mock_info("staker1", &[]);
        let mut env = mock_env();
        env.block.time = env.block.time.plus_seconds(1382400 + 1001);

        let res = query(
            deps.as_ref(),
            env.clone(),
            QueryMsg::ListClaims {
                address: "staker1".to_string(),
            },
        );
        let claims: ListClaimsResponse = from_binary(&res.unwrap()).unwrap();

        assert_eq!(claims.claims.len(), 3);
        assert_eq!(
            claims.claims,
            vec![
                ClaimResponse {
                    amount: Uint128::new(50),
                    release_at: Timestamp::from_nanos(1573180819879305533),
                    unbond_at: Timestamp::from_nanos(1571798419879305533),
                },
                ClaimResponse {
                    amount: Uint128::new(50),
                    release_at: Timestamp::from_nanos(1573181819879305533),
                    unbond_at: Timestamp::from_nanos(1571799419879305533),
                },
                ClaimResponse {
                    amount: Uint128::new(100),
                    release_at: Timestamp::from_nanos(1573526419879305533),
                    unbond_at: Timestamp::from_nanos(1571798419879305533),
                },
            ]
        );

        let msg = ExecuteMsg::ClaimUnbonded {};
        let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();
        assert_eq!(res.messages.len(), 1);
        assert_eq!(
            res.messages[0].msg,
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: "stake_token_address".to_string(),
                funds: vec![],
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: "staker1".to_string(),
                    amount: Uint128::new(50),
                })
                .unwrap(),
            })
        );

        // list claims now there should be 1 claim to be relased
        let res = query(
            deps.as_ref(),
            env,
            QueryMsg::ListClaims {
                address: "staker1".to_string(),
            },
        );
        let claims: ListClaimsResponse = from_binary(&res.unwrap()).unwrap();
        assert_eq!(
            claims.claims,
            vec![
                ClaimResponse {
                    amount: Uint128::new(50),
                    release_at: Timestamp::from_nanos(1573181819879305533),
                    unbond_at: Timestamp::from_nanos(1571799419879305533),
                },
                ClaimResponse {
                    amount: Uint128::new(100),
                    release_at: Timestamp::from_nanos(1573526419879305533),
                    unbond_at: Timestamp::from_nanos(1571798419879305533),
                },
            ]
        );
    }

    #[test]

    pub fn test_force_claim() {
        // init
        let mut deps = mock_dependencies_with_balance(&[]);
        let init_msg = default_init();
        let mut env = mock_env();
        env.block.height = 1000;
        let info = mock_info("creator", &[]);
        instantiate(deps.as_mut(), env, info, init_msg).unwrap();

        // bond
        let info = mock_info("stake_token_address", &[]);
        let mut env = mock_env();
        env.block.height = 1001;
        let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender: "staker1".to_string(),
            amount: Uint128::new(100),
            msg: to_binary(&ReceiveMsg::Bond { duration_day: 16 }).unwrap(),
        });
        let _res = execute(deps.as_mut(), env, info, msg).unwrap();

        // set reward per block
        let info = mock_info("reward_token_address", &[]);
        let mut env = mock_env();
        env.block.height = 1002;
        let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender: "creator".to_string(),
            amount: Uint128::new(100),
            msg: to_binary(&ExecuteMsg::SetRewardPerBlock {
                reward_per_block: Uint128::new(1),
            })
            .unwrap(),
        });
        let _res = execute(deps.as_mut(), env, info, msg).unwrap();

        // unbond
        let info = mock_info("staker1", &[]);
        let mut env = mock_env();
        env.block.time = env.block.time.plus_seconds(1000);
        let msg = ExecuteMsg::UnbondStake {
            amount: Some(Uint128::new(100)),
            duration_as_days: 16,
        };
        let _res = execute(deps.as_mut(), env, info, msg).unwrap();

        // force claim with wrong timestamp
        let info = mock_info("staker1", &[]);
        let mut env = mock_env();
        env.block.time = env.block.time.plus_seconds(1000);
        let msg = ExecuteMsg::ForceClaim {
            release_at: Timestamp::from_seconds(env.block.time.seconds() + 1382401),
        };
        let res = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert_eq!(res, ContractError::NoClaimForTimestamp {});

        // force claim
        let info = mock_info("staker1", &[]);
        let mut env = mock_env();
        env.block.time = env.block.time.plus_seconds(1000);
        let msg = ExecuteMsg::ForceClaim {
            release_at: Timestamp::from_nanos(1573180819879305533),
        };
        // list claims
        let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();

        assert_eq!(
            res.messages[0].msg,
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: "stake_token_address".to_string(),
                funds: vec![],
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: "fee_collector".to_string(),
                    amount: Uint128::new(10),
                })
                .unwrap(),
            })
        );
        assert_eq!(
            res.messages[1].msg,
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: "stake_token_address".to_string(),
                funds: vec![],
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: "staker1".to_string(),
                    amount: Uint128::new(90),
                })
                .unwrap(),
            })
        );
        // list claims and check if removed
        let res = query(
            deps.as_ref(),
            env,
            QueryMsg::ListClaims {
                address: "staker1".to_string(),
            },
        );
        let claims: ListClaimsResponse = from_binary(&res.unwrap()).unwrap();
        assert_eq!(claims.claims.len(), 0);
    }

    #[test]

    pub fn test_update_config() {
        // init
        let mut deps = mock_dependencies_with_balance(&[]);
        let init_msg = default_init();
        let env = mock_env();
        let info = mock_info("creator", &[]);
        instantiate(deps.as_mut(), env.clone(), info, init_msg).unwrap();

        // update config by random address
        let info = mock_info("random", &[]);
        let msg = ExecuteMsg::UpdateConfig {
            force_claim_ratio: None,
            admin: None,
            fee_collector: None,
        };
        let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap_err();
        assert_eq!(res, ContractError::Unauthorized {});

        // update config
        let info = mock_info("creator", &[]);
        let msg = ExecuteMsg::UpdateConfig {
            force_claim_ratio: Some(Decimal::percent(20)),
            admin: Some("admin2".to_string()),
            fee_collector: Some("fee_collector2".to_string()),
        };
        let _res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();

        // check config
        let config = query(deps.as_ref(), env, QueryMsg::Config {}).unwrap();
        let config: ConfigResponse = from_binary(&config).unwrap();
        assert_eq!(config.force_claim_ratio, Decimal::percent(20).to_string());
        assert_eq!(config.admin, "admin2".to_string());
        assert_eq!(config.fee_collector, "fee_collector2".to_string());
    }
    #[test]
    pub fn test_claim_impl() {
        //init
        let mut deps = mock_dependencies_with_balance(&[]);
        let init_msg = default_init();
        let mut env = mock_env();
        env.block.height = 1000;
        instantiate(deps.as_mut(), env, mock_info("creator", &[]), init_msg).unwrap();

        // set reward per block
        let info = mock_info("reward_token_address", &[]);
        let mut env = mock_env();
        env.block.height = 1002;
        let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender: "creator".to_string(),
            amount: Uint128::new(100),
            msg: to_binary(&ExecuteMsg::SetRewardPerBlock {
                reward_per_block: Uint128::new(1),
            })
            .unwrap(),
        });
        let _res = execute(deps.as_mut(), env, info, msg).unwrap();

        // bond
        let info = mock_info("stake_token_address", &[]);
        let env = mock_env();
        let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender: "staker1".to_string(),
            amount: Uint128::new(100),
            msg: to_binary(&ReceiveMsg::Bond { duration_day: 16 }).unwrap(),
        });
        let _res = execute(deps.as_mut(), env, info, msg).unwrap();
        // bond again
        let info = mock_info("stake_token_address", &[]);
        let env = mock_env();
        let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender: "staker1".to_string(),
            amount: Uint128::new(100),
            msg: to_binary(&ReceiveMsg::Bond { duration_day: 15 }).unwrap(),
        });
        let _res = execute(deps.as_mut(), env, info, msg).unwrap();
        // unbond
        let info = mock_info("staker1", &[]);
        let mut env = mock_env();
        env.block.time = env.block.time.plus_seconds(1000);
        let msg = ExecuteMsg::UnbondStake {
            amount: Some(Uint128::new(100)),
            duration_as_days: 16,
        };
        let _res = execute(deps.as_mut(), env, info, msg).unwrap();

        // unbond again
        let info = mock_info("staker1", &[]);
        let mut env = mock_env();
        env.block.time = env.block.time.plus_seconds(1000);
        env.block.time = env.block.time.plus_seconds(86400);
        let msg = ExecuteMsg::UnbondStake {
            amount: Some(Uint128::new(100)),
            duration_as_days: 15,
        };
        let _res = execute(deps.as_mut(), env, info, msg).unwrap();
        // force claim for 2 claims for same address but diffirent duration although same release_time
        let info = mock_info("staker1", &[]);
        let mut env = mock_env();
        env.block.time = env.block.time.plus_seconds(1000);
        let msg = ExecuteMsg::ForceClaim {
            release_at: env.block.time.plus_seconds(86400 * 16),
        };
        let res = execute(deps.as_mut(), env, info, msg).unwrap();
        assert_eq!(
            res.messages[0].msg,
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: "stake_token_address".to_string(),
                funds: vec![],
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: "fee_collector".to_string(),
                    amount: Uint128::new(20),
                })
                .unwrap(),
            })
        );
        assert_eq!(
            res.messages[1].msg,
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: "stake_token_address".to_string(),
                funds: vec![],
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: "staker1".to_string(),
                    amount: Uint128::new(180),
                })
                .unwrap(),
            })
        );
    }
    #[test]
    #[test]
    pub fn test_status() {
        // init
        let mut deps = mock_dependencies_with_balance(&[]);
        let init_msg = default_init();
        let mut env = mock_env();
        env.block.height = 1000;
        let info = mock_info("creator", &[]);
        instantiate(deps.as_mut(), env, info, init_msg).unwrap();
    }

    pub fn test_native() {
        // init
        let mut deps = mock_dependencies_with_balance(&[]);
        let init_msg = InstantiateMsg {
            stake_token_address: "stake_token_address".to_string(),
            reward_token_cw20: None,
            reward_token_native: Some("reward_token_native".to_string()),
            admin: None,
            force_claim_ratio: Decimal::from_str("0.1").unwrap(),
            fee_collector: "fee_collector".to_string(),
            max_bond_duration: 100,
        };
        let mut env = mock_env();
        env.block.height = 1000;
        let info = mock_info("creator", &[]);
        instantiate(deps.as_mut(), env.clone(), info, init_msg).unwrap();

        // set reward per block
        let info = mock_info("creator", &[coin(1_000_000, "reward_token_native")]);
        let mut env = mock_env();
        env.block.height = 2000;
        let msg = ExecuteMsg::SetRewardPerBlock {
            reward_per_block: Uint128::new(1000),
        };
        let _res = execute(deps.as_mut(), env, info, msg).unwrap();

        // bond
        let info = mock_info("stake_token_address", &[]);
        let mut env = mock_env();
        env.block.height = 2100;
        let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender: "staker1".to_string(),
            amount: Uint128::new(100),
            msg: to_binary(&ReceiveMsg::Bond { duration_day: 16 }).unwrap(),
        });
        let _res = execute(deps.as_mut(), env, info, msg).unwrap();

        // 100 blocks passed and staker 1 receive rewards = reward amount 100*1000=100_000
        let info = mock_info("staker1", &[]);
        let mut env = mock_env();
        env.block.height = 2200;
        let msg = ExecuteMsg::ReceiveReward {};
        let res = execute(deps.as_mut(), env, info, msg).unwrap();
        assert_eq!(
            res.messages[0].msg,
            CosmosMsg::Bank(BankMsg::Send {
                to_address: "staker1".to_string(),
                amount: vec![coin(100_000, "reward_token_native")],
            })
        );
        // 100 blocks passed and staker 1 unbonds = reward amount 100*1000=100_000
        let info = mock_info("staker1", &[]);
        let mut env = mock_env();
        env.block.height = 2300;
        let msg = ExecuteMsg::UnbondStake {
            amount: None,
            duration_as_days: 16,
        };
        let res = execute(deps.as_mut(), env, info, msg).unwrap();
        assert_eq!(
            res.messages[0].msg,
            CosmosMsg::Bank(BankMsg::Send {
                to_address: "staker1".to_string(),
                amount: vec![coin(100_000, "reward_token_native")],
            })
        );
    }
}
