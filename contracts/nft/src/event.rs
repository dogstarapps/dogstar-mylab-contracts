use crate::nft_info::{Action, Category};
use crate::storage_types::{PendingReward, PlayerReward, PotSnapshot, TokenId};
use soroban_sdk::{contractevent, symbol_short, Address, Env, Symbol};

#[contractevent]
pub struct PotOpen {
    #[topic]
    pub round: u32,
    pub total_terry: i128,
    pub total_power: u32,
    pub total_xtar: i128,
    pub total_participants: u32,
    pub total_effective_power: u32,
}

#[contractevent]
pub struct ShareCal {
    #[topic]
    pub player: Address,
    pub round_number: u32,
    pub share_percentage: u32,
    pub effective_power: u32,
    pub deck_bonus: u32,
    pub deck_categories: u32,
}

#[contractevent]
pub struct FeeAcc {
    pub terry: i128,
    pub power: u32,
    pub xtar: i128,
    pub fee_percentage: u32,
    pub from: Option<Address>,
    pub action: Action,
}

#[contractevent]
pub struct FeeWd {
    #[topic]
    pub recipient: Address,
    pub terry: i128,
    pub power: u32,
    pub xtar: i128,
}

#[contractevent]
pub struct FeePct {
    pub old_fee: u32,
    pub new_fee: u32,
}

#[contractevent]
pub struct RwdCld {
    #[topic]
    pub player: Address,
    #[topic]
    pub round_number: u32,
    pub terry_amount: i128,
    pub power_amount: u32,
    pub xtar_amount: i128,
}

#[contractevent]
pub struct RewardPd {
    #[topic]
    pub player: Address,
    #[topic]
    pub round_number: u32,
    pub terry_amount: i128,
    pub power_amount: u32,
    pub xtar_amount: i128,
}

#[contractevent]
pub struct RwdClaim {
    #[topic]
    pub player: Address,
    pub terry: i128,
    pub power: u32,
    pub xtar: i128,
}

#[contractevent]
pub struct Burn {
    #[topic]
    pub player: Address,
}

#[contractevent]
pub struct Stake {
    #[topic]
    pub action: Symbol,
    #[topic]
    pub player: Address,
    pub owner: Address,
    pub category: Category,
    pub token_id: TokenId,
}

#[contractevent]
pub struct Lend {
    #[topic]
    pub action: Symbol,
    #[topic]
    pub player: Address,
    pub owner: Address,
    pub category: Category,
    pub token_id: TokenId,
}

#[contractevent]
pub struct Borrow {
    #[topic]
    pub action: Symbol,
    #[topic]
    pub player: Address,
    pub owner: Address,
    pub category: Category,
    pub token_id: TokenId,
}

#[contractevent]
pub struct IdxUpd {
    pub l_index: u64,
    pub d_l: u64,
    pub deficit: u64,
    pub w_total: u64,
}

#[contractevent]
pub struct LoanTch {
    #[topic]
    pub player: Address,
    pub haircut: u32,
    pub reserve_left: u32,
    pub ownership_lost: bool,
}

#[contractevent]
pub struct LoanLiq {
    #[topic]
    pub player: Address,
}

#[contractevent]
pub struct Deck {
    #[topic]
    pub action: Symbol,
    #[topic]
    pub player: Address,
}

#[contractevent]
pub struct Mint {
    #[topic]
    pub player: Address,
}

#[contractevent]
pub struct Transfer {
    #[topic]
    pub from: Address,
    #[topic]
    pub to: Address,
}

#[contractevent]
pub struct Fight {
    #[topic]
    pub action: Symbol,
    pub fight: crate::actions::fight::Fight,
}

#[contractevent]
pub struct State {
    pub state: crate::storage_types::State,
}

// Event Emission
/// Emits an event when the pot is opened.
pub fn emit_pot_opened(env: &Env, round: u32, snapshot: &PotSnapshot) {
    PotOpen {
        round,
        total_terry: snapshot.total_terry,
        total_power: snapshot.total_power,
        total_xtar: snapshot.total_xtar,
        total_participants: snapshot.total_participants,
        total_effective_power: snapshot.total_effective_power,
    }
    .publish(env);
}

