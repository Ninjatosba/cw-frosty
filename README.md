# Frosty Staking Contract
Frosty is a staking contract with special features designed to allow users to stake CW20 tokens with a desired unbonding duration and receive rewards based on their staked amount and duration.

## Design
### Instantiation

Anyone can instantiate the contract by sending an InstantiateContract transaction. The message must include the following information: `stake_token_address`, `reward_token_address`, `admin`, `force_claim_ratio`, `fee_collector`, and `max_bond_duration`.





### Reward Funding
Rewards can only be funded by the contract's admin. The contract expects a RewardUpdate message from the CW20 reward contract, which must include a reward_end_date as a Timestamp.

At each RewardUpdate, the contract sets total_rewards as the sum of incoming rewards and remaining rewards. It also sets reward_end_date as the msg.reward_end_date and start_time as the current time.

### Bonding
Users can bond CW20 tokens to the contract by sending a Bond message. The message must include an unbonding_duration_as_days, which must be between 1 and max_bond_duration. When bonded, the user weight is calculated as shown below:

```
let position_weight = Decimal256::from_ratio(duration, Uint128::one())
.sqrt()
.checked_mul(Decimal256::from_ratio(amount, Uint128::one()))?;
```


### Reward Distribution
Rewards will be calculated depending on the weight of the position. At each `update_index` call the contract calculates how much reward is to be distubuted as follows


$$new_distrubution_balance = (now-last_updated/reward_end_time-last_updated)*total_reward_supply
index+= new_distrubution_balance/total_weight$$




### Claiming Rewards
Users can claim their rewards by sending a ClaimRewards message to the contract.
