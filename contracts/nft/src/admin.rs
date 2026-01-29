#![allow(deprecated)]

use crate::storage_types::*;
use soroban_sdk::{symbol_short, Address, Env};

pub fn has_administrator(e: &Env) -> bool {
    let key = DataKey::Admin;
    e.storage().instance().has(&key)
}

pub fn read_administrator(e: &Env) -> Address {
    let key = DataKey::Admin;
    e.storage().instance().get(&key).unwrap()
}

pub fn write_administrator(env: &Env, id: &Address) {
    let key = DataKey::Admin;
    env.storage().instance().set(&key, id);
}

#[allow(dead_code)]
pub fn is_whitelisted(e: &Env, member: &Address) -> bool {
    let key = DataKey::Whitelist(member.clone());
    if e.storage().persistent().has(&key) {
        e.storage().persistent().extend_ttl(
            &key,
            STORAGE_THRESHOLD_LEDGERS,
            STORAGE_BUMP_LEDGERS,
        );
        e.storage().persistent().get(&key).unwrap_or(false)
    } else {
        false
    }
}

// GUARDRAIL: Single-writer pattern helper for Config
pub fn update_config<F>(e: &Env, f: F)
where
    F: FnOnce(&Env, &mut Config),
{
    let mut config = read_config(e);
    f(e, &mut config);
    write_config(e, &config);
}

pub(crate) fn write_config(e: &Env, config: &Config) {
    let key: DataKey = DataKey::Config;
    e.storage().persistent().set(&key, config);
    e.storage().persistent().extend_ttl(
        &key,
        STORAGE_THRESHOLD_LEDGERS,
        STORAGE_BUMP_LEDGERS,
    );
}

pub fn read_config(e: &Env) -> Config {
    let key = DataKey::Config;
    let config = e.storage().persistent().get(&key).expect("Config not found");
    e.storage().persistent().extend_ttl(
        &key,
        STORAGE_THRESHOLD_LEDGERS,
        STORAGE_BUMP_LEDGERS,
    );
    config
}

pub fn is_pages_only(e: &Env) -> bool {
    let key = symbol_short!("pg_only");
    if let Some(value) = e.storage().persistent().get::<_, bool>(&key) {
        e.storage().persistent().extend_ttl(
            &key,
            STORAGE_THRESHOLD_LEDGERS,
            STORAGE_BUMP_LEDGERS,
        );
        value
    } else {
        false
    }
}

pub fn set_pages_only(e: &Env, enabled: bool) {
    let key = symbol_short!("pg_only");
    e.storage().persistent().set(&key, &enabled);
    e.storage().persistent().extend_ttl(
        &key,
        STORAGE_THRESHOLD_LEDGERS,
        STORAGE_BUMP_LEDGERS,
    );
}

pub fn read_total_effective_deck_power(e: &Env) -> u32 {
    let key = symbol_short!("deff");
    if let Some(value) = e.storage().persistent().get::<_, u32>(&key) {
        e.storage().persistent().extend_ttl(
            &key,
            STORAGE_THRESHOLD_LEDGERS,
            STORAGE_BUMP_LEDGERS,
        );
        value
    } else {
        0
    }
}

pub fn write_total_effective_deck_power(e: &Env, value: u32) {
    let key = symbol_short!("deff");
    e.storage().persistent().set(&key, &value);
    e.storage().persistent().extend_ttl(
        &key,
        STORAGE_THRESHOLD_LEDGERS,
        STORAGE_BUMP_LEDGERS,
    );
}

pub fn update_total_effective_deck_power<F>(e: &Env, f: F)
where
    F: FnOnce(&Env, &mut u32),
{
    let mut value = read_total_effective_deck_power(e);
    f(e, &mut value);
    write_total_effective_deck_power(e, value);
}

pub(crate) fn write_balance(e: &Env, balance: &Balance) {
    // Balance(u32) to track the different tokens balance???
    let key = DataKey::Balance;
    e.storage().persistent().set(&key, balance);
    e.storage().persistent().extend_ttl(
        &key,
        STORAGE_THRESHOLD_LEDGERS,
        STORAGE_BUMP_LEDGERS,
    );
}

