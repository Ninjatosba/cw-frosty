Frosty

Abstract
Frosty is a staking contract with special features. Mechanism allows users to stake cw20 tokens to contract with desired unbonding duration and recieve rewards
based on duration and staked amount.

Design
-Instantiate
Everyone can instantiate contract by sending InstantiateContract transaction. In message stake_token_address,reward_token_address,admin,force_claim_ratio,
fee_collector,max_bond_duration is expected.
-Fund reward
Rewards can only be funded by admin. Contract expects RewardUpdate msg from cw20_reward_contract. Message must contain a reward_end_date as Timestamp.
At each RewardUpdate the contract sets total_rewards as incoming_rewards+remaning_rewards also sets reward_end_date as msg.reward_end_date and start_time as current time.
-Bond
Users can bond cw20 tokens to contract by sending Bond msg. Message must contain unbonding_duration_as_days. This duration must be between 1 and max_bond_duration.
