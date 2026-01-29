use crate::error::NFTError;
use crate::event::{emit_deck_completed, emit_deck_place, emit_deck_remove, emit_deck_replace};
use crate::pot::management::{get_current_round, read_pot_in_progress_round, read_pot_status};
use crate::{admin::read_config, user_info::mint_terry, *};
use admin::{
    is_pages_only, read_total_effective_deck_power, update_balance,
    update_total_effective_deck_power, write_total_effective_deck_power,
};
use metadata::read_metadata;
use nft_info::{read_nft, update_nft, Action, Category};
use soroban_sdk::{log, panic_with_error, vec, Address, Env, Vec};
use storage_types::{
    DataKey, Deck, DeckKey, PagedListKind, PagedPosKind, PotStatus, TokenId, PAGE_SIZE_DECKS,
    STORAGE_BUMP_LEDGERS, STORAGE_THRESHOLD_LEDGERS,
};
fn deck_pos_key(owner: Address) -> DataKey {
    DataKey::Pos(PagedPosKind::Decks, owner, Category::Leader, TokenId(0))
}
use user_info::read_user;

fn write_deck(env: Env, user: Address, deck: Deck) {
    let owner = read_user(&env, user).owner;

    let key = DataKey::Deck(owner.clone());
    env.storage().persistent().set(&key, &deck);
    #[cfg(not(test))]
    {
        env.storage().persistent().extend_ttl(
            &key,
            STORAGE_THRESHOLD_LEDGERS,
            STORAGE_BUMP_LEDGERS,
        );
    }

    if !is_pages_only(&env) {
    let key = DataKey::Decks;
    let mut decks = read_decks(env.clone());
    if let Some(pos) = decks.iter().position(|deck| deck.owner == owner) {
        decks.set(pos.try_into().unwrap(), deck.clone())
    } else {
        decks.push_back(deck.clone());
    }

    env.storage().persistent().set(&key, &decks);
    env.storage()
        .persistent()
        .extend_ttl(&key, STORAGE_THRESHOLD_LEDGERS, STORAGE_BUMP_LEDGERS);
    }

    // New paged global index (IDs only)
    if deck.token_ids.len() > 0 {
        if !env.storage().persistent().has(&deck_pos_key(owner.clone())) {
            add_deck_page_index(
                &env,
                DeckKey {
                    owner,
                },
            );
        }
    } else if env.storage().persistent().has(&deck_pos_key(owner.clone())) {
        remove_deck_page_index(&env, owner);
    }
}

pub fn read_decks(env: Env) -> Vec<Deck> {
    let total_effective = read_total_effective_deck_power(&env);
    if is_pages_only(&env) {
        let count = env
            .storage()
            .persistent()
            .get::<_, u32>(&DataKey::PagedCount(PagedListKind::Decks))
            .unwrap_or(0);
        let mut out = vec![&env.clone()];
        let mut idx = 0;
        while idx < count {
            if let Some(key) = read_deck_key_at(&env, idx) {
                let deck_key = DataKey::Deck(key.owner.clone());
                if let Some(mut deck) = env.storage().persistent().get::<_, Deck>(&deck_key) {
                    deck.haw_ai_percentage = if total_effective > 0 {
                        (crate::pot::management::calculate_effective_power(
                            deck.total_power,
                            deck.bonus,
                        ) as u64
                            * 10_000)
                            .checked_div(total_effective as u64)
                            .unwrap_or(0) as u32
                    } else {
                        0
                    };
                    out.push_back(deck);
                }
            }
            idx += 1;
        }
        return out;
    }
    let key = DataKey::Decks;
    if let Some(decks) = env.storage().persistent().get::<_, Vec<Deck>>(&key) {
        env.storage().persistent().extend_ttl(
            &key,
            STORAGE_THRESHOLD_LEDGERS,
            STORAGE_BUMP_LEDGERS,
        );
        let mut out = vec![&env.clone()];
        for mut deck in decks {
            deck.haw_ai_percentage = if total_effective > 0 {
                (crate::pot::management::calculate_effective_power(deck.total_power, deck.bonus)
                    as u64
                    * 10_000)
                    .checked_div(total_effective as u64)
                    .unwrap_or(0) as u32
            } else {
                0
            };
            out.push_back(deck);
        }
        out
    } else {
        vec![&env.clone()]
    }
}

