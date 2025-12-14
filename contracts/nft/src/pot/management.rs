use crate::actions::deck::read_deck;
use crate::event::*;
use crate::storage_types::{
    DataKey, Deck, DogstarBalance, PendingReward, PlayerReward, PotBalance,
    PotSnapshot, STORAGE_BUMP_LEDGERS, STORAGE_THRESHOLD_LEDGERS,
};
use crate::admin::read_config;
use crate::nft_info::{Action, Category, read_nft};
use crate::metadata::read_metadata;
use crate::user_info::read_user;
use soroban_sdk::{Address, Env, Vec};

const DAY_IN_LEDGERS: u32 = 17280;
const POT_BUMP_AMOUNT: u32 = 30 * DAY_IN_LEDGERS;
const POT_LIFETIME_THRESHOLD: u32 = POT_BUMP_AMOUNT - DAY_IN_LEDGERS;

/// Calculates effective power by applying the deck bonus to base power.
pub fn calculate_effective_power(base_power: u32, deck_bonus: u32) -> u32 {
    base_power * (100 + deck_bonus) / 100
}

// Pot Balance Management
pub fn read_pot_balance(env: &Env) -> PotBalance {
    let key = DataKey::PotBalance;
    if env.storage().persistent().has(&key) {
        env.storage().persistent().extend_ttl(
            &key,
            STORAGE_THRESHOLD_LEDGERS,
            STORAGE_BUMP_LEDGERS,
        );
    }
    env.storage()
        .persistent()
        .get(&key)
        .unwrap_or(PotBalance {
            accumulated_terry: 0,
            accumulated_power: 0,
            accumulated_xtar: 0,
            last_opening_round: 0,
            total_openings: 0,
            last_updated: env.ledger().timestamp(),
        })
}

pub fn write_pot_balance(env: &Env, balance: &PotBalance) {
    env.storage()
        .persistent()
        .set(&DataKey::PotBalance, balance);
    env.storage().persistent().extend_ttl(
        &DataKey::PotBalance,
        STORAGE_THRESHOLD_LEDGERS,
        STORAGE_BUMP_LEDGERS,
    );
}

pub fn read_dogstar_balance(env: &Env) -> DogstarBalance {
    let key = DataKey::DogstarBalance;
    if env.storage().persistent().has(&key) {
        env.storage().persistent().extend_ttl(
            &key,
            STORAGE_THRESHOLD_LEDGERS,
            STORAGE_BUMP_LEDGERS,
        );
    }
    env.storage()
        .persistent()
        .get(&key)
        .unwrap_or(DogstarBalance {
            terry: 0,
            power: 0,
            xtar: 0,
        })
}

pub fn write_dogstar_balance(env: &Env, balance: &DogstarBalance) {
    env.storage()
        .persistent()
        .set(&DataKey::DogstarBalance, balance);
    env.storage().persistent().extend_ttl(
        &DataKey::DogstarBalance,
        STORAGE_THRESHOLD_LEDGERS,
        STORAGE_BUMP_LEDGERS,
    );
}

// Internal helper to accumulate pot balances and dogstar fees without requiring admin auth.
// This is intended to be called from trusted internal flows like mint/burn.
pub fn accumulate_pot_internal(env: &Env, terry: i128, power: u32, xtar: i128, from: Option<Address>, action: Option<Action>) {
    let config = read_config(env);
    let mut pot_balance = read_pot_balance(env);

    // Calculate fees in basis points
    let fee_percentage = config.dogstar_fee_percentage;
    let terry_fee = (terry * fee_percentage as i128) / 10000;
    let power_fee = (power * fee_percentage) / 10000;
    let xtar_fee = (xtar * fee_percentage as i128) / 10000;

    // Accumulate in pot balance (minus dogstar fees)
    pot_balance.accumulated_terry += terry - terry_fee;
    pot_balance.accumulated_power += power - power_fee;
    pot_balance.accumulated_xtar += xtar - xtar_fee;
    pot_balance.last_updated = env.ledger().timestamp();
    write_pot_balance(env, &pot_balance);

    // Update legacy dogstar balance for backward-compat events/UI
    let mut dogstar_balance = read_dogstar_balance(env);
    dogstar_balance.terry += terry_fee;
    dogstar_balance.power += power_fee;
    dogstar_balance.xtar += xtar_fee;
    write_dogstar_balance(env, &dogstar_balance);

    if terry_fee > 0 || power_fee > 0 || xtar_fee > 0 {
        emit_dogstar_fee_accumulated(env, terry_fee, power_fee, xtar_fee, fee_percentage, from, action);
    }
}

// Snapshot Management
pub fn write_pot_snapshot(env: &Env, round: u32, snapshot: &PotSnapshot) {
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
    if env.storage().persistent().has(&key) {
        env.storage().persistent().extend_ttl(
            &key,
            STORAGE_THRESHOLD_LEDGERS,
            STORAGE_BUMP_LEDGERS,
        );
    }
    env.storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| Vec::new(env))
}

