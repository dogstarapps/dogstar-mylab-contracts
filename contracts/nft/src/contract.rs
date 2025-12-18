//! This contract demonstrates a sample implementation of the Soroban token
//! interface.

use crate::actions::{read_deck, deck::read_decks};
use crate::actions::{
    burn, deck, fight, lending,
    lending::{Borrowing, Lending},
    stake, SidePosition,
};
    use crate::admin::{
    add_level, has_administrator, read_administrator, read_balance, read_config, read_state,
    update_level, write_administrator, write_balance, write_config, read_contract_vault,
    write_contract_vault, read_user_claimable_balance, write_user_claimable_balance,
    read_dogstar_claimable, write_dogstar_claimable, update_balance, update_contract_vault,
    update_user_claimable_balance, update_config,
};
use crate::error::NFTError;
use crate::event::*;
use crate::metadata::{read_metadata, write_metadata, CardMetadata};
use crate::nft_info::{exists, read_nft, remove_nft, update_nft, write_nft, Action, Card, Category, Currency};
use crate::pot::management::*;
use crate::storage_types::*;
    use crate::user_info::{
    add_card_to_owner, burn_terry, get_user_level, mint_terry, read_owner_card, read_user,
    write_user, update_owner_cards, update_user,
};

use soroban_sdk::{
    contract, contractimpl, token, Address, BytesN, Env, Symbol};
use soroban_sdk::{vec, String, Vec};
use soroban_token_sdk::TokenUtils;

fn bump_instance(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(STORAGE_THRESHOLD_LEDGERS, STORAGE_BUMP_LEDGERS);
}

#[contract]
pub struct NFT;

#[contractimpl]
impl NFT {
    // Helper to move an NFT between owners while keeping owner indexes consistent.
    // No API or logic change: encapsulates the existing pattern remove_nft + write_nft + owner index updates.
    fn move_nft(env: &Env, from: Address, to: Address, token_id: TokenId, card: Card) {
        update_owner_cards(env, from.clone(), |_, from_cards| {
            if let Some(pos) = from_cards.iter().position(|x| x == token_id.clone()) {
                from_cards.remove(pos.try_into().unwrap());
            }
        });

        remove_nft(env, from, token_id.clone());
        write_nft(env, to.clone(), token_id.clone(), card);

        update_owner_cards(env, to, |_, to_cards| {
            to_cards.push_back(token_id);
        });
    }

    // --- Multi‑asset pot admin: token registry ---
    pub fn register_token(e: Env, token: Address) {
        let admin = read_administrator(&e);
        admin.require_auth();
        bump_instance(&e);
        update_registered_tokens(&e, |_, tokens| {
            let mut exists = false;
            for t in tokens.iter() {
                if t == token {
                    exists = true;
                    break;
                }
            }
            if !exists {
                tokens.push_back(token.clone());
            }
        });
    }

    pub fn unregister_token(e: Env, token: Address) {
        let admin = read_administrator(&e);
        admin.require_auth();
        bump_instance(&e);
        update_registered_tokens(&e, |env, tokens| {
            let mut filtered = Vec::new(env);
            for t in tokens.iter() {
                if t != token {
                    filtered.push_back(t);
                }
            }
            *tokens = filtered;
        });
    }

