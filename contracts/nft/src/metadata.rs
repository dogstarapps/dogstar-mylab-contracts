use soroban_sdk::{contracttype, Env, Vec};

use crate::{
    admin::is_pages_only,
    nft_info::Category,
    storage_types::{
        DataKey, PagedListKind, TokenId, PAGE_SIZE_ALL_CARDS, STORAGE_BUMP_LEDGERS,
        STORAGE_THRESHOLD_LEDGERS,
    },
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
pub fn read_metadata(e: &Env, token_id: u32) -> CardMetadata {
    let key = DataKey::TokenId(token_id);
    e.storage().instance().get(&key).unwrap()
}

pub(crate) fn write_metadata(e: &Env, token_id: u32, metadata: CardMetadata) {
    let key = DataKey::TokenId(token_id);
    let already_exists = e.storage().instance().has(&key);
    e.storage().instance().set(&key, &metadata);
    e.storage().instance().extend_ttl(
        STORAGE_THRESHOLD_LEDGERS,
        STORAGE_BUMP_LEDGERS,
    );

    if !already_exists {
        if !is_pages_only(e) {
            let mut all_card_ids: Vec<TokenId> = e
                .storage()
                .persistent()
                .get(&DataKey::AllCardIds)
                .unwrap_or(Vec::new(&e));
            all_card_ids.push_back(TokenId(token_id));

            e.storage()
                .persistent()
                .set(&DataKey::AllCardIds, &all_card_ids);
            e.storage().persistent().extend_ttl(
                &DataKey::AllCardIds,
                STORAGE_THRESHOLD_LEDGERS,
                STORAGE_BUMP_LEDGERS,
            );
        }

        add_card_id_page_index(e, TokenId(token_id));
    }
}

pub fn read_all_card_ids_count(e: &Env) -> u32 {
    e.storage()
        .persistent()
        .get::<_, u32>(&DataKey::PagedCount(PagedListKind::AllCardIds))
        .unwrap_or(0)
}

pub fn read_all_card_ids_page(e: &Env, cursor: u32, limit: u32) -> Vec<TokenId> {
    if limit == 0 {
        return Vec::new(e);
    }
    let count = read_all_card_ids_count(e);
    if !is_pages_only(e)
        && !e
            .storage()
            .persistent()
            .has(&DataKey::PagedCount(PagedListKind::AllCardIds))
    {
        let legacy = e
            .storage()
            .persistent()
            .get::<_, Vec<TokenId>>(&DataKey::AllCardIds)
            .unwrap_or(Vec::new(e));
        let total = legacy.len();
        if cursor >= total {
            return Vec::new(e);
        }
        let end = (cursor.saturating_add(limit)).min(total);
        let mut out = Vec::new(e);
        let mut idx = cursor;
        while idx < end {
            out.push_back(legacy.get(idx).unwrap());
            idx += 1;
        }
        return out;
    }
    if cursor >= count {
        return Vec::new(e);
    }
    let end = (cursor.saturating_add(limit)).min(count);
    let mut out = Vec::new(e);
    let mut idx = cursor;
    while idx < end {
        if let Some(id) = read_card_id_at(e, idx) {
            out.push_back(id);
        }
        idx += 1;
    }
    out
}

fn read_card_id_at(e: &Env, idx: u32) -> Option<TokenId> {
    let page = idx / PAGE_SIZE_ALL_CARDS;
    let off = idx % PAGE_SIZE_ALL_CARDS;
    let key = DataKey::PagedList(PagedListKind::AllCardIds, page);
    let page_vec: Vec<TokenId> = e.storage().persistent().get(&key).unwrap_or(Vec::new(e));
    if off < page_vec.len() {
        Some(page_vec.get(off).unwrap())
    } else {
        None
    }
}

fn add_card_id_page_index(e: &Env, token_id: TokenId) {
    let count = e
        .storage()
        .persistent()
        .get::<_, u32>(&DataKey::PagedCount(PagedListKind::AllCardIds))
        .unwrap_or(0);
    let page = count / PAGE_SIZE_ALL_CARDS;
    let off = count % PAGE_SIZE_ALL_CARDS;
    let key = DataKey::PagedList(PagedListKind::AllCardIds, page);
    let mut page_vec: Vec<TokenId> = e.storage().persistent().get(&key).unwrap_or(Vec::new(e));
    if off == page_vec.len() {
        page_vec.push_back(token_id);
    } else if off < page_vec.len() {
        page_vec.set(off, token_id);
    } else {
        panic!("PagedList(AllCardIds) corrupted: offset beyond page length");
    }
    e.storage().persistent().set(&key, &page_vec);
    e.storage().persistent().extend_ttl(
        &key,
        STORAGE_THRESHOLD_LEDGERS,
        STORAGE_BUMP_LEDGERS,
    );
    e.storage()
        .persistent()
        .set(&DataKey::PagedCount(PagedListKind::AllCardIds), &(count + 1));
    e.storage().persistent().extend_ttl(
        &DataKey::PagedCount(PagedListKind::AllCardIds),
        STORAGE_THRESHOLD_LEDGERS,
        STORAGE_BUMP_LEDGERS,
    );
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