pub fn read_balance(e: &Env) -> Balance {
    let key = DataKey::Balance;
    if let Some(balance) = e.storage().persistent().get(&key) {
        e.storage().persistent().extend_ttl(
            &key,
            STORAGE_THRESHOLD_LEDGERS,
            STORAGE_BUMP_LEDGERS,
        );
        balance
    } else {
        Balance {
            admin_terry: 0,
            admin_power: 0,
            haw_ai_terry: 0,
            haw_ai_power: 0,
            haw_ai_xtar: 0,
            total_deck_power: 0,
        }
    }
}

// GUARDRAIL: Single-writer pattern helper for Balance
pub fn update_balance<F>(e: &Env, f: F)
where
    F: FnOnce(&Env, &mut Balance),
{
    let mut balance = read_balance(e);
    f(e, &mut balance);
    write_balance(e, &balance);
}

pub(crate) fn write_state(e: &Env, state: &State) {
    let key = DataKey::State;
    e.storage().persistent().set(&key, state);
    e.storage().persistent().extend_ttl(
        &key,
        STORAGE_THRESHOLD_LEDGERS,
        STORAGE_BUMP_LEDGERS,
    );

    e.events().publish((symbol_short!("state"),), state.clone());
}

pub fn read_state(e: &Env) -> State {
    let key = DataKey::State;
    if let Some(state) = e.storage().persistent().get(&key) {
        e.storage().persistent().extend_ttl(
            &key,
            STORAGE_THRESHOLD_LEDGERS,
            STORAGE_BUMP_LEDGERS,
        );
        state
    } else {
        State {
            total_offer: 0,
            total_demand: 0,
            total_interest: 0,
            total_loan_duration: 0,
            total_loan_count: 0,
            borrowed_time_seconds: 0,
            loans_time_seconds: 0,
            last_update_ts: e.ledger().timestamp(),
            active_loans: 0,
            l_index: 0,
            w_total: 0,
            total_staked_power: 0,
            total_borrowed_power: 0,
        }
    }
}

// GUARDRAIL: Single-writer pattern helper for State
pub fn update_state<F>(e: &Env, f: F)
where
    F: FnOnce(&Env, &mut State),
{
    let mut state = read_state(e);
    f(e, &mut state);
    write_state(e, &state);
}

pub fn add_level(e: &Env, level: Level) -> u32 {
    let level_id = get_and_increase_level_id(&e);
    e.storage()
        .persistent()
        .set(&DataKey::Level(level_id), &level);
    e.storage().persistent().extend_ttl(
        &DataKey::Level(level_id),
        STORAGE_THRESHOLD_LEDGERS,
        STORAGE_BUMP_LEDGERS,
    );

    level_id
}

pub fn update_level(e: &Env, level_id: u32, level: Level) {
    e.storage()
        .persistent()
        .set(&DataKey::Level(level_id), &level);
    e.storage().persistent().extend_ttl(
        &DataKey::Level(level_id),
        STORAGE_THRESHOLD_LEDGERS,
        STORAGE_BUMP_LEDGERS,
    );
}

pub fn get_and_increase_level_id(env: &Env) -> u32 {
    let key = DataKey::LevelId;
    // We don't extend TTL before reading because it might not exist (initialization).
    // If it exists and is archived, get() should restore it.
    let prev = env.storage().persistent().get(&key).unwrap_or(0u32);
    
    // We don't need to explicitly extend here because we are about to set (write) it,
    // which effectively refreshes it or we extend after write.

    env.storage()
        .persistent()
        .set(&key, &(prev + 1));
    // Extend TTL on write
    env.storage().persistent().extend_ttl(
        &key,
        STORAGE_THRESHOLD_LEDGERS,
        STORAGE_BUMP_LEDGERS,
    );
    prev + 1
}

// Vault management functions
pub(crate) fn write_contract_vault(e: &Env, vault: &ContractVault) {
    let key = DataKey::ContractVault;
    e.storage().persistent().set(&key, vault);
    e.storage().persistent().extend_ttl(
        &key,
        STORAGE_THRESHOLD_LEDGERS,
        STORAGE_BUMP_LEDGERS,
    );
}

