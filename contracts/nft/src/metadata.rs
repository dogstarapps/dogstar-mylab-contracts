use soroban_sdk::{contracttype, Env, Vec};

use crate::{
    nft_info::Category,
    storage_types::{DataKey, TokenId, STORAGE_BUMP_LEDGERS, STORAGE_THRESHOLD_LEDGERS},
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
pub(crate) fn write_metadata(e: &Env, token_id : u32,  metadata: CardMetadata)  {
    let key  = DataKey::TokenId(token_id);
    e.storage().instance().set(&key, &metadata);
}
*/

pub fn read_metadata(e: &Env, token_id: u32) -> CardMetadata {
    let key = DataKey::TokenId(token_id);
    e.storage().instance().get(&key).unwrap()
}

pub(crate) fn write_metadata(e: &Env, token_id: u32, metadata: CardMetadata) {
    let key = DataKey::TokenId(token_id);
    e.storage().instance().set(&key, &metadata);
    e.storage().instance().extend_ttl(
        STORAGE_THRESHOLD_LEDGERS,
        STORAGE_BUMP_LEDGERS,
    );

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
        e.storage().persistent().extend_ttl(
            &DataKey::AllCardIds,
            STORAGE_THRESHOLD_LEDGERS,
            STORAGE_BUMP_LEDGERS,
        );
    }
}


// GUARDRAIL: Single-writer helper for metadata
pub fn update_metadata<F>(e: &Env, token_id: u32, f: F)
where
    F: FnOnce(&Env, &mut CardMetadata),
{
    let mut meta = read_metadata(e, token_id);
    f(e, &mut meta);
    write_metadata(e, token_id, meta);
}