pub fn read_decks_count(env: &Env) -> u32 {
    let count = env
        .storage()
        .persistent()
        .get::<_, u32>(&DataKey::PagedCount(PagedListKind::Decks))
        .unwrap_or(0);
    if count == 0 && !is_pages_only(env) {
        let legacy = read_decks(env.clone());
        if !legacy.is_empty() {
            return legacy.len();
        }
    }
    count
}

pub fn read_decks_page(env: &Env, cursor: u32, limit: u32) -> Vec<DeckKey> {
    if limit == 0 {
        return Vec::new(env);
    }
    let count = read_decks_count(env);
    if !is_pages_only(env)
        && !env
            .storage()
            .persistent()
            .has(&DataKey::PagedCount(PagedListKind::Decks))
    {
        let legacy = read_decks(env.clone());
        let total = legacy.len();
        if cursor >= total {
            return Vec::new(env);
        }
        let end = (cursor.saturating_add(limit)).min(total);
        let mut out: Vec<DeckKey> = Vec::new(env);
        let mut idx = cursor;
        while idx < end {
            let deck = legacy.get(idx).unwrap();
            out.push_back(DeckKey {
                owner: deck.owner.clone(),
            });
            idx += 1;
        }
        return out;
    }
    if cursor >= count {
        return Vec::new(env);
    }
    let end = (cursor.saturating_add(limit)).min(count);
    let mut out: Vec<DeckKey> = Vec::new(env);
    let mut idx = cursor;
    while idx < end {
        if let Some(key) = read_deck_key_at(env, idx) {
            out.push_back(key);
        }
        idx += 1;
    }
    out
}

fn read_deck_key_at(env: &Env, idx: u32) -> Option<DeckKey> {
    let page = idx / PAGE_SIZE_DECKS;
    let off = idx % PAGE_SIZE_DECKS;
    let key = DataKey::PagedList(PagedListKind::Decks, page);
    let page_vec: Vec<DeckKey> = env.storage().persistent().get(&key).unwrap_or(Vec::new(env));
    if off < page_vec.len() {
        Some(page_vec.get(off).unwrap())
    } else {
        None
    }
}

fn write_decks_count(env: &Env, count: u32) {
    env.storage()
        .persistent()
        .set(&DataKey::PagedCount(PagedListKind::Decks), &count);
    env.storage().persistent().extend_ttl(
        &DataKey::PagedCount(PagedListKind::Decks),
        STORAGE_THRESHOLD_LEDGERS,
        STORAGE_BUMP_LEDGERS,
    );
}

fn add_deck_page_index(env: &Env, key: DeckKey) {
    let count = env
        .storage()
        .persistent()
        .get::<_, u32>(&DataKey::PagedCount(PagedListKind::Decks))
        .unwrap_or(0);
    let page = count / PAGE_SIZE_DECKS;
    let off = count % PAGE_SIZE_DECKS;
    let page_key = DataKey::PagedList(PagedListKind::Decks, page);
    let mut page_vec: Vec<DeckKey> = env.storage().persistent().get(&page_key).unwrap_or(Vec::new(env));
    if off == page_vec.len() {
        page_vec.push_back(key.clone());
    } else if off < page_vec.len() {
        page_vec.set(off, key.clone());
    } else {
        panic!("PagedList(Decks) corrupted: offset beyond page length");
    }
    env.storage().persistent().set(&page_key, &page_vec);
    env.storage().persistent().extend_ttl(
        &page_key,
        STORAGE_THRESHOLD_LEDGERS,
        STORAGE_BUMP_LEDGERS,
    );
    env.storage()
        .persistent()
        .set(&deck_pos_key(key.owner.clone()), &count);
    env.storage().persistent().extend_ttl(
        &deck_pos_key(key.owner),
        STORAGE_THRESHOLD_LEDGERS,
        STORAGE_BUMP_LEDGERS,
    );
    write_decks_count(env, count + 1);
}