    // Accumulate arbitrary SAC token into pot (net of Dogstar fees will be handled off‑chain for now)
    pub fn accumulate_pot_token(env: Env, token: Address, amount: i128) {
        let admin = read_administrator(&env);
        admin.require_auth();
        bump_instance(&env);
        assert!(amount >= 0, "Negative contributions not allowed");
        // Ensure registered
        let tokens = read_registered_tokens(&env);
        let mut ok = false; for t in tokens.iter() { if t == token { ok = true; break; } }
        assert!(ok, "Token not registered");

        // Transfer from admin to contract (assumes admin already holds the SAC token)
        let client = token::Client::new(&env, &token);
        client.transfer(&admin, &env.current_contract_address(), &amount);

        // Apply Dogstar fee to all assets
        let cfg = read_config(&env);
        let fee_percentage = cfg.dogstar_fee_percentage as i128; // basis points
        let fee = (amount * fee_percentage) / 10000;
        let net = amount - fee;

        if token == cfg.xtar_token {
            // XTAR: accumulate net to pot and store fee in vault
            update_pot_balance(&env, |_, pot_balance| {
                pot_balance.accumulated_xtar += net;
                pot_balance.last_updated = env.ledger().timestamp();
            });

            update_contract_vault(&env, |_, vault| {
                vault.dogstar_xtar += fee;
            });
        } else {
            // Generic token: accumulate net by token and accumulate fee in contract storage
            update_accumulated_by_token(&env, &token, |_, current| {
                *current += net;
            });
            
            if fee > 0 {
                update_dogstar_generic_fee(&env, &token, |_, current_fee| {
                    *current_fee += fee;
                });
            }
        }
    }
    pub fn initialize(e: Env, admin: Address, config: Config) {
        // Check if the contract is already initialized
        if has_administrator(&e) {
            panic!("already initialized");
        }
        bump_instance(&e);
        write_administrator(&e, &admin);
        write_config(&e, &config);
        write_balance(
            &e,
            &Balance {
                admin_power: 0,
                admin_terry: 0,
                haw_ai_power: 0,
                haw_ai_terry: 0,
                haw_ai_xtar: 0,
                total_deck_power: 0,
            },
        );
        
        // Initialize the contract vault
        write_contract_vault(
            &e,
            &ContractVault {
                haw_ai_pot_terry: 0,
                haw_ai_pot_power: 0,
                haw_ai_pot_xtar: 0,
                dogstar_terry: 0,
                dogstar_power: 0,
                dogstar_xtar: 0,
                total_claimable_terry: 0,
                total_claimable_power: 0,
                total_claimable_xtar: 0,
            },
        );

        let levels = vec![
            &e,
            Level {
                minimum_terry: 0,
                maximum_terry: 1000,
                name: String::from_str(&e, "PLASTICLOVER"),
            },
            Level {
                minimum_terry: 1001,
                maximum_terry: 5000,
                name: String::from_str(&e, "SHITCOINER"),
            },
            Level {
                minimum_terry: 5001,
                maximum_terry: 25000,
                name: String::from_str(&e, "MEMECE0"),
            },
            Level {
                minimum_terry: 25001,
                maximum_terry: 200000,
                name: String::from_str(&e, "CRYPTOBRO"),
            },
            Level {
                minimum_terry: 200001,
                maximum_terry: 500000,
                name: String::from_str(&e, "CHIEF"),
            },
            Level {
                minimum_terry: 500001,
                maximum_terry: 2000000,
                name: String::from_str(&e, "BOSS"),
            },
            Level {
                minimum_terry: 2000001,
                maximum_terry: 5000000,
                name: String::from_str(&e, "DIVINE"),
            },
            Level {
                minimum_terry: 5000001,
                maximum_terry: 10000000,
                name: String::from_str(&e, "LEGEND"),
            },
            Level {
                minimum_terry: 10000001,
                maximum_terry: 15000000,
                name: String::from_str(&e, "IMMORTAL"),
            },
            Level {
                minimum_terry: 15000001,
                maximum_terry: i128::MAX,
                name: String::from_str(&e, "Level 10"),
            },
        ];

        for (_, level) in levels.into_iter().enumerate() {
            add_level(&e, level);
        }

        // Emit initialization event
        e.events()
            .publish((Symbol::new(&e, "initialized"),), (admin,));
    }

    pub fn add_new_level(e: Env, level: Level) {
        let admin: Address = read_administrator(&e);
        admin.require_auth();
        bump_instance(&e);
        add_level(&e, level);
    }

    pub fn update_level(e: Env, level_id: u32, level: Level) {
        let admin: Address = read_administrator(&e);
        admin.require_auth();
        bump_instance(&e);
        update_level(&e, level_id, level);
    }

    pub fn mint_terry(e: Env, player: Address, amount: i128) {
        let admin = read_administrator(&e);
        admin.require_auth();
        bump_instance(&e);
        mint_terry(&e, player, amount);
    }

    pub fn batch_mint_terry(e: Env, to_addresses: Vec<Address>, amounts: Vec<i128>) {
        let admin = read_administrator(&e);
        admin.require_auth();
        bump_instance(&e);
        if to_addresses.len() != amounts.len() {
            panic!("Mismatched lengths of addresses and amounts");
        }
        
        // Define maximum mint amount per transaction (e.g., 1 billion)
        const MAX_MINT_AMOUNT: i128 = 1_000_000_000;
        const MAX_BATCH_SIZE: u32 = 100;
        
        assert!(to_addresses.len() <= MAX_BATCH_SIZE, "Batch size too large");
        
        // Validate all amounts before processing
        for amount in amounts.iter() {
            assert!(amount > 0, "Mint amount must be positive");
            assert!(amount <= MAX_MINT_AMOUNT, "Mint amount exceeds maximum");
        }

        for (to, amount) in to_addresses.iter().zip(amounts.iter()) {
            mint_terry(&e, to, amount);
        }
    }

    pub fn terry_balance(e: Env, player: Address) -> i128 {
        let user = read_user(&e, player);
        user.terry
    }

