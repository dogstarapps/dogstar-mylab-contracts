use crate::actions::deck::{read_deck, read_decks_count, read_decks_page};
use crate::event::*;
use crate::storage_types::{
    DataKey, Deck, DogstarBalance, PagedListKind, PendingReward, PlayerReward, PotBalance,
    PotSnapshot, PAGE_SIZE_DECKS, PAGE_SIZE_ROUNDS, STORAGE_BUMP_LEDGERS, STORAGE_THRESHOLD_LEDGERS,
};
use crate::admin::{is_pages_only, read_config, read_contract_vault, write_contract_vault, update_contract_vault};
use crate::event::*;
use crate::nft_info::{Action, Category, read_nft};
use crate::metadata::read_metadata;
use crate::user_info::read_user;
use soroban_sdk::{symbol_short, Address, Env, Vec};

/// Calculates effective power by applying the deck bonus to base power.
pub fn calculate_effective_power(base_power: u32, deck_bonus: u32) -> u32 {
    let base = base_power as u64;
    let bonus = (100 + deck_bonus) as u64;
    ((base.saturating_mul(bonus)) / 100) as u32
}

// Pot Balance Management
pub fn read_pot_balance(env: &Env) -> PotBalance {
    let key = DataKey::PotBalance;
    if let Some(balance) = env.storage().persistent().get(&key) {
        env.storage().persistent().extend_ttl(
            &key,
            STORAGE_THRESHOLD_LEDGERS,
            STORAGE_BUMP_LEDGERS,
        );
        balance
    } else {
        PotBalance {
            accumulated_terry: 0,
            accumulated_power: 0,
            accumulated_xtar: 0,
            last_opening_round: 0,
            total_openings: 0,
            last_updated: env.ledger().timestamp(),
        }
    }
}

pub fn read_pot_in_progress_round(env: &Env) -> Option<u32> {
    let key = symbol_short!("pot_ip");
    if let Some(round) = env.storage().persistent().get::<_, u32>(&key) {
        env.storage().persistent().extend_ttl(
            &key,
            STORAGE_THRESHOLD_LEDGERS,
            STORAGE_BUMP_LEDGERS,
        );
        Some(round)
    } else {
        None
    }
}

pub fn write_pot_in_progress_round(env: &Env, round: u32) {
    let key = symbol_short!("pot_ip");
    env.storage().persistent().set(&key, &round);
    env.storage().persistent().extend_ttl(
        &key,
        STORAGE_THRESHOLD_LEDGERS,
        STORAGE_BUMP_LEDGERS,
    );
}

pub fn clear_pot_in_progress_round(env: &Env) {
    let key = symbol_short!("pot_ip");
    env.storage().persistent().remove(&key);
}

pub(crate) fn write_pot_balance(env: &Env, balance: &PotBalance) {
    env.storage()
        .persistent()
        .set(&DataKey::PotBalance, balance);
    env.storage().persistent().extend_ttl(
        &DataKey::PotBalance,
        STORAGE_THRESHOLD_LEDGERS,
        STORAGE_BUMP_LEDGERS,
    );
}

// GUARDRAIL: Single-writer pattern helper for PotBalance
pub fn update_pot_balance<F>(e: &Env, f: F)
where
    F: FnOnce(&Env, &mut PotBalance),
{
    let mut balance = read_pot_balance(e);
    f(e, &mut balance);
    write_pot_balance(e, &balance);
}

pub fn read_dogstar_balance(env: &Env) -> DogstarBalance {
    let key = DataKey::DogstarBalance;
    if let Some(balance) = env.storage().persistent().get(&key) {
        env.storage().persistent().extend_ttl(
            &key,
            STORAGE_THRESHOLD_LEDGERS,
            STORAGE_BUMP_LEDGERS,
        );
        balance
    } else {
        DogstarBalance {
            terry: 0,
            power: 0,
            xtar: 0,
        }
    }
}

pub(crate) fn write_dogstar_balance(env: &Env, balance: &DogstarBalance) {
    env.storage()
        .persistent()
        .set(&DataKey::DogstarBalance, balance);
    env.storage().persistent().extend_ttl(
        &DataKey::DogstarBalance,
        STORAGE_THRESHOLD_LEDGERS,
        STORAGE_BUMP_LEDGERS,
    );
}

// GUARDRAIL: Single-writer pattern helper for DogstarBalance
pub fn update_dogstar_balance<F>(e: &Env, f: F)
where
    F: FnOnce(&Env, &mut DogstarBalance),
{
    let mut balance = read_dogstar_balance(e);
    f(e, &mut balance);
    write_dogstar_balance(e, &balance);
}