fn remove_deck_page_index(env: &Env, owner: Address) {
    let pos_key = deck_pos_key(owner.clone());
    if !env.storage().persistent().has(&pos_key) {
        return;
    }
    let count = read_decks_count(env);
    if count == 0 {
        env.storage().persistent().remove(&pos_key);
        return;
    }
    let pos: u32 = env.storage().persistent().get(&pos_key).unwrap();
    let last = count.saturating_sub(1);
    if pos > last {
        env.storage().persistent().remove(&pos_key);
        return;
    }

    let last_key = read_deck_key_at(env, last);
    if pos == last {
        // Remove last entry in its page
        let last_page = last / PAGE_SIZE_DECKS;
        let last_off = last % PAGE_SIZE_DECKS;
        let page_key = DataKey::PagedList(PagedListKind::Decks, last_page);
        let mut page_vec: Vec<DeckKey> =
            env.storage().persistent().get(&page_key).unwrap_or(Vec::new(env));
        if last_off < page_vec.len() {
            page_vec.remove(last_off.try_into().unwrap());
            env.storage().persistent().set(&page_key, &page_vec);
        }
        env.storage().persistent().remove(&pos_key);
        write_decks_count(env, last);
        return;
    }

    if let Some(last_key) = last_key {
        let pos_page = pos / PAGE_SIZE_DECKS;
        let pos_off = pos % PAGE_SIZE_DECKS;
        let pos_page_key = DataKey::PagedList(PagedListKind::Decks, pos_page);
        let mut pos_page_vec: Vec<DeckKey> =
            env.storage().persistent().get(&pos_page_key).unwrap_or(Vec::new(env));
        if pos_off < pos_page_vec.len() {
            pos_page_vec.set(pos_off, last_key.clone());
            env.storage().persistent().set(&pos_page_key, &pos_page_vec);
            env.storage()
                .persistent()
                .set(&deck_pos_key(last_key.owner.clone()), &pos);
        }

        let last_page = last / PAGE_SIZE_DECKS;
        let last_off = last % PAGE_SIZE_DECKS;
        let last_page_key = DataKey::PagedList(PagedListKind::Decks, last_page);
        let mut last_page_vec: Vec<DeckKey> =
            env.storage().persistent().get(&last_page_key).unwrap_or(Vec::new(env));
        if last_off < last_page_vec.len() {
            last_page_vec.remove(last_off.try_into().unwrap());
            env.storage().persistent().set(&last_page_key, &last_page_vec);
        }

        env.storage().persistent().remove(&pos_key);
        write_decks_count(env, last);
    }
}

pub fn migrate_decks_to_pages(env: &Env, start: u32, limit: u32) -> u32 {
    let decks = read_decks(env.clone());
    let total = decks.len();
    if start >= total {
        return total;
    }
    let end = (start.saturating_add(limit)).min(total);
    let mut idx = start;
    while idx < end {
        let deck = decks.get(idx).unwrap();
        if deck.token_ids.len() == 0 {
            idx += 1;
            continue;
        }
        let pos_key = deck_pos_key(deck.owner.clone());
        if !env.storage().persistent().has(&pos_key) {
            add_deck_page_index(env, DeckKey { owner: deck.owner.clone() });
        }
        idx += 1;
    }
    end
}

pub fn rebuild_total_effective_deck_power(env: &Env, start: u32, limit: u32) -> u32 {
    let paged_count = read_decks_count(env);
    if start == 0 {
        write_total_effective_deck_power(env, 0);
    }

    if paged_count == 0 && !is_pages_only(env) && env.storage().persistent().has(&DataKey::Decks)
    {
        let decks = read_decks(env.clone());
        let total = decks.len();
        if start >= total {
            return total;
        }
        let end = (start.saturating_add(limit)).min(total);
        let mut idx = start;
        while idx < end {
            let deck = decks.get(idx).unwrap();
            if deck.token_ids.len() == 4 {
                let effective =
                    crate::pot::management::calculate_effective_power(deck.total_power, deck.bonus);
                update_total_effective_deck_power(env, |_, total| {
                    *total = total.saturating_add(effective);
                });
            }
            idx += 1;
        }
        return end;
    }

    if paged_count == 0 {
        return 0;
    }
    if start >= paged_count {
        return paged_count;
    }
    let end = (start.saturating_add(limit)).min(paged_count);
    let mut idx = start;
    while idx < end {
        if let Some(key) = read_deck_key_at(env, idx) {
            let deck = read_deck(env.clone(), key.owner);
            if deck.token_ids.len() == 4 {
                let effective = crate::pot::management::calculate_effective_power(
                    deck.total_power,
                    deck.bonus,
                );
                update_total_effective_deck_power(env, |_, total| {
                    *total = total.saturating_add(effective);
                });
            }
        }
        idx += 1;
    }
    end
}

