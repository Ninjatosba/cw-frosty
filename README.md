# Frosty Staking Contract

Frosty is a staking contract with special features designed to allow users to stake CW20 tokens with a desired unbonding duration and receive rewards based on their staked amount and duration.

## Design

### Instantiation

Anyone can instantiate the contract by sending an InstantiateContract transaction. The message must include the following information: `stake_token_address`, `reward_token_address`, `admin`, `force_claim_ratio`, `fee_collector`, and `max_bond_duration`.

### Reward Funding

Rewards can only be funded by the contract's admin. The contract expects a `RewardUpdate` message from the CW20 reward contract, which must include a `reward_end_date` as a Timestamp.

At each RewardUpdate, the contract sets `total_rewards` as the sum of incoming rewards and remaining rewards. It also sets `reward_end_date` as the `msg.reward_end_date` and `start_time` as the current time.

### Bonding

Users can bond CW20 tokens to the contract by sending a Bond message. The message must include an `unbonding_duration_as_days`, which must be between 1 and `max_bond_duration`. When bonded, the user weight is calculated as shown below:

$$ \text{position weight} = \sqrt{{\texttt{duration}}} \times {\text{amount}} $$

### Reward Distribution

Rewards will be calculated depending on the weight of the position. At each `update_index` call the contract calculates how much reward is to be distubuted as follows

$$ {new Dist Balance = { {now-last Updated} \over {reward End Time-last Updated}}\*total Reward Supply} $$

$$ {global index = {last Global Index + new Dist Balance \over total Weight}} $$

At `update_staker_rewards` call the contract will calculate rewards for every position of the user(e.g. A user staked for two positions as 10 days and 30 days). Calculation is made as follows

$$ {new Rewards = {global Index-user Index} \* position Weight } $$

$$ {pending Rewards += new Rewards }$$

### Receive Rewards

Users can receive their rewards by sending a `ReceiveRewards` message to the contract.

### Unbonding

Users can unbond their staked tokens at any time by sending an `UnbondStake` transaction. The user must select which position to unbond by including the `duration_as_days` in the message.

Upon receiving the `UnbondStake` transaction. The rewards for the corresponding staking position will be updated and sent to the user. The contract will create a `claim` for bonded tokens to be claimed by user. This `claim` will not be claimable until the unbonding duration has elapsed. During the unbonding duration, the user will not receive any rewards.

### Force Claim

Users can claim their bonded_tokens before the unbonding duration elapsed by paying extra fee. The fee calculation is as follows

$$ {fee = {{release At - now \over release At-unbond At}*force Claim Ratio}*amount} $$