/// Emits an event when a player's share is calculated.
pub fn emit_share_calculated(env: &Env, player: &Address, reward: &PlayerReward) {
    ShareCal {
        player: player.clone(),
        round_number: reward.round_number,
        share_percentage: reward.share_percentage,
        effective_power: reward.effective_power,
        deck_bonus: reward.deck_bonus,
        deck_categories: reward.deck_categories,
    }
    .publish(env);
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
    FeeAcc {
        terry,
        power,
        xtar,
        fee_percentage,
        from,
        action: action_val,
    }
    .publish(env);
}

/// Emits an event when Dogstar fees are withdrawn.
pub fn emit_dogstar_fee_withdrawn(
    env: &Env,
    recipient: &Address,
    terry: i128,
    power: u32,
    xtar: i128,
) {
    FeeWd {
        recipient: recipient.clone(),
        terry,
        power,
        xtar,
    }
    .publish(env);
}

/// Emits an event when the Dogstar fee percentage is updated.
pub fn emit_dogstar_fee_percentage_updated(env: &Env, old_fee: u32, new_fee: u32) {
    FeePct { old_fee, new_fee }.publish(env);
}

/// Emits an event when a reward is claimed.
#[allow(dead_code)]
pub fn emit_reward_claimed(e: &Env, player: &Address, reward: &PendingReward) {
    RwdCld {
        player: player.clone(),
        round_number: reward.round_number,
        terry_amount: reward.terry_amount,
        power_amount: reward.power_amount,
        xtar_amount: reward.xtar_amount,
    }
    .publish(e);
}

/// Emits an event when a reward is marked as pending due to missing trustline.
#[allow(dead_code)]
pub fn emit_reward_pending(e: &Env, player: &Address, reward: &PendingReward) {
    RewardPd {
        player: player.clone(),
        round_number: reward.round_number,
        terry_amount: reward.terry_amount,
        power_amount: reward.power_amount,
        xtar_amount: reward.xtar_amount,
    }
    .publish(e);
}

/// Emits an event when rewards are claimed from HAW AI pot.
pub fn emit_rewards_claimed(e: &Env, player: &Address, terry: i128, power: u32, xtar: i128) {
    RwdClaim {
        player: player.clone(),
        terry,
        power,
        xtar,
    }
    .publish(e);
}

/// Emits an event when a card is burned.
pub fn emit_burn(env: &Env, player: &Address) {
    Burn {
        player: player.clone(),
    }
    .publish(env);
}

/// Emits an event when a card is staked.
pub fn emit_stake(env: &Env, player: &Address, category: &Category, token_id: &TokenId) {
    Stake {
        action: symbol_short!("open"),
        player: player.clone(),
        owner: player.clone(),
        category: category.clone(),
        token_id: token_id.clone(),
    }
    .publish(env);
}

/// Emits an event when stake power is increased.
pub fn emit_stake_increased(
    env: &Env,
    player: &Address,
    category: &Category,
    token_id: &TokenId,
) {
    Stake {
        action: symbol_short!("increase"),
        player: player.clone(),
        owner: player.clone(),
        category: category.clone(),
        token_id: token_id.clone(),
    }
    .publish(env);
}

/// Emits an event when a card is unstaked.
pub fn emit_unstake(env: &Env, player: &Address, category: &Category, token_id: &TokenId) {
    Stake {
        action: symbol_short!("close"),
        player: player.clone(),
        owner: player.clone(),
        category: category.clone(),
        token_id: token_id.clone(),
    }
    .publish(env);
}

/// Emits an event when a card is lent.
pub fn emit_lend(env: &Env, player: &Address, category: &Category, token_id: &TokenId) {
    Lend {
        action: symbol_short!("open"),
        player: player.clone(),
        owner: player.clone(),
        category: category.clone(),
        token_id: token_id.clone(),
    }
    .publish(env);
}

