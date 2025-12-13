use soroban_sdk::{contracttype, Env, String, Vec};

use crate::{
    nft_info::Category,
    storage_types::{DataKey, TokenId},
};

#[derive(Clone)]
#[contracttype]
pub struct CardMetadata {
    pub initial_power: u32,
    pub max_power: u32,
    pub level: u32,
    pub category: Category,
    pub price_xtar: i128,
    pub price_terry: i128,
    pub token_id: u32,
}

/*
pub fn write_metadata(e: &Env, token_id : u32,  metadata: CardMetadata)  {
    let key  = DataKey::TokenId(token_id);
    e.storage().instance().set(&key, &metadata);
}
*/

pub fn read_metadata(e: &Env, token_id: u32) -> CardMetadata {
    let key = DataKey::TokenId(token_id);
    e.storage().instance().get(&key).unwrap()
}

pub fn write_metadata(e: &Env, token_id: u32, metadata: CardMetadata) {
    let key = DataKey::TokenId(token_id);
    e.storage().instance().set(&key, &metadata);

    // Recuperamos el listado actual de todos los TokenIds
    let mut all_card_ids: Vec<TokenId> = e
        .storage()
        .persistent()
        .get(&DataKey::AllCardIds)
        .unwrap_or(Vec::new(&e));

    // Verificamos si el TokenId ya existe en la lista
    let token_id_exists = all_card_ids.contains(&TokenId(token_id));

    // Solo agregamos el TokenId si no existe previamente
    if !token_id_exists {
        all_card_ids.push_back(TokenId(token_id));

        // Actualizamos la lista de TokenIds en el almacenamiento
        e.storage()
            .persistent()
            .set(&DataKey::AllCardIds, &all_card_ids);
    }
}