    pub fn mint(
        env: Env,
        user: Address,
        token_id: TokenId,
        card_level: u32,
        buy_currency: Currency,
    ) {
        user.require_auth();
        bump_instance(&env);

        let user: User = read_user(&env, user.clone());
        let to: Address = user.owner.clone();
        let user_level = get_user_level(&env, to.clone());

        assert!(
            user_level >= card_level,
            "User level too low to mint this card"
        );
        assert!(
            !Self::exists(&env, to.clone(), token_id.clone()),
            "Token ID already exists"
        );

        let card_metadata = read_metadata(&env, token_id.clone().0);
        let nft = Card {
            power: card_metadata.initial_power,
            locked_by_action: Action::None,
        };
        write_nft(&env, to.clone(), token_id.clone(), nft.clone());

        add_card_to_owner(&env, token_id.clone(), to.clone()).map_err(|_e| NFTError::NotAuthorized).unwrap();


        let config: Config = read_config(&env);
        
        update_balance(&env, |e, balance| {
            if buy_currency == Currency::Terry {
                let amount = card_metadata.price_terry;
                assert!(user.terry >= amount, "Not enough terry to burn");
                let withdrawable_amount = (config.withdrawable_percentage as i128 * amount) / 100;
                let haw_ai_amount = amount - withdrawable_amount;
                burn_terry(e, user.owner.clone(), amount);
                balance.admin_terry += withdrawable_amount;
                crate::pot::management::accumulate_pot_internal(e, haw_ai_amount, 0, 0, Some(user.owner.clone()), Some(Action::Mint));
            } else {
                let token = token::Client::new(e, &config.xtar_token.clone());
                let burnable_amount =
                    (config.burnable_percentage as i128 * card_metadata.price_xtar) / 100;
                let haw_ai_amount = card_metadata.price_xtar - burnable_amount;
                token.burn(&to.clone(), &burnable_amount);
                
                // Transfer XTAR to contract instead of external address
                token.transfer(&to.clone(), &e.current_contract_address(), &haw_ai_amount);
                
                // Store XTAR in contract vault
                update_contract_vault(e, |_, vault| {
                    vault.haw_ai_pot_xtar += haw_ai_amount;
                });
                
                balance.haw_ai_xtar += haw_ai_amount;
                crate::pot::management::accumulate_pot_internal(e, 0, 0, haw_ai_amount, Some(user.owner.clone()), Some(Action::Mint));
            };
        });

        // Emit mint event
        emit_mint(&env, &to);
    }

    pub fn transfer(env: Env, from: Address, to: Address, token_id: TokenId) {
        from.require_auth();
        bump_instance(&env);
        let nft: Card = read_nft(&env, from.clone(), token_id.clone()).unwrap();
        // Prevent transferring cards locked by an action
        assert!(nft.locked_by_action == Action::None, "Card is locked by an action");
        // Move NFT and update owner indexes atomically
        Self::move_nft(&env, from.clone(), to.clone(), token_id, nft);

        // Emit transfer event
        emit_transfer(&env, &from, &to);
    }

    pub fn burn(env: Env, user: Address, token_id: TokenId) {
        bump_instance(&env);
        burn::burn(env, user, token_id)
    }

    pub fn upgrade(e: Env, new_wasm_hash: BytesN<32>) {
        let admin: Address = read_administrator(&e);
        admin.require_auth();
        bump_instance(&e);
        e.deployer().update_current_contract_wasm(new_wasm_hash);
    }

    pub fn set_admin(e: Env, new_admin: Address) {
        let admin = read_administrator(&e);
        admin.require_auth();
        bump_instance(&e);
        write_administrator(&e, &new_admin);
        TokenUtils::new(&e).events().set_admin(admin, new_admin);
    }

    pub fn check_admin(e: Env) -> bool {
        let admin = read_administrator(&e);
        admin.require_auth();
        true
    }

    pub fn check_admin_address(e: Env) -> Address {
        let admin = read_administrator(&e);
        admin.require_auth();
        admin
    }

    pub fn maintenance(e: Env) {
        // Anyone can call this to keep the contract alive
        bump_instance(&e);
        crate::admin::touch_globals(&e);
    }

    pub fn add_level(e: &Env, level: Level) -> u32 {
        bump_instance(e);
        add_level(e, level)
    }

    pub fn add_to_whitelist(e: &Env, members: Vec<Address>) {
        let admin = read_administrator(e);
        admin.require_auth();
        bump_instance(e);
        for member in members.iter() {
            e.storage()
                .persistent()
                .set(&DataKey::Whitelist(member.clone()), &true);
            e.storage().persistent().extend_ttl(
                &DataKey::Whitelist(member),
                STORAGE_THRESHOLD_LEDGERS,
                STORAGE_BUMP_LEDGERS,
            );
        }
    }

    pub fn remove_from_whitelist(e: &Env, members: Vec<Address>) {
        let admin = read_administrator(e);
        admin.require_auth();
        bump_instance(e);
        for member in members.iter() {
            e.storage()
                .persistent()
                .remove(&DataKey::Whitelist(member.clone()));
        }
    }

    pub fn card(env: &Env, owner: Address, token_id: TokenId) -> Option<Card> {
        read_nft(env, owner, token_id)
    }

    pub fn exists(env: &Env, owner: Address, token_id: TokenId) -> bool {
        exists(env, owner, token_id)
    }

    pub fn admin_balance(env: &Env) -> Balance {
        read_balance(&env)
    }

    pub fn admin_state(env: &Env) -> State {
        read_state(&env)
    }

    pub fn config(env: Env) -> Config {
        read_config(&env)
    }

    pub fn create_metadata(e: &Env, card: CardMetadata, id: u32) {
        let admin = read_administrator(&e);
        admin.require_auth();
        bump_instance(e);
        write_metadata(e, id, card);
    }

    pub fn get_card(e: &Env, id: u32) -> CardMetadata {
        read_metadata(e, id)
    }

