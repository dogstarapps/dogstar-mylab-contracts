use crate::error::MyLabError;
use crate::nft_info::read_nft;
use crate::storage_types::{
    DataKey, Level, TokenId, User, STORAGE_BUMP_LEDGERS, STORAGE_THRESHOLD_LEDGERS,
};
use soroban_sdk::{log, Address, Env, Vec};

pub fn add_card_to_owner(env: &Env, token_id: TokenId, user: Address) -> Result<(), MyLabError> {
    log!(&env, "Add card to owner function");
    if let Some(card) = read_nft(&env, user.clone(), token_id.clone()) {
        log!(&env, "add_card_to_owner >> Found card {}", card.clone());
        update_owner_cards(env, user.clone(), |_, cards| {
            if cards.iter().position(|id| id == token_id.clone()).is_none() {
                cards.push_back(token_id.clone());
            }
        });
        Ok(())
    } else {
        log!(
            &env,
            "add_card_to_owner >> Card not found in add_card_to_owner"
        );
        return Err(MyLabError::NotNFT);
    }
}

// TODO:
// read_user shoudld either return a user or return an error of not found

pub fn read_user(e: &Env, user: Address) -> User {
    let key = DataKey::User(user.clone());
    if let Some(user_data) = e.storage().persistent().get::<_, User>(&key) {
        e.storage().persistent().extend_ttl(
            &key,
            STORAGE_THRESHOLD_LEDGERS,
            STORAGE_BUMP_LEDGERS,
        );
        user_data
    } else {
        User {
            owner: user,
            power: 0,
            terry: 0,
            total_history_terry: 0,
            level: 1,
        }
    }
}

pub(crate) fn write_user(e: &Env, user: Address, mut user_info: User) {
    // GUARDRAIL: Always recalculate level based on total_history_terry before writing.
    // This ensures consistency even if the caller modified terry but forgot to update level,
    // or if the caller read stale level data.
    user_info.level = calculate_level_from_balance(e, user_info.total_history_terry);

    let key = DataKey::User(user);
    e.storage().persistent().set(&key, &user_info);
    e.storage().persistent().extend_ttl(
        &key,
        STORAGE_THRESHOLD_LEDGERS,
        STORAGE_BUMP_LEDGERS,
    );
}

// GUARDRAIL A: Single-writer pattern helper
pub fn update_user<F>(e: &Env, owner: Address, f: F)
where
    F: FnOnce(&Env, &mut User),
{
    let mut user = read_user(e, owner.clone());
    f(e, &mut user);
    write_user(e, owner, user);
}

pub fn get_user_level(e: &Env, user: Address) -> u32 {
    let user = read_user(&e, user.clone());
    let balance = user.total_history_terry;
    log!(&e, "get_user_level >> User balance {}", balance);

    calculate_level_from_balance(e, balance)
}

fn calculate_level_from_balance(e: &Env, balance: i128) -> u32 {
    // Fetch the last level ID from storage - Using direct get to enable auto-restore
    let key_level_id = DataKey::LevelId;
    
    let last_level_id = e
        .storage()
        .persistent()
        .get::<_, u32>(&key_level_id)
        .expect("Level configuration missing: LevelId not found");
        
    e.storage().persistent().extend_ttl(
        &key_level_id,
        STORAGE_THRESHOLD_LEDGERS,
        STORAGE_BUMP_LEDGERS,
    );

    let mut best_fit_level = 1;

    for i in 1..=last_level_id {
        let key = DataKey::Level(i);
        
        let level: Level = e.storage().persistent().get(&key)
            .expect("Level configuration corrupted: Missing level data");
            
        // Direct get for auto-restore
        e.storage().persistent().extend_ttl(
            &key,
            STORAGE_THRESHOLD_LEDGERS,
            STORAGE_BUMP_LEDGERS,
        );
        
        if balance >= level.minimum_terry {
            best_fit_level = i;
            if balance <= level.maximum_terry {
                return i;
            }
        }
    }

    // Return the highest level found if balance exceeds all maximums
    best_fit_level
}

fn write_owner_card(env: &Env, owner: Address, token_ids: Vec<TokenId>) {
    log!(
        &env,
        "write_owner_card >> Write owner card for {}, token_ids {}",
        owner.clone(),
        token_ids.clone()
    );
    let key = DataKey::OwnerOwnedCardIds(owner);
    env.storage().persistent().set(&key, &token_ids);
    env.storage().persistent().extend_ttl(
        &key,
        STORAGE_THRESHOLD_LEDGERS,
        STORAGE_BUMP_LEDGERS,
    );
}

// GUARDRAIL C: Single-writer pattern helper for owner cards
pub fn update_owner_cards<F>(e: &Env, owner: Address, f: F)
where
    F: FnOnce(&Env, &mut Vec<TokenId>),
{
    let mut cards = read_owner_card(e, owner.clone());
    f(e, &mut cards);
    write_owner_card(e, owner, cards);
}

pub fn read_owner_card(env: &Env, owner: Address) -> Vec<TokenId> {
    log!(
        &env,
        "read_owner_card >> Read owner card for {}",
        owner.clone()
    );
    let key = DataKey::OwnerOwnedCardIds(owner.clone());

    if let Some(card_list) = env.storage().persistent().get(&key) {
        env.storage().persistent().extend_ttl(
            &key,
            STORAGE_THRESHOLD_LEDGERS,
            STORAGE_BUMP_LEDGERS,
        );
        card_list
    } else {
        log!(&env, "Not found cards for owner {}", owner.clone());
        // Persist empty list so the key exists for new users
        let empty = Vec::new(&env);
        env.storage().persistent().set(&key, &empty);
        env.storage().persistent().extend_ttl(
            &key,
            STORAGE_THRESHOLD_LEDGERS,
            STORAGE_BUMP_LEDGERS,
        );
        empty
    }
}

pub fn mint_terry(e: &Env, owner: Address, amount: i128) {
    update_user(e, owner, |_, user| {
        user.terry += amount;
        user.total_history_terry += amount;
    });
}

pub fn burn_terry(e: &Env, owner: Address, amount: i128) {
    update_user(e, owner, |_, user| {
        assert!(user.terry >= amount, "Not enough terry to burn");
        user.terry -= amount;
    });
    log!(&e, "burn_terry >> Burned terry {}", amount);
}