pub fn write_registered_tokens(env: &Env, tokens: &Vec<Address>) {
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
    if env.storage().persistent().has(&key) {
        env.storage().persistent().extend_ttl(
            &key,
            STORAGE_THRESHOLD_LEDGERS,
            STORAGE_BUMP_LEDGERS,
        );
    }
    env.storage()
        .persistent()
        .get(&key)
        .unwrap_or(0)
}

pub fn write_accumulated_by_token(env: &Env, token: &Address, amount: i128) {
    let key = DataKey::AccumulatedByToken(token.clone());
    env.storage().persistent().set(&key, &amount);
    env.storage().persistent().extend_ttl(
        &key,
        STORAGE_THRESHOLD_LEDGERS,
        STORAGE_BUMP_LEDGERS,
    );
}

pub fn read_user_generic_claimable(env: &Env, user: &Address, token: &Address) -> i128 {
    let key = DataKey::UserGenericClaimable(user.clone(), token.clone());
    if env.storage().persistent().has(&key) {
        env.storage().persistent().extend_ttl(
            &key,
            STORAGE_THRESHOLD_LEDGERS,
            STORAGE_BUMP_LEDGERS,
        );
    }
    env.storage()
        .persistent()
        .get(&key)
        .unwrap_or(0)
}

pub fn write_user_generic_claimable(env: &Env, user: &Address, token: &Address, amount: i128) {
    let key = DataKey::UserGenericClaimable(user.clone(), token.clone());
    env.storage().persistent().set(&key, &amount);
    env.storage().persistent().extend_ttl(
        &key,
        STORAGE_THRESHOLD_LEDGERS,
        STORAGE_BUMP_LEDGERS,
    );
}

pub fn read_pot_snapshot(env: &Env, round: u32) -> Option<PotSnapshot> {
    env.storage()
        .persistent()
        .get(&DataKey::OpeningSnapshot(round))
}

pub fn write_player_reward(env: &Env, round: u32, player: &Address, reward: &PlayerReward) {
    let key = DataKey::PlayerShare(round, player.clone());

    env.storage().persistent().set(&key, reward);
    env.storage().persistent().extend_ttl(
        &key,
        STORAGE_THRESHOLD_LEDGERS,
        STORAGE_BUMP_LEDGERS,
    );
}

pub fn read_player_reward(env: &Env, round: u32, player: &Address) -> Option<PlayerReward> {
    env.storage()
        .persistent()
        .get(&DataKey::PlayerShare(round, player.clone()))
}

pub fn write_pending_reward(env: &Env, round: u32, player: &Address, reward: &PendingReward) {
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
    if env.storage().persistent().has(&key) {
        env.storage().persistent().extend_ttl(
            &key,
            STORAGE_THRESHOLD_LEDGERS,
            STORAGE_BUMP_LEDGERS,
        );
    }
    env.storage()
        .persistent()
        .get(&key)
}

pub fn get_current_round(env: &Env) -> u32 {
    let key = DataKey::CurrentRound;
    if env.storage().persistent().has(&key) {
        env.storage().persistent().extend_ttl(
            &key,
            STORAGE_THRESHOLD_LEDGERS,
            STORAGE_BUMP_LEDGERS,
        );
    }
    env.storage()
        .persistent()
        .get(&key)
        .unwrap_or(0)
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

pub fn get_all_rounds(env: &Env) -> Vec<u32> {
    let key = DataKey::AllRounds;
    if env.storage().persistent().has(&key) {
        env.storage().persistent().extend_ttl(
            &key,
            STORAGE_THRESHOLD_LEDGERS,
            STORAGE_BUMP_LEDGERS,
        );
    }
    env.storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| Vec::new(env))
}

pub fn add_round(env: &Env, round: u32) {
    let mut rounds = get_all_rounds(env);
    rounds.push_back(round);

    env.storage().persistent().set(&DataKey::AllRounds, &rounds);
    env.storage().persistent().extend_ttl(
        &DataKey::AllRounds,
        STORAGE_THRESHOLD_LEDGERS,
        STORAGE_BUMP_LEDGERS,
    );
}

pub fn get_eligible_players(env: &Env) -> Vec<Address> {
    let mut eligible_players = Vec::new(env);
    let key = DataKey::Decks;
    // Extend TTL for the global decks list to ensure it stays alive during pot distribution
    if env.storage().persistent().has(&key) {
        env.storage().persistent().extend_ttl(
            &key,
            STORAGE_THRESHOLD_LEDGERS,
            STORAGE_BUMP_LEDGERS,
        );
    }
    let decks = env
        .storage()
        .persistent()
        .get::<DataKey, Vec<Deck>>(&key)
        .unwrap_or(Vec::new(env));

    for deck in decks.iter() {
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