// fn remove_deck(env: Env, user: Address) {
//     let owner = read_user(&env, user).owner;
//     let key = DataKey::Deck(owner.clone());
//     env.storage().persistent().remove(&key);
//     if env.storage().persistent().has(&key) {
//         env.storage().persistent().extend_ttl(
//             &key,
//             BALANCE_LIFETIME_THRESHOLD,
//             BALANCE_BUMP_AMOUNT,
//         );
//     }

//     let key = DataKey::Decks;
//     let mut decks = read_decks(env.clone());
//     if let Some(pos) = decks.iter().position(|deck| deck.owner == owner) {
//         decks.remove(pos.try_into().unwrap());
//     }

//     env.storage().persistent().set(&key, &decks);

//     env.storage()
//         .persistent()
//         .extend_ttl(&key, BALANCE_LIFETIME_THRESHOLD, BALANCE_BUMP_AMOUNT);
// }

pub fn read_deck(env: Env, user: Address) -> Deck {
    let owner = read_user(&env, user).owner;

    let key = DataKey::Deck(owner.clone());
    let new_deck = Deck {
        owner: owner.clone(),
        haw_ai_percentage: 0,
        total_power: 0,
        bonus: 0,
        deck_categories: 0,
        token_ids: Vec::new(&env),
    };
    
    let mut deck = if let Some(deck) = env.storage().persistent().get(&key) {
        #[cfg(not(test))]
        {
            env.storage().persistent().extend_ttl(
                &key,
                STORAGE_THRESHOLD_LEDGERS,
                STORAGE_BUMP_LEDGERS,
            );
        }
        deck
    } else {
        // Persist deck default so the key always exists (prevents MissingValue on new users)
        env.storage().persistent().set(&key, &new_deck);
        #[cfg(not(test))]
        {
            env.storage().persistent().extend_ttl(
                &key,
                STORAGE_THRESHOLD_LEDGERS,
                STORAGE_BUMP_LEDGERS,
            );
        }
        new_deck
    };
    let total_effective = read_total_effective_deck_power(&env);
    deck.haw_ai_percentage = if total_effective > 0 {
        (crate::pot::management::calculate_effective_power(deck.total_power, deck.bonus) as u64
            * 10_000)
            .checked_div(total_effective as u64)
            .unwrap_or(0) as u32
    } else {
        0
    };
    deck
}

pub fn recompute_deck_after_power_change(env: &Env, owner: Address) {
    let mut deck = read_deck(env.clone(), owner.clone());
    let old_deck = deck.clone();
    recompute_deck_totals(env, owner.clone(), &old_deck, &mut deck);
    write_deck(env.clone(), owner, deck);
}

fn contains_token(token_ids: &Vec<TokenId>, token_id: &TokenId) -> bool {
    token_ids.iter().position(|x| x == token_id.clone()).is_some()
}

fn assert_no_duplicates(token_ids: &Vec<TokenId>) {
    let len = token_ids.len();
    for i in 0..len {
        let a = token_ids.get(i).unwrap();
        for j in (i + 1)..len {
            let b = token_ids.get(j).unwrap();
            assert!(a != b, "Duplicate token in deck");
        }
    }
}

