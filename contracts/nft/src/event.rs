use crate::nft_info::{Action, Category};
use crate::storage_types::{PendingReward, PlayerReward, PotSnapshot, TokenId};
use soroban_sdk::{symbol_short, Address, Env};

// Event Emission
/// Emits an event when the pot is opened.
pub fn emit_pot_opened(env: &Env, round: u32, snapshot: &PotSnapshot) {
    env.events().publish(
        (symbol_short!("pot_open"), round),
        (
            snapshot.total_terry,
            snapshot.total_power,
            snapshot.total_xtar,
            snapshot.total_participants,
            snapshot.total_effective_power,
        ),
    );
}

/// Emits an event when a player's share is calculated.
pub fn emit_share_calculated(env: &Env, player: &Address, reward: &PlayerReward) {
    env.events().publish(
        (symbol_short!("share_cal"), player.clone()),
        (
            reward.round_number,
            reward.share_percentage,
            reward.effective_power,
            reward.deck_bonus,
            reward.deck_categories,
        ),
    );
}

/// Emits an event when Dogstar fees are accumulated.
pub fn emit_dogstar_fee_accumulated(
    env: &Env,
    terry: i128,
    power: u32,
    xtar: i128,
    fee_percentage: u32,
    from: Option<Address>,
    action: Option<Action>,
) {
    let action_val = action.unwrap_or(Action::None);

    // let demo_account_strkey = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF";

    // Convert &str to soroban_sdk::String
    // let demo_address_string = String::from_str(&env, demo_account_strkey);

    // Use the soroban_sdk::String to create the Address
    // let demo_address = Address::from_string(&demo_address_string);
    // let from_val = from.unwrap_or(Address::from_string(String::from(&"GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF")));

    env.events().publish(
        (symbol_short!("fee_acc"),),
        (terry, power, xtar, fee_percentage, from, action_val),
    );
}

/// Emits an event when Dogstar fees are withdrawn.
pub fn emit_dogstar_fee_withdrawn(
    env: &Env,
    recipient: &Address,
    terry: i128,
    power: u32,
    xtar: i128,
) {
    env.events().publish(
        (symbol_short!("fee_wd"), recipient.clone()),
        (terry, power, xtar),
    );
}

/// Emits an event when the Dogstar fee percentage is updated.
pub fn emit_dogstar_fee_percentage_updated(env: &Env, old_fee: u32, new_fee: u32) {
    env.events()
        .publish((symbol_short!("fee_pct"),), (old_fee, new_fee));
}

/// Emits an event when a reward is claimed.
pub fn emit_reward_claimed(e: &Env, player: &Address, reward: &PendingReward) {
    e.events().publish(
        (
            symbol_short!("rwd_cld"),
            player.clone(),
            reward.round_number,
        ),
        (reward.terry_amount, reward.power_amount, reward.xtar_amount),
    );
}

/// Emits an event when a reward is marked as pending due to missing trustline.
pub fn emit_reward_pending(e: &Env, player: &Address, reward: &PendingReward) {
    e.events().publish(
        (
            symbol_short!("reward_pd"),
            player.clone(),
            reward.round_number,
        ),
        (reward.terry_amount, reward.power_amount, reward.xtar_amount),
    );
}

/// Emits an event when rewards are claimed from HAW AI pot.
pub fn emit_rewards_claimed(e: &Env, player: &Address, terry: i128, power: u32, xtar: i128) {
    e.events().publish(
        (symbol_short!("rwd_claim"), player.clone()),
        (terry, power, xtar),
    );
}

/// Emits an event when a card is burned.
pub fn emit_burn(env: &Env, player: &Address) {
    env.events()
        .publish((symbol_short!("burn"), player.clone()), ());
}

/// Emits an event when a card is staked.
pub fn emit_stake(env: &Env, player: &Address, category: &Category, token_id: &TokenId) {
    env.events().publish(
        (
            symbol_short!("stake"),
            symbol_short!("open"),
            player.clone(),
        ),
        (player.clone(), category.clone(), token_id.clone()),
    );
}

