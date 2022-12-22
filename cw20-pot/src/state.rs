use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, DepsMut, StdResult, Uint128, Uint64};
use cw_storage_plus::{Item, Map};

#[cw_serde]
pub struct Config {
    pub owner: Addr,
    pub cw20_addr: Addr,
}

pub const CONFIG: Item<Config> = Item::new("config");

#[cw_serde]
pub struct Pot {
    pub target_addr: Addr,
    pub threshold: Uint128,
    pub collected: Uint128,
}

pub const POT_SEQ: Item<u64> = Item::new("pot_seq");
pub const POTS: Map<u64, Pot> = Map::new("pot");

pub fn save_pot(deps: DepsMut, pot: &Pot) -> StdResult<()> {
    let id = POT_SEQ.load(deps.storage)?;
    let id = Uint64::new(id).checked_add(Uint64::new(1))?.u64();
    POT_SEQ.save(deps.storage, &id)?;

    POTS.save(deps.storage, id, pot)
}
