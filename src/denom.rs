use cosmwasm_std::Addr;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum Denom {
    Native(String),
    Cw20(Addr),
}

// TODO: remove or figure out where needed
impl Default for Denom {
    fn default() -> Denom {
        Denom::Native(String::default())
    }
}

impl Denom {
    pub fn is_empty(&self) -> bool {
        match self {
            Denom::Native(string) => string.is_empty(),
            Denom::Cw20(addr) => addr.as_ref().is_empty(),
        }
    }
    //implement to string function
    pub fn to_string(&self) -> String {
        match self {
            Denom::Native(string) => string.to_string(),
            Denom::Cw20(addr) => addr.as_ref().to_string(),
        }
    }
}