/// Emits an event when stake power is increased.
pub fn emit_stake_increased(
    env: &Env,
    player: &Address,
    category: &Category,
    token_id: &TokenId,
) {
    env.events().publish(
        (
            symbol_short!("stake"),
            symbol_short!("increase"),
            player.clone(),
        ),
        (player.clone(), category.clone(), token_id.clone()),
    );
}

/// Emits an event when a card is unstaked.
pub fn emit_unstake(env: &Env, player: &Address, category: &Category, token_id: &TokenId) {
    env.events().publish(
        (
            symbol_short!("stake"),
            symbol_short!("close"),
            player.clone(),
        ),
        (player.clone(), category.clone(), token_id.clone()),
    );
}

/// Emits an event when a card is lent.
pub fn emit_lend(env: &Env, player: &Address, category: &Category, token_id: &TokenId) {
    env.events().publish(
        (symbol_short!("lend"), symbol_short!("open"), player.clone()),
        (player.clone(), category.clone(), token_id.clone()),
    );
}

/// Emits an event when lending is withdrawn.
pub fn emit_withdraw(env: &Env, player: &Address, category: &Category, token_id: &TokenId) {
    env.events().publish(
        (
            symbol_short!("lend"),
            symbol_short!("close"),
            player.clone(),
        ),
        (player.clone(), category.clone(), token_id.clone()),
    );
}

/// Emits an event when borrowing is made.
pub fn emit_borrow(env: &Env, player: &Address, category: &Category, token_id: &TokenId) {
    env.events().publish(
        (
            symbol_short!("borrow"),
            symbol_short!("open"),
            player.clone(),
        ),
        (player.clone(), category.clone(), token_id.clone()),
    );
}

/// Emits an event when repayment is made.
pub fn emit_repay(env: &Env, player: &Address, category: &Category, token_id: &TokenId) {
    env.events().publish(
        (
            symbol_short!("borrow"),
            symbol_short!("close"),
            player.clone(),
        ),
        (player.clone(), category.clone(), token_id.clone()),
    );
}

/// Emits an event when the liquidation index is updated (lazy pro‑rata)
pub fn emit_index_updated(env: &Env, l_index: u64, d_l: u64, deficit: u64, w_total: u64) {
    env.events().publish(
        (symbol_short!("idx_upd"),),
        (l_index, d_l, deficit, w_total),
    );
}

/// Emits an event when a loan is touched and a haircut is applied
pub fn emit_loan_touched(
    env: &Env,
    player: &Address,
    haircut: u32,
    reserve_left: u32,
    ownership_lost: bool,
) {
    env.events().publish(
        (symbol_short!("loan_tch"), player.clone()),
        (haircut, reserve_left, ownership_lost),
    );
}

/// Emits an event when a loan is fully liquidated
pub fn emit_loan_liquidated(env: &Env, player: &Address) {
    env.events()
        .publish((symbol_short!("loan_liq"), player.clone()), ());
}

/// Emits an event when a card is placed in a deck.
pub fn emit_deck_place(env: &Env, player: &Address) {
    env.events().publish(
        (
            symbol_short!("deck"),
            symbol_short!("place"),
            player.clone(),
        ),
        (),
    );
}

/// Emits an event when a card is replaced in a deck.
pub fn emit_deck_replace(env: &Env, player: &Address) {
    env.events().publish(
        (
            symbol_short!("deck"),
            symbol_short!("replace"),
            player.clone(),
        ),
        (),
    );
}

/// Emits an event when a card is removed from a deck.
pub fn emit_deck_remove(env: &Env, player: &Address) {
    env.events().publish(
        (
            symbol_short!("deck"),
            symbol_short!("remove"),
            player.clone(),
        ),
        (),
    );
}

/// Emits an event when a deck is completed (4 cards).
pub fn emit_deck_completed(env: &Env, player: &Address) {
    env.events().publish(
        (
            symbol_short!("deck"),
            symbol_short!("complete"),
            player.clone(),
        ),
        (),
    );
}

/// Emits an event when a card is minted.
pub fn emit_mint(env: &Env, player: &Address) {
    env.events()
        .publish((symbol_short!("mint"), player.clone()), ());
}

/// Emits an event when a card is transferred.
pub fn emit_transfer(env: &Env, from: &Address, to: &Address) {
    env.events()
        .publish((symbol_short!("transfer"), from.clone(), to.clone()), ());
}
