use std::str::FromStr;

use cosmwasm_std::{Addr, Decimal256, StdError, StdResult, Uint128};

pub fn days_to_seconds(days: u128) -> u64 {
    (days * 24 * 60 * 60) as u64
}

// calculate the reward with decimal
pub fn get_decimals(value: Decimal256) -> StdResult<Decimal256> {
    let stringed: &str = &value.to_string();
    let parts: &[&str] = &stringed.split('.').collect::<Vec<&str>>();
    match parts.len() {
        1 => Ok(Decimal256::zero()),
        2 => {
            let decimals: Decimal256 = Decimal256::from_str(&("0.".to_owned() + parts[1]))?;
            Ok(decimals)
        }
        _ => Err(StdError::generic_err("Unexpected number of dots")),
    }
}
pub fn calculate_weight(amount: Uint128, duration: u128) -> StdResult<Decimal256> {
    let new_weight = Decimal256::from_ratio(duration, Uint128::one())
        .sqrt()
        .checked_mul(Decimal256::from_ratio(amount, Uint128::one()))?;
    Ok(new_weight)
}