// Internal helper to accumulate pot balances and dogstar fees without requiring admin auth.
// This is intended to be called from trusted internal flows like mint/burn.
pub fn accumulate_pot_internal(env: &Env, terry: i128, power: u32, xtar: i128, from: Option<Address>, action: Option<Action>) {
    let config = read_config(env);

    // Calculate fees in basis points
    let fee_percentage = config.dogstar_fee_percentage;
    let terry_fee = (terry * fee_percentage as i128) / 10000;
    let power_fee = (power * fee_percentage) / 10000;
    let xtar_fee = (xtar * fee_percentage as i128) / 10000;

    // Accumulate in pot balance (minus dogstar fees)
    update_pot_balance(env, |_, pot_balance| {
        pot_balance.accumulated_terry += terry - terry_fee;
        pot_balance.accumulated_power += power - power_fee;
        pot_balance.accumulated_xtar += xtar - xtar_fee;
        pot_balance.last_updated = env.ledger().timestamp();
    });

    // Update contract vault with new fees
    update_contract_vault(env, |_, vault| {
        vault.dogstar_terry += terry_fee;
        vault.dogstar_power += power_fee;
        vault.dogstar_xtar += xtar_fee;
    });

    // Update legacy dogstar balance for backward-compat events/UI
    update_dogstar_balance(env, |_, dogstar_balance| {
        dogstar_balance.terry += terry_fee;
        dogstar_balance.power += power_fee;
        dogstar_balance.xtar += xtar_fee;
    });

    if terry_fee > 0 || power_fee > 0 || xtar_fee > 0 {
        emit_dogstar_fee_accumulated(env, terry_fee, power_fee, xtar_fee, fee_percentage, from, action);
    }
}

pub fn read_dogstar_generic_fee(env: &Env, token: &Address) -> i128 {
    let key = DataKey::DogstarGenericFee(token.clone());
    if let Some(fee) = env.storage().persistent().get(&key) {
        env.storage().persistent().extend_ttl(
            &key,
            STORAGE_THRESHOLD_LEDGERS,
            STORAGE_BUMP_LEDGERS,
        );
        fee
    } else {
        0
    }
}

pub(crate) fn write_dogstar_generic_fee(env: &Env, token: &Address, amount: i128) {
    let key = DataKey::DogstarGenericFee(token.clone());
    env.storage().persistent().set(&key, &amount);
    env.storage().persistent().extend_ttl(
        &key,
        STORAGE_THRESHOLD_LEDGERS,
        STORAGE_BUMP_LEDGERS,
    );
}

// GUARDRAIL: Single-writer pattern helper for DogstarGenericFee
pub fn update_dogstar_generic_fee<F>(e: &Env, token: &Address, f: F)
where
    F: FnOnce(&Env, &mut i128),
{
    let mut fee = read_dogstar_generic_fee(e, token);
    f(e, &mut fee);
    write_dogstar_generic_fee(e, token, fee);
}

// Snapshot Management
pub(crate) fn write_pot_snapshot(env: &Env, round: u32, snapshot: &PotSnapshot) {
    let key = DataKey::OpeningSnapshot(round);

    env.storage().persistent().set(&key, snapshot);
    env.storage().persistent().extend_ttl(
        &key,
        STORAGE_THRESHOLD_LEDGERS,
        STORAGE_BUMP_LEDGERS,
    );
}

// Multi‑asset helpers
pub fn read_registered_tokens(env: &Env) -> Vec<Address> {
    let key = DataKey::RegisteredTokens;
    if let Some(tokens) = env.storage().persistent().get(&key) {
        env.storage().persistent().extend_ttl(
            &key,
            STORAGE_THRESHOLD_LEDGERS,
            STORAGE_BUMP_LEDGERS,
        );
        tokens
    } else {
        Vec::new(env)
    }
}

// GUARDRAIL: Single-writer pattern helper for RegisteredTokens
pub fn update_registered_tokens<F>(e: &Env, f: F)
where
    F: FnOnce(&Env, &mut Vec<Address>),
{
    let mut tokens = read_registered_tokens(e);
    f(e, &mut tokens);
    write_registered_tokens(e, &tokens);
}

