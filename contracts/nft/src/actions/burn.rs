use crate::event::emit_burn;
use crate::{user_info::mint_terry, *};
use admin::read_config;
use metadata::read_metadata;
use nft_info::{read_nft, remove_nft, Action};
use soroban_sdk::{Address, Env};
use storage_types::TokenId;
use user_info::{read_owner_card, read_user, write_owner_card, write_user};

pub fn burn(env: Env, user: Address, token_id: TokenId) {
    user.require_auth();
    let mut user = read_user(&env, user.clone());
    let owner = user.owner.clone();

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

    // Mint owner's share
    user.power += receive_power as u32;
    write_user(&env.clone(), owner.clone(), user);
    mint_terry(&env, owner.clone(), receive_amount);
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
    let mut user_card_ids = read_owner_card(&env, owner.clone());
    let index = user_card_ids.iter().position(|x| x == token_id).unwrap();
    user_card_ids.remove(index as u32);
    write_owner_card(&env, owner.clone(), user_card_ids);
}