    pub fn create_user(e: Env, address: Address) {
        let admin = read_administrator(&e);
        admin.require_auth();
        bump_instance(&e);
        let user: User = User {
            owner: address.clone(),
            power: 100,
            terry: 0,
            total_history_terry: 0,
            level: 1,
        };
        write_user(&e, address, user);
    }

    pub fn get_all_cards(e: &Env) -> soroban_sdk::Vec<CardMetadata> {
        let mut all_cards = soroban_sdk::Vec::new(&e);
        let key = DataKey::AllCardIds;
        if e.storage().persistent().has(&key) {
            e.storage().persistent().extend_ttl(
                &key,
                STORAGE_THRESHOLD_LEDGERS,
                STORAGE_BUMP_LEDGERS,
            );
        }
        let card_ids = e
            .storage()
            .persistent()
            .get::<DataKey, soroban_sdk::Vec<TokenId>>(&key)
            .unwrap_or(soroban_sdk::Vec::new(&e));
        for token_id in card_ids.iter() {
            let card_metadata = read_metadata(e, token_id.0);
            all_cards.push_back(card_metadata);
        }
        all_cards
    }

    pub fn get_player_cards_with_state(
        e: &Env,
        player: Address,
    ) -> soroban_sdk::Vec<(CardMetadata, Card)> {
        let mut player_cards = soroban_sdk::Vec::new(&e);
        let owned_card_ids = read_owner_card(e, player.clone());
        for token_id in owned_card_ids.iter() {
            let card_metadata = read_metadata(e, token_id.0);
            if let Some(card) = read_nft(e, player.clone(), token_id.clone()) {
                player_cards.push_back((card_metadata, card));
            }
        }
        player_cards
    }

    pub fn add_power_to_card(env: &Env, player: Address, token_id: u32, amount: u32) {
        bump_instance(env);
        
        // Single writer update
        update_nft(env, player.clone(), TokenId(token_id), |e, card| {
            // Cap power to metadata max
            let metadata = crate::metadata::read_metadata(e, token_id);
            let new_power = (card.power as u128 + amount as u128)
                .min(metadata.max_power as u128) as u32;
            card.power = new_power;
        });

        update_user(env, player.clone(), |_, user| {
            assert!(user.power >= amount, "Insufficient user POWER");
            user.power = user.power.checked_sub(amount).expect("POWER underflow");
        });
    }

    pub fn read_user(env: &Env, player: Address) -> User {
        read_user(env, player.clone())
    }
}

//Pot Management

#[contractimpl]
impl NFT {
    pub fn get_current_pot_state(env: Env) -> (PotBalance, DogstarBalance) {
        (read_pot_balance(&env), read_dogstar_balance(&env))
    }

    // Consolidated read for UI: totals in the pot (since last opening)
    // and per-user claimables across core assets and registered generic SACs.
    pub fn view_pot_overview(env: Env, user: Address) -> PotOverview {
        let pot = read_pot_balance(&env);
        let registered = read_registered_tokens(&env);

        // Totals for generic tokens currently accumulated in the pot
        let mut total_generics: Vec<GenericTokenAmount> = Vec::new(&env);
        for token in registered.iter() {
            let amt = read_accumulated_by_token(&env, &token);
            if amt > 0 { total_generics.push_back(GenericTokenAmount { token: token.clone(), amount: amt }); }
        }

        // Per-user claimables (core assets)
        let user_claim = read_user_claimable_balance(&env, &user);

        // Per-user claimables for registered generic tokens
        let mut user_generics: Vec<GenericTokenAmount> = Vec::new(&env);
        for token in registered.iter() {
            let amt = read_user_generic_claimable(&env, &user, &token);
            if amt > 0 { user_generics.push_back(GenericTokenAmount { token: token.clone(), amount: amt }); }
        }

        PotOverview {
            total_terry: pot.accumulated_terry,
            total_power: pot.accumulated_power,
            total_xtar: pot.accumulated_xtar,
            total_generics,
            user_terry: user_claim.terry,
            user_power: user_claim.power,
            user_xtar: user_claim.xtar,
            user_generics,
        }
    }

    pub fn get_player_potential_reward(env: Env, player: Address) -> PendingReward {
        let current_round = get_current_round(&env);
        let balance = read_pot_balance(&env);
        let deck = read_deck(env.clone(), player.clone());
        let players = get_eligible_players(&env);
        let mut total_effective_power: u128 = 0;
        for p in players.iter() {
            let d = read_deck(env.clone(), p.clone());
            if d.token_ids.len() == 4 {
                total_effective_power += calculate_effective_power(d.total_power, d.bonus) as u128;
            }
        }
        const PRECISION: u128 = 10000;
        let effective_power = calculate_effective_power(deck.total_power, deck.bonus) as u128;
        let share = if total_effective_power > 0 {
            ((effective_power * PRECISION) / total_effective_power) as u128
        } else {
            0
        };

        let registered = read_registered_tokens(&env);
        let mut generic_rewards = Vec::new(&env);
        for token in registered.iter() {
            let amt = read_accumulated_by_token(&env, &token);
            if amt > 0 {
                let reward_amt = (amt * share as i128) / PRECISION as i128;
                generic_rewards.push_back(GenericTokenAmount { token: token.clone(), amount: reward_amt });
            }
        }

        PendingReward {
            round_number: current_round,
            terry_amount: (balance.accumulated_terry * share as i128) / PRECISION as i128,
            power_amount: (balance.accumulated_power as u128 * share / PRECISION) as u32,
            xtar_amount: (balance.accumulated_xtar * share as i128) / PRECISION as i128,
            generic_tokens: generic_rewards,
            status: RewardStatus::Pending,
        }
    }