pub fn read_contract_vault(e: &Env) -> ContractVault {
    let key = DataKey::ContractVault;
    if let Some(vault) = e.storage().persistent().get(&key) {
        e.storage().persistent().extend_ttl(
            &key,
            STORAGE_THRESHOLD_LEDGERS,
            STORAGE_BUMP_LEDGERS,
        );
        vault
    } else {
        ContractVault {
            haw_ai_pot_terry: 0,
            haw_ai_pot_power: 0,
            haw_ai_pot_xtar: 0,
            dogstar_terry: 0,
            dogstar_power: 0,
            dogstar_xtar: 0,
            total_claimable_terry: 0,
            total_claimable_power: 0,
            total_claimable_xtar: 0,
        }
    }
}

// GUARDRAIL: Single-writer pattern helper for ContractVault
pub fn update_contract_vault<F>(e: &Env, f: F)
where
    F: FnOnce(&Env, &mut ContractVault),
{
    let mut vault = read_contract_vault(e);
    f(e, &mut vault);
    write_contract_vault(e, &vault);
}

pub(crate) fn write_user_claimable_balance(e: &Env, user: &Address, balance: &UserClaimableBalance) {
    let key = DataKey::UserClaimableBalance(user.clone());
    e.storage().persistent().set(&key, balance);
    e.storage().persistent().extend_ttl(
        &key,
        STORAGE_THRESHOLD_LEDGERS,
        STORAGE_BUMP_LEDGERS,
    );
}

pub fn read_user_claimable_balance(e: &Env, user: &Address) -> UserClaimableBalance {
    let key = DataKey::UserClaimableBalance(user.clone());
    if let Some(balance) = e.storage().persistent().get(&key) {
        e.storage().persistent().extend_ttl(
            &key,
            STORAGE_THRESHOLD_LEDGERS,
            STORAGE_BUMP_LEDGERS,
        );
        balance
    } else {
        UserClaimableBalance {
            terry: 0,
            power: 0,
            xtar: 0,
            last_claim_round: 0,
            last_claim_timestamp: 0,
        }
    }
}

// GUARDRAIL: Single-writer pattern helper for UserClaimableBalance
pub fn update_user_claimable_balance<F>(e: &Env, user: &Address, f: F)
where
    F: FnOnce(&Env, &mut UserClaimableBalance),
{
    let mut balance = read_user_claimable_balance(e, user);
    f(e, &mut balance);
    write_user_claimable_balance(e, user, &balance);
}

#[allow(dead_code)]
pub(crate) fn write_dogstar_claimable(e: &Env, balance: &UserClaimableBalance) {
    let key = DataKey::DogstarClaimableBalance;
    e.storage().persistent().set(&key, balance);
    e.storage().persistent().extend_ttl(
        &key,
        STORAGE_THRESHOLD_LEDGERS,
        STORAGE_BUMP_LEDGERS,
    );
}

#[allow(dead_code)]
pub fn read_dogstar_claimable(e: &Env) -> UserClaimableBalance {
    let key = DataKey::DogstarClaimableBalance;
    if let Some(balance) = e.storage().persistent().get(&key) {
        e.storage().persistent().extend_ttl(
            &key,
            STORAGE_THRESHOLD_LEDGERS,
            STORAGE_BUMP_LEDGERS,
        );
        balance
    } else {
        UserClaimableBalance {
            terry: 0,
            power: 0,
            xtar: 0,
            last_claim_round: 0,
            last_claim_timestamp: 0,
        }
    }
}