/// Emits an event when lending is withdrawn.
pub fn emit_withdraw(env: &Env, player: &Address, category: &Category, token_id: &TokenId) {
    Lend {
        action: symbol_short!("close"),
        player: player.clone(),
        owner: player.clone(),
        category: category.clone(),
        token_id: token_id.clone(),
    }
    .publish(env);
}

/// Emits an event when borrowing is made.
pub fn emit_borrow(env: &Env, player: &Address, category: &Category, token_id: &TokenId) {
    Borrow {
        action: symbol_short!("open"),
        player: player.clone(),
        owner: player.clone(),
        category: category.clone(),
        token_id: token_id.clone(),
    }
    .publish(env);
}

/// Emits an event when repayment is made.
pub fn emit_repay(env: &Env, player: &Address, category: &Category, token_id: &TokenId) {
    Borrow {
        action: symbol_short!("close"),
        player: player.clone(),
        owner: player.clone(),
        category: category.clone(),
        token_id: token_id.clone(),
    }
    .publish(env);
}

/// Emits an event when the liquidation index is updated (lazy pro‑rata)
pub fn emit_index_updated(env: &Env, l_index: u64, d_l: u64, deficit: u64, w_total: u64) {
    IdxUpd {
        l_index,
        d_l,
        deficit,
        w_total,
    }
    .publish(env);
}

/// Emits an event when a loan is touched and a haircut is applied
pub fn emit_loan_touched(
    env: &Env,
    player: &Address,
    haircut: u32,
    reserve_left: u32,
    ownership_lost: bool,
) {
    LoanTch {
        player: player.clone(),
        haircut,
        reserve_left,
        ownership_lost,
    }
    .publish(env);
}

/// Emits an event when a loan is fully liquidated
pub fn emit_loan_liquidated(env: &Env, player: &Address) {
    LoanLiq {
        player: player.clone(),
    }
    .publish(env);
}

/// Emits an event when a card is placed in a deck.
pub fn emit_deck_place(env: &Env, player: &Address) {
    Deck {
        action: symbol_short!("place"),
        player: player.clone(),
    }
    .publish(env);
}

/// Emits an event when a card is replaced in a deck.
pub fn emit_deck_replace(env: &Env, player: &Address) {
    Deck {
        action: symbol_short!("replace"),
        player: player.clone(),
    }
    .publish(env);
}

/// Emits an event when a card is removed from a deck.
pub fn emit_deck_remove(env: &Env, player: &Address) {
    Deck {
        action: symbol_short!("remove"),
        player: player.clone(),
    }
    .publish(env);
}

/// Emits an event when a deck is completed (4 cards).
pub fn emit_deck_completed(env: &Env, player: &Address) {
    Deck {
        action: symbol_short!("complete"),
        player: player.clone(),
    }
    .publish(env);
}

/// Emits an event when a card is minted.
pub fn emit_mint(env: &Env, player: &Address) {
    Mint {
        player: player.clone(),
    }
    .publish(env);
}

/// Emits an event when a card is transferred.
pub fn emit_transfer(env: &Env, from: &Address, to: &Address) {
    Transfer {
        from: from.clone(),
        to: to.clone(),
    }
    .publish(env);
}

/// Emits an event when a fight is opened.
pub fn emit_fight_open(env: &Env, fight: &crate::actions::fight::Fight) {
    Fight {
        action: symbol_short!("open"),
        fight: fight.clone(),
    }
    .publish(env);
}

/// Emits an event when a fight is closed.
pub fn emit_fight_close(env: &Env, fight: &crate::actions::fight::Fight) {
    Fight {
        action: symbol_short!("close"),
        fight: fight.clone(),
    }
    .publish(env);
}

/// Emits an event when the admin state is updated.
pub fn emit_state_updated(env: &Env, state: &crate::storage_types::State) {
    State { state: state.clone() }.publish(env);
}