    pub fn get_historical_snapshot(e: Env, round: u32) -> Option<PotSnapshot> {
        read_pot_snapshot(&e, round)
    }

    pub fn get_player_participation(env: Env, player: Address, round: u32) -> Option<PlayerReward> {
        read_player_reward(&env, round, &player)
    }

    pub fn get_pending_rewards(env: Env, player: Address) -> Vec<PendingReward> {
        let mut pending = Vec::new(&env);
        for round in get_all_rounds(&env).iter() {
            if let Some(reward) = read_pending_reward(&env, round, &player) {
                if reward.status != RewardStatus::Claimed {
                    pending.push_back(reward);
                }
            }
        }
        pending
    }

    pub fn accumulate_pot(env: Env, terry: i128, power: u32, xtar: i128, from: Option<Address>, action: Option<Action>) {                                      
        if xtar < 0 {
            panic!("Negative XTAR contributions not allowed");
        }

        let admin = read_administrator(&env);
        admin.require_auth();
        bump_instance(&env);
        let config = read_config(&env);

        if xtar > 0 {
            let funding_source = from.clone().unwrap_or_else(|| admin.clone());
            if funding_source != admin {
                funding_source.require_auth();
            }
            let token_client = token::Client::new(&env, &config.xtar_token);
            token_client.transfer(&funding_source, &env.current_contract_address(), &xtar);
        }

        let fee_percentage = config.dogstar_fee_percentage;
        let terry_fee = (terry * fee_percentage as i128) / 10000;
        let power_fee = (power * fee_percentage) / 10000;
        let xtar_fee = (xtar * fee_percentage as i128) / 10000;
        
        // Accumulate in pot balance (minus dogstar fees)
        update_pot_balance(&env, |_, pot_balance| {
            pot_balance.accumulated_terry += terry - terry_fee;
            pot_balance.accumulated_power += power - power_fee;
            pot_balance.accumulated_xtar += xtar - xtar_fee;
            pot_balance.last_updated = env.ledger().timestamp();
        });
        
        // Store dogstar fees in vault instead of separate balance
        update_contract_vault(&env, |_, vault| {
            vault.dogstar_terry += terry_fee;
            vault.dogstar_power += power_fee;
            vault.dogstar_xtar += xtar_fee;
        });
        
        // Keep old balance for backward compatibility (can be removed later)
        update_dogstar_balance(&env, |_, dogstar_balance| {
            dogstar_balance.terry += terry_fee;
            dogstar_balance.power += power_fee;
            dogstar_balance.xtar += xtar_fee;
        });
        
        if terry_fee > 0 || power_fee > 0 || xtar_fee > 0 {
            emit_dogstar_fee_accumulated(&env, terry_fee, power_fee, xtar_fee, fee_percentage, from, action);
        }
    }

    pub fn claim_dogstar_fees(env: Env, claimer: Address) {
        // Restrict to admin only for protocol fee claims
        let admin = read_administrator(&env);
        admin.require_auth();
        bump_instance(&env);
        assert!(claimer == admin, "Only admin can claim dogstar fees");
        
        let config = read_config(&env);
        let mut vault = read_contract_vault(&env);
        
        let terry_to_claim = vault.dogstar_terry;
        let power_to_claim = vault.dogstar_power;
        let xtar_to_claim = vault.dogstar_xtar;

        // Check for generic fees
        let registered = read_registered_tokens(&env);
        let mut has_generic = false;
        for token in registered.iter() {
            if read_dogstar_generic_fee(&env, &token) > 0 { has_generic = true; break; }
        }

        if terry_to_claim == 0 && power_to_claim == 0 && xtar_to_claim == 0 && !has_generic {
            panic!("No fees available to claim");
        }
        
        // Transfer assets to claimer
        if terry_to_claim > 0 {
            update_user(&env, claimer.clone(), |_, user| {
                user.terry += terry_to_claim;
            });
            vault.dogstar_terry = 0;
        }
        
        if power_to_claim > 0 {
            update_user(&env, claimer.clone(), |_, user| {
                user.power += power_to_claim;
            });
            vault.dogstar_power = 0;
        }
        
        if xtar_to_claim > 0 {
            let token = token::Client::new(&env, &config.xtar_token);
            token.transfer(&env.current_contract_address(), &claimer, &xtar_to_claim);
            vault.dogstar_xtar = 0;
        }
        
        // Claim generic tokens
        for token in registered.iter() {
            let fee = read_dogstar_generic_fee(&env, &token);
            if fee > 0 {
                let client = token::Client::new(&env, &token);
                client.transfer(&env.current_contract_address(), &claimer, &fee);
                update_dogstar_generic_fee(&env, &token, |_, current_fee| {
                    *current_fee = 0;
                });
            }
        }

        // Write updated vault
        update_contract_vault(&env, |_, v| {
            // Re-read to ensure no race condition, although we are in single thread execution conceptually
            // In the helper, we get &mut ContractVault.
            // We need to apply the changes we calculated based on the read snapshot `vault` at the beginning.
            // BUT wait, `vault` was read at the top. If I use `update_contract_vault` now, I should re-apply the diffs.
            // Or better, just move the whole logic inside `update_contract_vault`.
            // However, that involves many side effects (transfers).
            // Since this is `claim_dogstar_fees`, and only Admin can call it, and we are not in parallel execution in Soroban (yet?), 
            // but for "Lost Update" protection, we should use the helper.
            // The issue is the side effects (transfers) happening based on the values.
            
            // Correct pattern:
            // 1. Read vault (already done).
            // 2. Determine what to claim.
            // 3. Perform transfers.
            // 4. Update vault using helper, zeroing out what was claimed.
            
            if terry_to_claim > 0 { v.dogstar_terry = 0; }
            if power_to_claim > 0 { v.dogstar_power = 0; }
            if xtar_to_claim > 0 { v.dogstar_xtar = 0; }
        });
        
        emit_dogstar_fee_withdrawn(&env, &claimer, terry_to_claim, power_to_claim, xtar_to_claim);
    }
    
