use crate::admin::read_config;
use crate::event::*;
use crate::pot::management::{
    get_all_rounds, read_player_reward, read_pot_snapshot, write_pending_reward,
};
use crate::storage_types::DataKey;
use crate::storage_types::{
    PendingReward, RewardClaim, RewardStatus, STORAGE_BUMP_LEDGERS,
    STORAGE_THRESHOLD_LEDGERS,
};
use crate::user_info::{read_user, write_user};
use soroban_sdk::{Address, Env, Vec};

const MIN_REWARD_AMOUNT: i128 = 1;
const MIN_XLM_RESERVE: i128 = 500_0000; // 0.5 XLM in stroops

// pub fn verify_trustline(e: &Env, player: &Address, token_id: &Address) -> bool {
//     let token_client = token::Client::new(e, token_id);
//     // Check if the player has a trustline by attempting to get their balance
//     token_client.balance(player) >= 0
// }

// pub fn check_xlm_reserve(e: &Env, player: &Address) -> bool {
//     let native_token = token::Client::new(e, &e.stellar_asset_address("native"));
//     // Check if the player has enough XLM for the trustline reserve
//     native_token.balance(player) >= MIN_XLM_RESERVE
// }

pub fn process_reward(e: &Env, player: &Address, reward: &PendingReward) -> RewardStatus {
    let mut user = read_user(e, player.clone());
    let _config = read_config(e);
    let mut updated = false;
    let final_status = RewardStatus::Claimed;

    if reward.terry_amount > MIN_REWARD_AMOUNT {
        user.terry += reward.terry_amount;
        updated = true;
    }

    if reward.power_amount > 0 {
        user.power += reward.power_amount;
        updated = true;
    }

    // if reward.xtar_amount > MIN_REWARD_AMOUNT {
    //     let xtar_token = token::Client::new(e, &config.xtar_token);
    //     if verify_trustline(e, player, &config.xtar_token) {
    //         xtar_token.transfer(&e.current_contract_address(), player, &reward.xtar_amount);
    //         updated = true;
    //     } else {
    //         final_status = RewardStatus::AwaitingTrustLine;
    //         // Emit RewardPending instead of trustLine_required
    //         emit_reward_pending(e, player, reward);
    //     }
    // }

    if updated {
        write_user(e, player.clone(), user);
    }

    final_status
}

pub fn claim_all_pending_rewards(e: Env, player: Address) {
    player.require_auth();

    let all_rounds = get_all_rounds(&e);
    let mut pending_rewards = Vec::new(&e);
    let timestamp = e.ledger().timestamp();

    for round in all_rounds.iter() {
        if let Some(player_reward) = read_player_reward(&e, round, &player) {
            if let Some(snapshot) = read_pot_snapshot(&e, round) {
                let share = player_reward.share_percentage as i128;
                let reward = PendingReward {
                    round_number: round,
                    terry_amount: (snapshot.total_terry * share) / 10000,
                    power_amount: ((snapshot.total_power as i128 * share) / 10000) as u32,
                    xtar_amount: (snapshot.total_xtar * share) / 10000,
                    generic_tokens: Vec::new(&e), // Initialize empty for legacy fallback
                    status: RewardStatus::Pending,
                };

                // First, atomically mark the reward as being processed to prevent race conditions
                let key = DataKey::PlayerShare(round, player.clone());

                // Check if reward still exists (prevents double-claim)
                if !e.storage().persistent().has(&key) {
                    continue; // Already claimed, skip
                }

                // Remove reward BEFORE processing to prevent double-spending
                e.storage().persistent().remove(&key);

                let status = process_reward(&e, &player, &reward);
                if status == RewardStatus::Claimed {
                    emit_reward_claimed(&e, &player, &reward);
                } else {
                    // If processing failed, write as pending (but not back to PlayerShare)
                    let pending_reward = PendingReward { status, ..reward };
                    write_pending_reward(&e, round, &player, &pending_reward);
                    pending_rewards.push_back(pending_reward);
                }
            }
        }
    }

    if !pending_rewards.is_empty() {
        let claim = RewardClaim {
            player: player.clone(),
            rewards: pending_rewards,
            claimed_at: timestamp,
        };
        e.storage()
            .persistent()
            .set(&DataKey::RewardClaim(player.clone(), timestamp), &claim);
        e.storage().persistent().extend_ttl(
            &DataKey::RewardClaim(player, timestamp),
            STORAGE_THRESHOLD_LEDGERS,
            STORAGE_BUMP_LEDGERS,
        );
    }
}