pub(crate) fn write_registered_tokens(env: &Env, tokens: &Vec<Address>) {
    env.storage()
        .persistent()
        .set(&DataKey::RegisteredTokens, tokens);
    env.storage().persistent().extend_ttl(
        &DataKey::RegisteredTokens,
        STORAGE_THRESHOLD_LEDGERS,
        STORAGE_BUMP_LEDGERS,
    );
}

pub fn read_accumulated_by_token(env: &Env, token: &Address) -> i128 {
    let key = DataKey::AccumulatedByToken(token.clone());
    if let Some(amount) = env.storage().persistent().get(&key) {
        env.storage().persistent().extend_ttl(
            &key,
            STORAGE_THRESHOLD_LEDGERS,
            STORAGE_BUMP_LEDGERS,
        );
        amount
    } else {
        0
    }
}

pub(crate) fn write_accumulated_by_token(env: &Env, token: &Address, amount: i128) {
    let key = DataKey::AccumulatedByToken(token.clone());
    env.storage().persistent().set(&key, &amount);
    env.storage().persistent().extend_ttl(
        &key,
        STORAGE_THRESHOLD_LEDGERS,
        STORAGE_BUMP_LEDGERS,
    );
}

// GUARDRAIL: Single-writer pattern helper for AccumulatedByToken
pub fn update_accumulated_by_token<F>(e: &Env, token: &Address, f: F)
where
    F: FnOnce(&Env, &mut i128),
{
    let mut amount = read_accumulated_by_token(e, token);
    f(e, &mut amount);
    write_accumulated_by_token(e, token, amount);
}

pub fn read_user_generic_claimable(env: &Env, user: &Address, token: &Address) -> i128 {
    let key = DataKey::UserGenericClaimable(user.clone(), token.clone());
    if let Some(amount) = env.storage().persistent().get(&key) {
        env.storage().persistent().extend_ttl(
            &key,
            STORAGE_THRESHOLD_LEDGERS,
            STORAGE_BUMP_LEDGERS,
        );
        amount
    } else {
        0
    }
}

pub(crate) fn write_user_generic_claimable(env: &Env, user: &Address, token: &Address, amount: i128) {
    let key = DataKey::UserGenericClaimable(user.clone(), token.clone());
    env.storage().persistent().set(&key, &amount);
    env.storage().persistent().extend_ttl(
        &key,
        STORAGE_THRESHOLD_LEDGERS,
        STORAGE_BUMP_LEDGERS,
    );
}

// GUARDRAIL: Single-writer pattern helper for UserGenericClaimable
pub fn update_user_generic_claimable<F>(e: &Env, user: &Address, token: &Address, f: F)
where
    F: FnOnce(&Env, &mut i128),
{
    let mut amount = read_user_generic_claimable(e, user, token);
    f(e, &mut amount);
    write_user_generic_claimable(e, user, token, amount);
}

pub fn read_pot_snapshot(env: &Env, round: u32) -> Option<PotSnapshot> {
    let key = DataKey::OpeningSnapshot(round);
    if let Some(snapshot) = env.storage().persistent().get(&key) {
        env.storage().persistent().extend_ttl(
            &key,
            STORAGE_THRESHOLD_LEDGERS,
            STORAGE_BUMP_LEDGERS,
        );
        Some(snapshot)
    } else {
        None
    }
}

pub(crate) fn write_player_reward(env: &Env, round: u32, player: &Address, reward: &PlayerReward) {
    let key = DataKey::PlayerShare(round, player.clone());

    env.storage().persistent().set(&key, reward);
    env.storage().persistent().extend_ttl(
        &key,
        STORAGE_THRESHOLD_LEDGERS,
        STORAGE_BUMP_LEDGERS,
    );
}

pub fn read_player_reward(env: &Env, round: u32, player: &Address) -> Option<PlayerReward> {
    let key = DataKey::PlayerShare(round, player.clone());
    if let Some(reward) = env.storage().persistent().get(&key) {
        env.storage().persistent().extend_ttl(
            &key,
            STORAGE_THRESHOLD_LEDGERS,
            STORAGE_BUMP_LEDGERS,
        );
        Some(reward)
    } else {
        None
    }
}

pub(crate) fn write_pending_reward(env: &Env, round: u32, player: &Address, reward: &PendingReward) {
    let key = DataKey::PendingReward(round, player.clone());
    env.storage().persistent().set(&key, reward);
    env.storage().persistent().extend_ttl(
        &key,
        STORAGE_THRESHOLD_LEDGERS,
        STORAGE_BUMP_LEDGERS,
    );
}

