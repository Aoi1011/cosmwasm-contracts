use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Uint128, Uint64};
use cw20::Cw20ReceiveMsg;

#[cw_serde]
pub struct InstantiateMsg {
    pub admin: Option<String>,
    pub cw20_addr: String,
}

#[cw_serde]
pub enum ExecuteMsg {
    CreatePot {
        target_addr: String,
        threshold: Uint128,
    },
    Receive(Cw20ReceiveMsg),
}

#[cw_serde]
pub enum ReceiveMsg {
    Send { id: Uint64 },
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(PotResponse)]
    GetPot { id: Uint64 },
}

#[cw_serde]
pub struct PotResponse {
    pub target_addr: String,
    pub threshold: Uint128,
    pub collected: Uint128,
}