fn recompute_deck_totals(env: &Env, owner: Address, old_deck: &Deck, deck: &mut Deck) {
    let old_complete = old_deck.token_ids.len() == 4;
    let old_total = old_deck.total_power;
    if old_complete && old_total > 0 {
        update_balance(env, |_, balance| {
            balance.total_deck_power = balance.total_deck_power.saturating_sub(old_total);
        });
        let old_effective =
            crate::pot::management::calculate_effective_power(old_total, old_deck.bonus);
        update_total_effective_deck_power(env, |_, total| {
            *total = total.saturating_sub(old_effective);
        });
    }

    if deck.token_ids.len() == 4 {
        let mut unique_categories = vec![&env.clone()];
        let mut total_power = 0;
        for id in deck.token_ids.iter() {
            let nft = read_nft(env, owner.clone(), id.clone()).unwrap();
            let metadata = read_metadata(env, id.clone().0);
            let category = metadata.category;
            total_power += nft.power;
            if !unique_categories.contains(&category) {
                unique_categories.push_back(category.clone());
            }
        }
        let deck_categories = unique_categories.len() as u32;
        let bonus = match deck_categories {
            2 => 5,
            3 => 10,
            4 => 25,
            _ => 0,
        };
        deck.bonus = bonus;
        deck.total_power = total_power;
        deck.deck_categories = deck_categories;

        update_balance(env, |_, balance| {
            balance.total_deck_power += total_power;
        });
        let new_effective =
            crate::pot::management::calculate_effective_power(total_power, bonus);
        update_total_effective_deck_power(env, |_, total| {
            *total = total.saturating_add(new_effective);
        });

        emit_deck_completed(env, &owner);
    } else {
        deck.total_power = 0;
        deck.bonus = 0;
        deck.deck_categories = 0;
    }
}

fn apply_deck_update(env: Env, owner: Address, mut deck: Deck, new_token_ids: Vec<TokenId>) -> Deck {
    assert!(new_token_ids.len() <= 4, "Decks cannot exceed 4 cards!");
    assert_no_duplicates(&new_token_ids);

    let old_deck = deck.clone();

    // Validate tokens and locks
    for id in new_token_ids.iter() {
        let nft = read_nft(&env, owner.clone(), id.clone()).unwrap();
        if contains_token(&old_deck.token_ids, &id) {
            assert!(nft.locked_by_action == Action::Deck, "Not locked by Deck");
        } else {
            assert!(nft.locked_by_action == Action::None, "Locked by other action");
        }
    }

    // Unlock removed cards
    for id in old_deck.token_ids.iter() {
        if !contains_token(&new_token_ids, &id) {
            let nft = read_nft(&env, owner.clone(), id.clone()).unwrap();
            assert!(nft.locked_by_action == Action::Deck, "Not locked by Deck");
            update_nft(&env, owner.clone(), id.clone(), |_, card| {
                card.locked_by_action = Action::None;
            });
        }
    }

    // Lock newly added cards
    for id in new_token_ids.iter() {
        if !contains_token(&old_deck.token_ids, &id) {
            update_nft(&env, owner.clone(), id.clone(), |_, card| {
                card.locked_by_action = Action::Deck;
            });
        }
    }

    deck.token_ids = new_token_ids;
    recompute_deck_totals(&env, owner.clone(), &old_deck, &mut deck);
    write_deck(env.clone(), owner, deck.clone());
    deck
}

pub fn place(env: Env, user: Address, token_id: TokenId) {
    let round_to_check = read_pot_in_progress_round(&env)
        .unwrap_or_else(|| get_current_round(&env).saturating_add(1));
    let status = read_pot_status(&env, round_to_check);
    if status != PotStatus::Init && status != PotStatus::Finalized {
        panic_with_error!(&env, NFTError::PotInProgress);
    }
    let owner = read_user(&env, user.clone()).owner;
    let deck = read_deck(env.clone(), owner.clone());
    assert!(deck.token_ids.len() < 4, "Decks are exceed!");

    let mut new_token_ids = deck.token_ids.clone();
    assert!(
        !contains_token(&new_token_ids, &token_id),
        "Token already in deck"
    );
    new_token_ids.push_back(token_id.clone());

    apply_deck_update(env.clone(), owner.clone(), deck, new_token_ids);

    // Emit deck place event
    emit_deck_place(&env, &owner);

    let config = read_config(&env);
    mint_terry(&env, owner.clone(), config.terry_per_deck);

    update_balance(&env, |_, balance| {
        balance.haw_ai_terry += config.terry_per_deck * config.haw_ai_percentage as i128 / 100;
    });
}