pub fn read_pending_reward(env: &Env, round: u32, player: &Address) -> Option<PendingReward> {
    let key = DataKey::PendingReward(round, player.clone());
    if let Some(reward) = env.storage().persistent().get(&key) {
        env.storage().persistent().extend_ttl(
            &key,
            STORAGE_THRESHOLD_LEDGERS,
            STORAGE_BUMP_LEDGERS,
        );
        Some(reward)
    } else {
        None
    }
}

pub fn get_current_round(env: &Env) -> u32 {
    let key = DataKey::CurrentRound;
    if let Some(round) = env.storage().persistent().get(&key) {
        env.storage().persistent().extend_ttl(
            &key,
            STORAGE_THRESHOLD_LEDGERS,
            STORAGE_BUMP_LEDGERS,
        );
        round
    } else {
        0
    }
}

pub fn set_current_round(env: &Env, round: u32) {
    env.storage()
        .persistent()
        .set(&DataKey::CurrentRound, &round);
    env.storage().persistent().extend_ttl(
        &DataKey::CurrentRound,
        STORAGE_THRESHOLD_LEDGERS,
        STORAGE_BUMP_LEDGERS,
    );
}

pub fn read_pot_status(env: &Env, round: u32) -> crate::storage_types::PotStatus {
    let key = DataKey::PotStatus(round);
    env.storage()
        .persistent()
        .get::<_, crate::storage_types::PotStatus>(&key)
        .unwrap_or(crate::storage_types::PotStatus::Init)
}

fn pot_total_decks_key(round: u32) -> (soroban_sdk::Symbol, u32) {
    (symbol_short!("pot_tot"), round)
}

pub fn write_pot_total_decks(env: &Env, round: u32, total: u32) {
    let key = pot_total_decks_key(round);
    env.storage().persistent().set(&key, &total);
    env.storage().persistent().extend_ttl(
        &key,
        STORAGE_THRESHOLD_LEDGERS,
        STORAGE_BUMP_LEDGERS,
    );
}

pub fn read_pot_total_decks(env: &Env, round: u32) -> u32 {
    let key = pot_total_decks_key(round);
    if let Some(total) = env.storage().persistent().get::<_, u32>(&key) {
        env.storage().persistent().extend_ttl(
            &key,
            STORAGE_THRESHOLD_LEDGERS,
            STORAGE_BUMP_LEDGERS,
        );
        total
    } else {
        0
    }
}

pub fn write_pot_status(env: &Env, round: u32, status: crate::storage_types::PotStatus) {
    let key = DataKey::PotStatus(round);
    env.storage().persistent().set(&key, &status);
    env.storage().persistent().extend_ttl(
        &key,
        STORAGE_THRESHOLD_LEDGERS,
        STORAGE_BUMP_LEDGERS,
    );
}

pub fn read_pot_cursor(env: &Env, round: u32) -> u32 {
    let key = DataKey::PotCursor(round);
    env.storage().persistent().get::<_, u32>(&key).unwrap_or(0)
}

pub fn write_pot_cursor(env: &Env, round: u32, cursor: u32) {
    let key = DataKey::PotCursor(round);
    env.storage().persistent().set(&key, &cursor);
    env.storage().persistent().extend_ttl(
        &key,
        STORAGE_THRESHOLD_LEDGERS,
        STORAGE_BUMP_LEDGERS,
    );
}

pub fn get_all_rounds(env: &Env) -> Vec<u32> {
    if is_pages_only(env) {
        let count = env
            .storage()
            .persistent()
            .get::<_, u32>(&DataKey::PagedCount(PagedListKind::Rounds))
            .unwrap_or(0);
        let mut out = Vec::new(env);
        let mut idx = 0;
        while idx < count {
            if let Some(round) = read_round_at(env, idx) {
                out.push_back(round);
            }
            idx += 1;
        }
        return out;
    }
    let key = DataKey::AllRounds;
    if let Some(rounds) = env.storage().persistent().get(&key) {
        env.storage().persistent().extend_ttl(
            &key,
            STORAGE_THRESHOLD_LEDGERS,
            STORAGE_BUMP_LEDGERS,
        );
        rounds
    } else {
        Vec::new(env)
    }
}

pub fn get_rounds_count(env: &Env) -> u32 {
    let count = env
        .storage()
        .persistent()
        .get::<_, u32>(&DataKey::PagedCount(PagedListKind::Rounds))
        .unwrap_or(0);
    if count == 0 && !is_pages_only(env) {
        let legacy = get_all_rounds(env);
        if !legacy.is_empty() {
            return legacy.len();
        }
    }
    count
}