// Function to extend TTL of all critical global keys (maintenance)
pub fn touch_globals(e: &Env) {
    // 1. Config
    let key_config = DataKey::Config;
    e.storage().persistent().extend_ttl(
        &key_config,
        STORAGE_THRESHOLD_LEDGERS,
        STORAGE_BUMP_LEDGERS,
    );

    // 2. Balance
    let key_balance = DataKey::Balance;
    e.storage().persistent().extend_ttl(
        &key_balance,
        STORAGE_THRESHOLD_LEDGERS,
        STORAGE_BUMP_LEDGERS,
    );

    // 3. State
    let key_state = DataKey::State;
    e.storage().persistent().extend_ttl(
        &key_state,
        STORAGE_THRESHOLD_LEDGERS,
        STORAGE_BUMP_LEDGERS,
    );

    // 4. ContractVault
    let key_vault = DataKey::ContractVault;
    e.storage().persistent().extend_ttl(
        &key_vault,
        STORAGE_THRESHOLD_LEDGERS,
        STORAGE_BUMP_LEDGERS,
    );

    // 5. Dogstar Claimable Balance
    let key_dogstar = DataKey::DogstarClaimableBalance;
    e.storage().persistent().extend_ttl(
        &key_dogstar,
        STORAGE_THRESHOLD_LEDGERS,
        STORAGE_BUMP_LEDGERS,
    );

    // 6. LevelId and Levels
    let key_level_id = DataKey::LevelId;
    e.storage().persistent().extend_ttl(
        &key_level_id,
        STORAGE_THRESHOLD_LEDGERS,
        STORAGE_BUMP_LEDGERS,
    );
    
    // We try to read LevelId to iterate levels. 
    // If it fails (really missing), we just skip level iteration but we tried to extend LevelId above.
    if let Some(last_level_id) = e.storage().persistent().get::<_, u32>(&key_level_id) {
        // Touch all levels
        for i in 1..=last_level_id {
            let key_level = DataKey::Level(i);
            e.storage().persistent().extend_ttl(
                &key_level,
                STORAGE_THRESHOLD_LEDGERS,
                STORAGE_BUMP_LEDGERS,
            );
        }
    }

    // 7. TokenIdCounter (CRITICAL)
    let key_counter = DataKey::TokenIdCounter;
    e.storage().persistent().extend_ttl(
        &key_counter,
        STORAGE_THRESHOLD_LEDGERS,
        STORAGE_BUMP_LEDGERS,
    );

    // 8. PotBalance (CRITICAL)
    let key_pot = DataKey::PotBalance;
    e.storage().persistent().extend_ttl(
        &key_pot,
        STORAGE_THRESHOLD_LEDGERS,
        STORAGE_BUMP_LEDGERS,
    );

    // 9. AllCardIds
    let key_all_cards = DataKey::AllCardIds;
    e.storage().persistent().extend_ttl(
        &key_all_cards,
        STORAGE_THRESHOLD_LEDGERS,
        STORAGE_BUMP_LEDGERS,
    );

    // 10. Global Action Lists
    let global_lists = [
        DataKey::Decks,
        DataKey::Stakes,
        DataKey::Lendings,
        DataKey::Borrowings,
        DataKey::Fights,
        DataKey::CurrentRound,
        DataKey::AllRounds,
        DataKey::RegisteredTokens,
    ];

    for key in global_lists.iter() {
        e.storage().persistent().extend_ttl(
            key,
            STORAGE_THRESHOLD_LEDGERS,
            STORAGE_BUMP_LEDGERS,
        );
    }

    // 11. Paged index counts and migration cursors (no per-page scan here)
    let page_meta = [
        DataKey::PagedCount(PagedListKind::Stakes),
        DataKey::PagedCount(PagedListKind::Fights),
        DataKey::PagedCount(PagedListKind::Lendings),
        DataKey::PagedCount(PagedListKind::Borrowings),
        DataKey::PagedCount(PagedListKind::Decks),
        DataKey::PagedCount(PagedListKind::Rounds),
        DataKey::MigrationCursor(PagedListKind::Stakes),
        DataKey::MigrationCursor(PagedListKind::Fights),
        DataKey::MigrationCursor(PagedListKind::Lendings),
        DataKey::MigrationCursor(PagedListKind::Borrowings),
        DataKey::MigrationCursor(PagedListKind::Decks),
        DataKey::MigrationCursor(PagedListKind::Rounds),
    ];

    for key in page_meta.iter() {
        if e.storage().persistent().has(key) {
            e.storage().persistent().extend_ttl(
                key,
                STORAGE_THRESHOLD_LEDGERS,
                STORAGE_BUMP_LEDGERS,
            );
        }
    }
}
