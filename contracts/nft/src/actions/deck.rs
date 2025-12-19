use crate::event::{emit_deck_completed, emit_deck_place, emit_deck_remove, emit_deck_replace};
use crate::{admin::read_config, user_info::mint_terry, *};
use admin::{read_balance, update_balance};
use metadata::read_metadata;
use nft_info::{read_nft, update_nft, write_nft, Action};
use soroban_sdk::{log, vec, Address, Env, Vec};
use storage_types::{DataKey, Deck, TokenId, STORAGE_BUMP_LEDGERS, STORAGE_THRESHOLD_LEDGERS};
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

    let key = DataKey::Decks;
    let mut decks = read_decks(env.clone());
    if let Some(pos) = decks.iter().position(|deck| deck.owner == owner) {
        decks.set(pos.try_into().unwrap(), deck)
    } else {
        decks.push_back(deck);
    }

    env.storage().persistent().set(&key, &decks);
    env.storage()
        .persistent()
        .extend_ttl(&key, STORAGE_THRESHOLD_LEDGERS, STORAGE_BUMP_LEDGERS);
}

pub fn read_decks(env: Env) -> Vec<Deck> {
    let key = DataKey::Decks;
    if let Some(decks) = env.storage().persistent().get(&key) {
        env.storage().persistent().extend_ttl(
            &key,
            STORAGE_THRESHOLD_LEDGERS,
            STORAGE_BUMP_LEDGERS,
        );
        decks
    } else {
        vec![&env.clone()]
    }
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
    
    if let Some(deck) = env.storage().persistent().get(&key) {
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
    }
}

pub fn place(env: Env, user: Address, token_id: TokenId) {
    let mut deck = read_deck(env.clone(), user.clone());

    assert!(deck.token_ids.len() < 4, "Decks are exceed!");

    let nft_read = read_nft(&env, user.clone(), token_id.clone()).unwrap();

    assert!(
        nft_read.locked_by_action == Action::None,
        "Locked by other action"
    );

    update_nft(&env, user.clone(), token_id.clone(), |_, card| {
        card.locked_by_action = Action::Deck;
    });

    deck.token_ids.push_back(token_id.clone());

    let _deck_size = deck.token_ids.len();

    if deck.token_ids.len() == 4 {
        calculate_deck_balance(env.clone(), user.clone(), &mut deck);
    }
    write_deck(env.clone(), user.clone(), deck);

    // Emit deck place event
    emit_deck_place(&env, &user);

    let config = read_config(&env);
    mint_terry(&env, user.clone(), config.terry_per_deck);

    update_balance(&env, |_, balance| {
        balance.haw_ai_terry += config.terry_per_deck * config.haw_ai_percentage as i128 / 100;
    });
}

pub fn replace(env: Env, user: Address, prev_token_id: TokenId, token_id: TokenId) {
    let mut deck = read_deck(env.clone(), user.clone());

    let prev_nft_read = read_nft(&env, user.clone(), prev_token_id.clone()).unwrap();

    assert!(
        prev_nft_read.locked_by_action == Action::Deck,
        "Not locked by Deck"
    );

    let nft_read = read_nft(&env, user.clone(), token_id.clone()).unwrap();
    assert!(
        nft_read.locked_by_action == Action::None,
        "Locked by other action"
    );
    if let Some(index) = deck
        .token_ids
        .iter()
        .position(|x| x == prev_token_id.clone())
    {
        deck.token_ids.remove(index.try_into().unwrap());
        deck.token_ids
            .insert(index.try_into().unwrap(), token_id.clone());
    }
    
    update_nft(&env, user.clone(), prev_token_id.clone(), |_, card| {
        card.locked_by_action = Action::None;
    });

    if deck.token_ids.len() == 4 {
        calculate_deck_balance(env.clone(), user.clone(), &mut deck);
    }
    write_deck(env.clone(), user.clone(), deck);

    // Emit deck replace event
    emit_deck_replace(&env, &user);

    update_nft(&env, user.clone(), token_id.clone(), |_, card| {
        card.locked_by_action = Action::Deck;
    });
}

pub fn update_deck(env: Env, user: Address, token_ids: Vec<TokenId>) {
    let mut deck = read_deck(env.clone(), user.clone());

    assert!(token_ids.len() <= 4, "Decks cannot exceed 4 cards!");

    deck.token_ids = token_ids;

    if deck.token_ids.len() == 4 {
        calculate_deck_balance(env.clone(), user.clone(), &mut deck);
    }
    write_deck(env.clone(), user.clone(), deck);
}

pub fn remove_place(env: Env, user: Address, token_id: TokenId) {
    let mut deck = read_deck(env.clone(), user.clone());

    assert!(deck.token_ids.len() > 0, "Decks are null!");

    let nft_read = read_nft(&env, user.clone(), token_id.clone()).unwrap();

    log!(&env, "deck token id length {}", deck.token_ids.len());

    if let Some(index) = deck.token_ids.iter().position(|x| x == token_id.clone()) {
        deck.token_ids.remove(index.try_into().unwrap());
    }

    assert!(nft_read.locked_by_action == Action::Deck, "Not locked by Deck");
    
    update_nft(&env, user.clone(), token_id.clone(), |_, card| {
        card.locked_by_action = Action::None;
    });

    update_balance(&env, |_, balance| {
        if balance.total_deck_power >= deck.total_power {
            balance.total_deck_power -= deck.total_power.clone();
        } else {
            balance.total_deck_power = 0;
        }
    });

    deck.total_power = 0;
    deck.bonus = 0;
    deck.deck_categories = 0;

    let _deck_size = deck.token_ids.len();

    write_deck(env.clone(), user.clone(), deck);
    update_haw_ai_percentages(env.clone());

    // Emit deck remove event
    emit_deck_remove(&env, &user);

    let config = read_config(&env);
    mint_terry(&env, user.clone(), config.terry_per_deck);

    update_balance(&env, |_, balance| {
        balance.haw_ai_terry += config.terry_per_deck * config.haw_ai_percentage as i128 / 100;
    });
}

pub fn calculate_deck_balance(env: Env, player_address: Address, deck: &mut Deck) {
    let mut unique_categories = vec![&env.clone()];
    let mut total_power = 0;
    if deck.token_ids.len() == 4 {
        for id in deck.token_ids.iter() {
            let _nft = read_nft(&env, player_address.clone(), id.clone()).unwrap();
            let metadata = read_metadata(&env, id.clone().0);
            let category = metadata.category;

            total_power += _nft.power;

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

        update_balance(&env, |_, balance| {
             balance.total_deck_power += total_power;
        });

        deck.bonus = bonus;
        deck.total_power = total_power;
        deck.deck_categories = deck_categories;

        // Emit deck completed event
        emit_deck_completed(&env, &player_address);

        update_haw_ai_percentages(env.clone());
    }
}
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

pub fn update_haw_ai_percentages(env: Env) {
    let decks = read_decks(env.clone());
    let balance = read_balance(&env);

    for mut deck in decks {
        let haw_ai_percentage = if balance.total_deck_power > 0 {
            deck.total_power * (100 + deck.bonus) / balance.total_deck_power
        } else {
            0
        };

        deck.haw_ai_percentage = haw_ai_percentage;

        write_deck(env.clone(), deck.owner.clone(), deck);
    }
}