pub fn get_rounds_page(env: &Env, cursor: u32, limit: u32) -> Vec<u32> {
    if limit == 0 {
        return Vec::new(env);
    }
    let count = get_rounds_count(env);
    if !is_pages_only(env)
        && !env
            .storage()
            .persistent()
            .has(&DataKey::PagedCount(PagedListKind::Rounds))
    {
        let legacy = get_all_rounds(env);
        let total = legacy.len();
        if cursor >= total {
            return Vec::new(env);
        }
        let end = (cursor.saturating_add(limit)).min(total);
        let mut out: Vec<u32> = Vec::new(env);
        let mut idx = cursor;
        while idx < end {
            out.push_back(legacy.get(idx).unwrap());
            idx += 1;
        }
        return out;
    }
    if cursor >= count {
        return Vec::new(env);
    }
    let end = (cursor.saturating_add(limit)).min(count);
    let mut out: Vec<u32> = Vec::new(env);
    let mut idx = cursor;
    while idx < end {
        if let Some(round) = read_round_at(env, idx) {
            out.push_back(round);
        }
        idx += 1;
    }
    out
}

fn read_round_at(env: &Env, idx: u32) -> Option<u32> {
    let page = idx / PAGE_SIZE_ROUNDS;
    let off = idx % PAGE_SIZE_ROUNDS;
    let key = DataKey::PagedList(PagedListKind::Rounds, page);
    let page_vec: Vec<u32> = env.storage().persistent().get(&key).unwrap_or(Vec::new(env));
    if off < page_vec.len() {
        Some(page_vec.get(off).unwrap())
    } else {
        None
    }
}

fn write_rounds_count(env: &Env, count: u32) {
    env.storage()
        .persistent()
        .set(&DataKey::PagedCount(PagedListKind::Rounds), &count);
    env.storage().persistent().extend_ttl(
        &DataKey::PagedCount(PagedListKind::Rounds),
        STORAGE_THRESHOLD_LEDGERS,
        STORAGE_BUMP_LEDGERS,
    );
}

fn add_round_page_index(env: &Env, round: u32) {
    let count = env
        .storage()
        .persistent()
        .get::<_, u32>(&DataKey::PagedCount(PagedListKind::Rounds))
        .unwrap_or(0);
    let page = count / PAGE_SIZE_ROUNDS;
    let off = count % PAGE_SIZE_ROUNDS;
    let page_key = DataKey::PagedList(PagedListKind::Rounds, page);
    let mut page_vec: Vec<u32> = env.storage().persistent().get(&page_key).unwrap_or(Vec::new(env));
    if off == page_vec.len() {
        page_vec.push_back(round);
    } else if off < page_vec.len() {
        page_vec.set(off, round);
    } else {
        panic!("PagedList(Rounds) corrupted: offset beyond page length");
    }
    env.storage().persistent().set(&page_key, &page_vec);
    env.storage().persistent().extend_ttl(
        &page_key,
        STORAGE_THRESHOLD_LEDGERS,
        STORAGE_BUMP_LEDGERS,
    );
    write_rounds_count(env, count + 1);
}

pub fn add_round(env: &Env, round: u32) {
    let mut legacy_len: u32 = 0;
    if !is_pages_only(env) {
    let mut rounds = get_all_rounds(env);
    rounds.push_back(round);
        legacy_len = rounds.len();

    env.storage().persistent().set(&DataKey::AllRounds, &rounds);
    env.storage().persistent().extend_ttl(
        &DataKey::AllRounds,
        STORAGE_THRESHOLD_LEDGERS,
        STORAGE_BUMP_LEDGERS,
    );
    }

    // New paged index (idempotent)
    let stored_count = env
        .storage()
        .persistent()
        .get::<_, u32>(&DataKey::PagedCount(PagedListKind::Rounds))
        .unwrap_or(0);
    if is_pages_only(env) {
        add_round_page_index(env, round);
    } else if stored_count < legacy_len {
        add_round_page_index(env, round);
    }
}

pub fn migrate_rounds_to_pages(env: &Env, start: u32, limit: u32) -> u32 {
    let rounds = get_all_rounds(env);
    let total = rounds.len();
    if start >= total {
        return total;
    }
    let end = (start.saturating_add(limit)).min(total);
    let stored_count = env
        .storage()
        .persistent()
        .get::<_, u32>(&DataKey::PagedCount(PagedListKind::Rounds))
        .unwrap_or(0);
    let mut idx = start;
    while idx < end {
        let round = rounds.get(idx).unwrap();
        if stored_count < rounds.len() {
            add_round_page_index(env, round);
        }
        idx += 1;
    }
    end
}

