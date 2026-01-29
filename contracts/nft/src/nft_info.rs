use soroban_sdk::{contracttype, log, Address, Env};

use crate::storage_types::{DataKey, TokenId, STORAGE_BUMP_LEDGERS, STORAGE_THRESHOLD_LEDGERS};

#[derive(Debug, Clone, PartialEq, Eq)]
#[contracttype]
pub enum Category {
    Leader,
    Resource,
    Skill,
    Weapon,
}

#[derive(Clone, PartialEq)]
#[contracttype]
pub enum Currency {
    Terry,
    Xtar,
}

#[derive(Debug, Clone, PartialEq)]
#[contracttype]
pub enum Action {
    None,
    Stake,
    Fight,
    Lend,
    Borrow,
    Burn,
    Deck,
    Mint,
}

#[contracttype]
#[derive(Clone)]
pub struct Card {
    pub power: u32,
    pub locked_by_action: Action,
}

#[derive(Clone)]
#[contracttype]
pub struct CardInfo {
    pub initial_power: u32,
    pub max_power: u32,
    pub price_xtar: i128,
    pub price_terry: i128,
}

impl CardInfo {
    pub fn get_default_card(_category: Category) -> Self {
        Self {
            initial_power: 1000,
            max_power: 10000,
            price_terry: 100,
            price_xtar: 100,
        }
    }
}

pub(crate) fn write_nft(env: &Env, owner: Address, token_id: TokenId, card: Card) {
    log!(
        &env,
        "write_nft >> Write nft for {}, token id {}",
        owner.clone(),
        token_id.clone()
    );
    let key = DataKey::Card(owner, token_id);
    env.storage().persistent().set(&key, &card);
    env.storage()
        .persistent()
        .extend_ttl(&key, STORAGE_THRESHOLD_LEDGERS, STORAGE_BUMP_LEDGERS);
}

pub fn read_nft(env: &Env, owner: Address, token_id: TokenId) -> Option<Card> {
    log!(
        &env,
        "read_nft >> Read nft for {}, token id {}",
        owner.clone(),
        token_id.clone()
    );
    let key = DataKey::Card(owner, token_id);
    if let Some(card) = env.storage().persistent().get::<_, Card>(&key) {
        env.storage().persistent().extend_ttl(
            &key,
            STORAGE_THRESHOLD_LEDGERS,
            STORAGE_BUMP_LEDGERS,
        );
        Some(card)
    } else {
        None
    }
}

pub fn exists(env: &Env, owner: Address, token_id: TokenId) -> bool {
    let key: DataKey = DataKey::Card(owner, token_id.clone());
    env.storage().persistent().has(&key)
}

pub(crate) fn remove_nft(env: &Env, owner: Address, token_id: TokenId) {
    let key = DataKey::Card(owner, token_id.clone());
    env.storage().persistent().remove(&key);
}


// GUARDRAIL: Single-writer helper for NFTs
pub fn update_nft<F>(env: &Env, owner: Address, token_id: TokenId, f: F)
where
    F: FnOnce(&Env, &mut Card),
{
    let mut card = read_nft(env, owner.clone(), token_id.clone())
        .expect("NFT not found for update");
    f(env, &mut card);
    write_nft(env, owner, token_id, card);
}