pub fn replace(env: Env, user: Address, prev_token_id: TokenId, token_id: TokenId) {
    let round_to_check = read_pot_in_progress_round(&env)
        .unwrap_or_else(|| get_current_round(&env).saturating_add(1));
    let status = read_pot_status(&env, round_to_check);
    if status != PotStatus::Init && status != PotStatus::Finalized {
        panic_with_error!(&env, NFTError::PotInProgress);
    }
    let owner = read_user(&env, user.clone()).owner;
    let deck = read_deck(env.clone(), owner.clone());
    assert!(
        contains_token(&deck.token_ids, &prev_token_id),
        "Token not in deck"
    );
    assert!(
        !contains_token(&deck.token_ids, &token_id),
        "Token already in deck"
    );

    let mut new_token_ids = deck.token_ids.clone();
    if let Some(index) = new_token_ids
        .iter()
        .position(|x| x == prev_token_id.clone())
    {
        new_token_ids.set(index.try_into().unwrap(), token_id.clone());
    }

    apply_deck_update(env.clone(), owner.clone(), deck, new_token_ids);

    // Emit deck replace event
    emit_deck_replace(&env, &owner);
}

#[allow(dead_code)]
pub fn update_deck(env: Env, user: Address, token_ids: Vec<TokenId>) {
    let round_to_check = read_pot_in_progress_round(&env)
        .unwrap_or_else(|| get_current_round(&env).saturating_add(1));
    let status = read_pot_status(&env, round_to_check);
    if status != PotStatus::Init && status != PotStatus::Finalized {
        panic_with_error!(&env, NFTError::PotInProgress);
    }
    let owner = read_user(&env, user.clone()).owner;
    let deck = read_deck(env.clone(), owner.clone());
    apply_deck_update(env.clone(), owner, deck, token_ids);
}

pub fn remove_place(env: Env, user: Address, token_id: TokenId) {
    let round_to_check = read_pot_in_progress_round(&env)
        .unwrap_or_else(|| get_current_round(&env).saturating_add(1));
    let status = read_pot_status(&env, round_to_check);
    if status != PotStatus::Init && status != PotStatus::Finalized {
        panic_with_error!(&env, NFTError::PotInProgress);
    }
    let owner = read_user(&env, user.clone()).owner;
    let deck = read_deck(env.clone(), owner.clone());
    assert!(deck.token_ids.len() > 0, "Decks are null!");
    assert!(
        contains_token(&deck.token_ids, &token_id),
        "Token not in deck"
    );

    log!(&env, "deck token id length {}", deck.token_ids.len());

    let mut new_token_ids = deck.token_ids.clone();
    if let Some(index) = new_token_ids.iter().position(|x| x == token_id.clone()) {
        new_token_ids.remove(index.try_into().unwrap());
    }

    apply_deck_update(env.clone(), owner.clone(), deck, new_token_ids);

    // Emit deck remove event
    emit_deck_remove(&env, &owner);

    let config = read_config(&env);
    mint_terry(&env, owner.clone(), config.terry_per_deck);

    update_balance(&env, |_, balance| {
        balance.haw_ai_terry += config.terry_per_deck * config.haw_ai_percentage as i128 / 100;
    });
}

// Deprecated: use apply_deck_update() which keeps total_deck_power consistent.
// pub fn remove_all_place(env: Env, user: Address) {
//     let deck = read_deck(env.clone(), user.clone());

//     // release all action, write nft
//     for i in 0..4 {
//         let token_id = deck.token_ids.get(i).unwrap();

//         let mut nft = read_nft(&env, user.clone(), token_id.clone()).unwrap();
//         nft.locked_by_action = Action::None;

//         write_nft(&env.clone(), user.clone(), token_id.clone(), nft);
//     }

//     // update balance
//     let mut balance = read_balance(&env);
//     balance.total_deck_power -= deck.total_power;
//     write_balance(&env, &balance);

//     // remove deck
//     remove_deck(env.clone(), user.clone());

//     // update haw ai percentage
//

#[allow(dead_code)]
pub fn update_haw_ai_percentages(_env: Env) {
    // No-op: haw_ai_percentage is computed on read to avoid global scans.
}