    // Admin function to make dogstar fees claimable
    // DEPRECATED: Fees are now claimed directly from accumulated vault in claim_dogstar_fees
    pub fn release_dogstar_fees(env: Env) {
        let admin = read_administrator(&env);
        admin.require_auth();
        bump_instance(&env);
        // No-op to prevent double-accounting bug.
        // Logic moved to unified claim_dogstar_fees.
    }

    pub fn open_pot(env: Env, round: u32) -> Result<(), NFTError> {
        let admin = read_administrator(&env);
        admin.require_auth();
        bump_instance(&env);
        let current_round = get_current_round(&env);
        if round <= current_round {
            return Err(NFTError::RoundAlreadyProcessed);
        }
        let balance = read_pot_balance(&env);
        
        // Move pot balance to vault for distribution
        update_contract_vault(&env, |_, vault| {
            vault.haw_ai_pot_terry += balance.accumulated_terry;
            vault.haw_ai_pot_power += balance.accumulated_power;
            vault.haw_ai_pot_xtar += balance.accumulated_xtar;
            vault.total_claimable_terry += balance.accumulated_terry;
            vault.total_claimable_power += balance.accumulated_power;
            vault.total_claimable_xtar += balance.accumulated_xtar;
        });
        
        // Snapshot generic SAC tokens
        let registered = read_registered_tokens(&env);
        let mut generic_tokens = Vec::new(&env);
        for token_addr in registered.iter() {
            let amt = read_accumulated_by_token(&env, &token_addr);
            if amt > 0 { generic_tokens.push_back(crate::storage_types::GenericTokenAmount { token: token_addr.clone(), amount: amt }); }
            // reset accumulated for next cycle
            if amt != 0 { 
                update_accumulated_by_token(&env, &token_addr, |_, current| {
                    *current = 0;
                });
            }
        }

        let snapshot = PotSnapshot {
            round_number: round,
            total_terry: balance.accumulated_terry,
            total_power: balance.accumulated_power,
            total_xtar: balance.accumulated_xtar,
            timestamp: env.ledger().timestamp(),
            total_participants: 0,
            total_effective_power: 0,
            generic_tokens,
        };
        write_pot_snapshot(&env, round, &snapshot);
        emit_pot_opened(&env, round, &snapshot);
        
        // Calculate and store player shares as claimable balances
        Self::calculate_and_store_claimable_shares(&env, round, &snapshot);
        
        set_current_round(&env, round);
        add_round(&env, round);
        
        update_pot_balance(&env, |e, pot| {
            pot.accumulated_terry = 0;
            pot.accumulated_power = 0;
            pot.accumulated_xtar = 0;
            pot.last_opening_round = round;
            pot.total_openings = balance.total_openings + 1;
            pot.last_updated = e.ledger().timestamp();
        });

        Ok(())
    }
    
    fn calculate_and_store_claimable_shares(env: &Env, round: u32, snapshot: &PotSnapshot) {
        calculate_player_shares(env, round);
        
        // Get all participants for this round
        let decks = read_decks(env.clone());
        
        for deck in decks.iter() {
            if let Some(player_reward) = read_player_reward(env, round, &deck.owner) {
                // Calculate share based on effective power
                let share_percentage = player_reward.share_percentage;
                
                let terry_share = (snapshot.total_terry * share_percentage as i128) / 10000;
                let power_share = (snapshot.total_power * share_percentage) / 10000;
                let xtar_share = (snapshot.total_xtar * share_percentage as i128) / 10000;
                
                // Update user's claimable balance
                update_user_claimable_balance(env, &deck.owner, |_, user_claimable| {
                    user_claimable.terry += terry_share;
                    user_claimable.power += power_share;
                    user_claimable.xtar += xtar_share;
                    user_claimable.last_claim_round = round;
                });

                // Add generic SAC token claimables from snapshot
                for gta in snapshot.generic_tokens.iter() {
                    let token_share = (gta.amount * share_percentage as i128) / 10000;
                    if token_share > 0 {
                        update_user_generic_claimable(env, &deck.owner, &gta.token, |_, current| {
                            *current += token_share;
                        });
                    }
                }
            }
        }
    }