pub fn get_eligible_players(env: &Env) -> Vec<Address> {
    let mut eligible_players = Vec::new(env);
    let total = read_decks_count(env);
    let mut cursor: u32 = 0;
    let limit = PAGE_SIZE_DECKS;
    while cursor < total {
        let page = read_decks_page(env, cursor, limit);
        if page.is_empty() {
            break;
        }
        for key in page.iter() {
            let deck = read_deck(env.clone(), key.owner.clone());
        if deck.token_ids.len() == 4 {
            eligible_players.push_back(deck.owner);
        }
        }
        cursor = cursor.saturating_add(limit);
    }

    eligible_players
}

pub fn get_eligible_players_count(env: &Env) -> u32 {
    // Count is based on deck pages to match cursor semantics in get_eligible_players_page.
    read_decks_count(env)
}

pub fn get_eligible_players_page(env: &Env, cursor: u32, limit: u32) -> Vec<Address> {
    // Cursor/limit are applied over the deck index; returned list is filtered to eligible owners.
    let mut eligible_players = Vec::new(env);
    let page = read_decks_page(env, cursor, limit);
    if page.is_empty() {
        return eligible_players;
    }
    for key in page.iter() {
        let deck = read_deck(env.clone(), key.owner.clone());
        if deck.token_ids.len() == 4 {
            eligible_players.push_back(deck.owner);
        }
    }
    eligible_players
}


pub fn get_eligible_players_with_shares(env: &Env) -> Vec<(Address, u32, u32, u32, Vec<(u32, u32, Category)>, u32)> {
    let players = get_eligible_players(env);
    let mut total_effective_power: u32 = 0;
    let mut player_data = Vec::new(env);

    // First pass: calculate effective powers and collect all player data with card details
    // No need for duplicate deck validation - get_eligible_players already filters complete decks
    for player in players.iter() {
        let deck = read_deck(env.clone(), player.clone());
        let user = read_user(env, player.clone());
        let effective_power = calculate_effective_power(deck.total_power, deck.bonus);
        total_effective_power += effective_power;

        // Collect card details: (token_id, power, category)
        let mut card_details = Vec::new(env);
        for token_id in deck.token_ids.iter() {
            let card = read_nft(env, player.clone(), token_id.clone());
            let metadata = read_metadata(env, token_id.0);

            let power = card.map(|c| c.power).unwrap_or(0);
            let category = metadata.category;

            card_details.push_back((token_id.0, power, category));
        }

        player_data.push_back((player, user.level, deck.total_power, effective_power, card_details, deck.bonus));
    }

    // Second pass: calculate share percentages based on total effective power
    let mut result = Vec::new(env);
    for (player, level, total_power, effective_power, card_details, bonus) in player_data.iter() {
        let share_percentage = if total_effective_power > 0 {
            (effective_power * 10000) / total_effective_power // Basis points
        } else {
            0
        };
        result.push_back((player, level, total_power, share_percentage, card_details, bonus));
    }
    result
}

pub fn calculate_player_shares(env: &Env, round: u32) {
    let players = get_eligible_players(env);
    let mut total_effective_power: u32 = 0;
    let mut player_powers = Vec::new(env);

    for player in players.iter() {
        let deck = read_deck(env.clone(), player.clone());
        if deck.token_ids.len() == 4 {
            let effective_power = calculate_effective_power(deck.total_power, deck.bonus);
            total_effective_power += effective_power;
            player_powers.push_back((player, effective_power, deck.bonus, deck.deck_categories));
        }
    }

    for (player, effective_power, deck_bonus, deck_categories) in player_powers.iter() {
        let share_percentage = if total_effective_power > 0 {
            (effective_power * 10000) / total_effective_power // Basis points
        } else {
            0
        };

        let reward = PlayerReward {
            share_percentage,
            effective_power,
            round_number: round,
            deck_bonus,
            deck_categories,
        };

        write_player_reward(env, round, &player, &reward);
        emit_share_calculated(env, &player, &reward);
    }

    if let Some(mut snapshot) = read_pot_snapshot(env, round) {
        snapshot.total_participants = player_powers.len();
        snapshot.total_effective_power = total_effective_power;
        write_pot_snapshot(env, round, &snapshot);
    }
}
