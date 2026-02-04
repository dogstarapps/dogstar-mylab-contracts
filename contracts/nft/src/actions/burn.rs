use crate::event::emit_burn;
use crate::*;
use admin::read_config;
use metadata::read_metadata;
use nft_info::{read_nft, remove_nft, Action};
use soroban_sdk::{Address, Env};
use storage_types::TokenId;
use user_info::{update_user, update_owner_cards};

pub fn burn(env: Env, user: Address, token_id: TokenId) {
    user.require_auth();
    // We don't need read_user here anymore for the update logic, 
    // but we need 'owner' address. Let's read just enough or use update_user pattern.
    // read_user is cheap if cached, but let's just get owner from Address if possible? 
    // No, 'user' is the address/owner.
    let owner = user.clone();

    let config = read_config(&env);
    let nft = read_nft(&env, owner.clone(), token_id.clone()).unwrap();
    let card_metadata = read_metadata(&env, token_id.0);

    // Calculate Terry and Power amounts
    let terry_amount =
        card_metadata.price_terry * (nft.power as i128 / card_metadata.initial_power as i128) / 2;
    let receive_amount = terry_amount * config.burn_receive_percentage as i128 / 100;
    let pot_terry = terry_amount - receive_amount; // Terry to pot
    let total_power = card_metadata.initial_power + nft.power / 2;
    let receive_power = total_power as i128 * config.burn_receive_percentage as i128 / 100;
    let pot_power = total_power as i128 - receive_power;

    // Mint owner's share (Atomic update of Power + Terry + Level)
    update_user(&env, owner.clone(), |_, u| {
        u.power += receive_power as u32;
        u.terry += receive_amount;
        u.total_history_terry += receive_amount;
    });

    // Accumulate to pot with Dogstar fee deduction (internal helper, no admin auth)
    crate::pot::management::accumulate_pot_internal(
        &env,
        pot_terry,
        pot_power as u32,
        0,
        Some(owner.clone()),
        Some(Action::Burn),
    );

    // Emit burn event
    emit_burn(&env, &owner);

    // Remove card and NFT
    remove_owner_card(&env, owner.clone(), token_id.clone());
    remove_nft(&env, owner, token_id);
}

pub fn remove_owner_card(env: &Env, owner: Address, token_id: TokenId) {
    update_owner_cards(env, owner.clone(), |_, user_card_ids: &mut soroban_sdk::Vec<TokenId>| {
        if let Some(index) = user_card_ids.iter().position(|x| x == token_id) {
            user_card_ids.remove(index as u32);
        }
    });
}