    pub fn claim_haw_ai_pot_share(env: Env, player: Address) -> Result<(i128, u32, i128), NFTError> {
        player.require_auth();
        bump_instance(&env);

        let mut claimable = read_user_claimable_balance(&env, &player);
        let config = read_config(&env);

        // Also consider generic SAC claimables before failing
        let mut has_generic = false;
        let tokens = read_registered_tokens(&env);
        for token in tokens.iter() {
            if read_user_generic_claimable(&env, &player, &token) > 0 { has_generic = true; break; }
        }
        if claimable.terry == 0 && claimable.power == 0 && claimable.xtar == 0 && !has_generic {
            return Err(NFTError::NoRewardsAvailable);
        }
        
        let terry_to_claim = claimable.terry;
        let power_to_claim = claimable.power;
        let xtar_to_claim = claimable.xtar;
        
        // Transfer assets to player
        if terry_to_claim > 0 {
            mint_terry(&env, player.clone(), terry_to_claim);
        }
        
        if power_to_claim > 0 {
            update_user(&env, player.clone(), |_, user| {
                user.power += power_to_claim;
            });
        }
        
        if xtar_to_claim > 0 {
            let token = token::Client::new(&env, &config.xtar_token);
            token.transfer(&env.current_contract_address(), &player, &xtar_to_claim);
        }
        
        // Update claim record
        update_user_claimable_balance(&env, &player, |e, claimable_rec| {
             claimable_rec.terry = 0;
             claimable_rec.power = 0;
             claimable_rec.xtar = 0;
             claimable_rec.last_claim_timestamp = e.ledger().timestamp();
        });
        
        // Update vault to reflect claimed amounts
        update_contract_vault(&env, |_, vault| {
            vault.total_claimable_terry -= terry_to_claim;
            vault.total_claimable_power -= power_to_claim;
            vault.total_claimable_xtar -= xtar_to_claim;
        });
        
        // Emit event
        emit_rewards_claimed(&env, &player, terry_to_claim, power_to_claim, xtar_to_claim);

        // Sweep generic SAC token claimables
        let tokens = read_registered_tokens(&env);
        for token in tokens.iter() {
            let to_claim = read_user_generic_claimable(&env, &player, &token);
            if to_claim > 0 {
                let client = token::Client::new(&env, &token);
                client.transfer(&env.current_contract_address(), &player, &to_claim);
                update_user_generic_claimable(&env, &player, &token, |_, current| {
                    *current = 0;
                });
            }
        }

        Ok((terry_to_claim, power_to_claim, xtar_to_claim))
    }
    
    pub fn view_claimable_balance(env: Env, player: Address) -> UserClaimableBalance {
        read_user_claimable_balance(&env, &player)
    }
    
    pub fn view_vault_status(env: Env) -> ContractVault {
        read_contract_vault(&env)
    }
    
    pub fn claim_all_pending_rewards(env: Env, player: Address) -> Result<(i128, u32, i128), NFTError> {
        // Legacy function - redirect to new claim function
        bump_instance(&env);
        Self::claim_haw_ai_pot_share(env, player)
    }

    pub fn update_dogstar_fee_percentage(env: Env, fee_percentage: u32) {
        let admin = read_administrator(&env);
        admin.require_auth();
        bump_instance(&env);
        
        // Maximum fee percentage (50% = 5000 basis points)
        const MAX_FEE_PERCENTAGE: u32 = 5000;
        assert!(fee_percentage <= MAX_FEE_PERCENTAGE, "Fee percentage exceeds maximum (50%)");

        let mut old_fee = 0;
        update_config(&env, |_, config| {
            old_fee = config.dogstar_fee_percentage;
            config.dogstar_fee_percentage = fee_percentage;
        });
        emit_dogstar_fee_percentage_updated(&env, old_fee, fee_percentage);
    }

    pub fn contribute_to_pot(env: Env, terry: i128, power: u32, xtar: i128) {
        let admin = read_administrator(&env);
        admin.require_auth();
        bump_instance(&env);
        assert!(
            terry >= 0 && xtar >= 0,
            "Negative contributions not allowed"
        );
        Self::accumulate_pot(env, terry, power, xtar, None, None);
    }

    pub fn get_eligible_players(env: Env) -> Vec<Address> {
        get_eligible_players(&env)
    }

    pub fn get_eligible_players_with_shares(
        env: Env,
    ) -> Vec<(Address, u32, u32, u32, Vec<(u32, u32, Category)>, u32)> {
        get_eligible_players_with_shares(&env)
    }

    pub fn get_all_rounds(env: Env) -> Vec<u32> {
        get_all_rounds(&env)
    }

    pub fn get_current_round(env: Env) -> u32 {
        get_current_round(&env)
    }
}

// Stake, Fight, Lend & Borrow, Deck sections unchanged
#[contractimpl]
impl NFT {
    pub fn stake(
        env: Env,
        user: Address,
        category: Category,
        token_id: TokenId,
        period_index: u32,
    ) {
        bump_instance(&env);
        stake::stake(env, user, category, token_id, period_index)
    }

    pub fn increase_stake_power(
        env: Env,
        user: Address,
        category: Category,
        token_id: TokenId,
        increase_power: u32,
    ) {
        bump_instance(&env);
        stake::increase_stake_power(env, user, category, token_id, increase_power)
    }

    pub fn unstake(env: Env, user: Address, category: Category, token_id: TokenId) {
        bump_instance(&env);
        stake::unstake(env, user, category, token_id)
    }

    pub fn read_stake(
        env: &Env,
        user: Address,
        category: Category,
        token_id: TokenId,
    ) -> stake::Stake {
        stake::read_stake(env, user, category, token_id)
    }

    pub fn read_stakes(env: Env) -> Vec<stake::Stake> {
        stake::read_stakes(env)
    }
}

#[contractimpl]
impl NFT {
    pub fn open_position(
        env: Env,
        owner: Address,
        category: Category,
        token_id: TokenId,
        currency: fight::FightCurrency,
        side_position: SidePosition,
        leverage: u32,
        power_staked: u32,
    ) {
        bump_instance(&env);
        fight::open_position(
            env,
            owner,
            category,
            token_id,
            currency,
            side_position,
            leverage,
            power_staked,
        )
    }

    pub fn close_position(env: Env, owner: Address, category: Category, token_id: TokenId) {
        bump_instance(&env);
        fight::close_position(env, owner, category, token_id)
    }

    pub fn currency_price(env: Env, oracle_contract_id: Address) -> i128 {
        fight::get_currency_price(env, oracle_contract_id, fight::FightCurrency::BTC)
    }

    pub fn read_fight(
        env: Env,
        user: Address,
        category: Category,
        token_id: TokenId,
    ) -> fight::Fight {
        fight::read_fight(env, user, category, token_id)
    }

    pub fn read_fights(env: Env) -> Vec<fight::Fight> {
        fight::read_fights(env)
    }

    pub fn check_liquidation(env: Env, liquidator: Address, user: Address, category: Category, token_id: TokenId) {
        bump_instance(&env);
        fight::check_liquidation(env, liquidator, user, category, token_id)
    }
}

#[contractimpl]
impl NFT {
    pub fn lend(env: Env, lender: Address, category: Category, token_id: TokenId, power: u32) {
        bump_instance(&env);
        lending::lend(env, lender, category, token_id, power)
    }

    pub fn borrow(env: Env, borrower: Address, category: Category, token_id: TokenId, power: u32) {
        bump_instance(&env);
        lending::borrow(env, borrower, category, token_id, power)
    }

    pub fn repay(env: Env, borrower: Address, category: Category, token_id: TokenId) {
        bump_instance(&env);
        lending::repay(env, borrower, category, token_id)
    }

    pub fn withdraw(env: Env, lender: Address, category: Category, token_id: TokenId) {
        bump_instance(&env);
        lending::withdraw(env, lender, category, token_id)
    }

    pub fn get_current_apy(env: Env) -> u64 {
        lending::get_current_apy(env)
    }

    pub fn borrow_quote(
        env: Env,
        borrower: Address,
        category: Category,
        token_id: TokenId,
        power: u32,
    ) -> lending::BorrowQuote {
        lending::borrow_quote(env, borrower, category, token_id, power)
    }

    pub fn read_lending(
        env: Env,
        player: Address,
        category: Category,
        token_id: TokenId,
    ) -> lending::Lending {
        lending::read_lending(env, player, category, token_id)
    }

    pub fn read_borrowing(
        env: Env,
        player: Address,
        category: Category,
        token_id: TokenId,
    ) -> lending::Borrowing {
        lending::read_borrowing(env, player, category, token_id)
    }

    pub fn read_borrowings(env: Env) -> Vec<Borrowing> {
        lending::read_borrowings(env)
    }

    pub fn read_lendings(env: Env) -> Vec<Lending> {
        lending::read_lendings(env)
    }

    pub fn touch_loans(env: Env, loans: Vec<(Address, Category, TokenId)>) {
        lending::touch_loans(env, loans)
    }
}

#[contractimpl]
impl NFT {
    pub fn place(env: Env, owner: Address, token_id: TokenId) {
        bump_instance(&env);
        deck::place(env, owner, token_id);
    }

    pub fn replace(env: Env, owner: Address, prev_token_id: TokenId, token_id: TokenId) {
        bump_instance(&env);
        deck::replace(env, owner, prev_token_id, token_id);
    }

    pub fn remove_place(env: Env, owner: Address, token_id: TokenId) {
        bump_instance(&env);
        deck::remove_place(env, owner, token_id)
    }

    pub fn read_deck(env: Env, owner: Address) -> Deck {
        deck::read_deck(env, owner)
    }
}
