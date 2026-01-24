use crate::event::{emit_stake, emit_stake_increased, emit_unstake};
use crate::{user_info::mint_terry, *};
use crate::admin::{read_config, read_state, update_balance, update_state};
use nft_info::{read_nft, update_nft, write_nft, Action, Category};
use soroban_sdk::{contracttype, vec, Address, Env, Vec};
use storage_types::{DataKey, TokenId, STORAGE_BUMP_LEDGERS, STORAGE_THRESHOLD_LEDGERS};
use user_info::read_user;

#[contracttype]
#[derive(Clone, PartialEq)]
pub struct Stake {
    pub owner: Address,
    pub category: Category,
    pub token_id: TokenId,
    pub power: u32,
    pub period: u32,
    pub interest_percentage: u32,
    pub staked_time: u32,
}

pub fn write_stake(env: &Env, user: Address, category: Category, token_id: TokenId, stake: Stake) {
    let owner = read_user(&env, user).owner;
    let key = DataKey::Stake(owner.clone(), category.clone(), token_id.clone());
    env.storage().persistent().set(&key, &stake);
    env.storage()
        .persistent()
        .extend_ttl(&key, STORAGE_THRESHOLD_LEDGERS, STORAGE_BUMP_LEDGERS);

    let key = DataKey::Stakes;
    let mut stakes = read_stakes(env.clone());
    if let Some(pos) = stakes.iter().position(|stake| {
        stake.owner == owner && stake.category == category && stake.token_id == token_id
    }) {
        stakes.set(pos.try_into().unwrap(), stake)
    } else {
        stakes.push_back(stake)
    }

    env.storage().persistent().set(&key, &stakes);

    env.storage()
        .persistent()
        .extend_ttl(&key, STORAGE_THRESHOLD_LEDGERS, STORAGE_BUMP_LEDGERS);
}

pub fn read_stakes(env: Env) -> Vec<Stake> {
    let key = DataKey::Stakes;
    if let Some(stakes) = env.storage().persistent().get(&key) {
        env.storage().persistent().extend_ttl(
            &key,
            STORAGE_THRESHOLD_LEDGERS,
            STORAGE_BUMP_LEDGERS,
        );
        stakes
    } else {
        vec![&env.clone()]
    }
}

pub fn remove_stake(env: &Env, user: Address, category: Category, token_id: TokenId) {
    let owner = read_user(&env, user).owner;

    let key = DataKey::Stake(owner.clone(), category.clone(), token_id.clone());
    env.storage().persistent().remove(&key);

    let key = DataKey::Stakes;
    let mut stakes = read_stakes(env.clone());
    if let Some(pos) = stakes.iter().position(|stake| {
        stake.owner == owner && stake.category == category && stake.token_id == token_id
    }) {
        stakes.remove(pos.try_into().unwrap());
    }

    env.storage().persistent().set(&key, &stakes);

    #[cfg(not(test))]
    {
        env.storage().persistent().extend_ttl(
            &key,
            STORAGE_THRESHOLD_LEDGERS,
            STORAGE_BUMP_LEDGERS,
        );
    }
}

pub fn read_stake(env: &Env, user: Address, category: Category, token_id: TokenId) -> Stake {
    let owner = read_user(&env, user).owner;
    let key = DataKey::Stake(owner.clone(), category.clone(), token_id.clone());
    let stake: Stake = env
        .storage()
        .persistent()
        .get(&key)
        .expect("Stake not found");
    #[cfg(not(test))]
    {
        env.storage()
            .persistent()
            .extend_ttl(&key, STORAGE_THRESHOLD_LEDGERS, STORAGE_BUMP_LEDGERS);
    }
    stake
}

pub fn stake(env: Env, user: Address, category: Category, token_id: TokenId, period_index: u32) {
    user.require_auth();
    assert!(
        category == Category::Skill || category == Category::Leader,
        "Invalid Category to stake"
    );
    let owner = read_user(&env, user).owner;

    let nft_read = read_nft(&env, owner.clone(), token_id.clone()).unwrap();
    assert!(nft_read.locked_by_action == Action::None, "Locked NFT");

    let config = read_config(&env);
    // Validate period index bounds to avoid panic
    assert!(
        period_index < config.stake_periods.len(),
        "Invalid period index"
    );
    assert!(
        period_index < config.stake_interest_percentages.len(),
        "Invalid period index"
    );
    let power_fee = config.power_action_fee * nft_read.power / 100;

    let staked_power = nft_read
        .power
        .checked_sub(power_fee)
        .expect("Insufficient POWER for fee");

    // Single Writer Update
    update_nft(&env, owner.clone(), token_id.clone(), |_, card| {
        card.locked_by_action = Action::Stake;
        card.power = 0;
    });

    update_balance(&env, |_, balance| {
        balance.haw_ai_power += power_fee;
    });

    let stake_period = config.stake_periods.get(period_index).unwrap();
    let stake_interest = config.stake_interest_percentages.get(period_index).unwrap();

    write_stake(
        &env,
        owner.clone(),
        category.clone(),
        token_id.clone(),
        Stake {
            owner: owner.clone(),
            category,
            token_id: token_id.clone(),
            power: staked_power,
            period: stake_period,
            interest_percentage: stake_interest,
            staked_time: env
                .ledger()
                .timestamp()
                .try_into()
                .expect("Timestamp exceeds u32 limit"),
        },
    );

    // Emit stake event
    emit_stake(&env, &owner);

    update_state(&env, |_, state| {
        state.total_staked_power += staked_power as u64;
    });
}

pub fn increase_stake_power(
    env: Env,
    user: Address,
    category: Category,
    token_id: TokenId,
    increase_power: u32,
) {
    user.require_auth();
    let owner = read_user(&env, user).owner;

    // Input validation
    assert!(increase_power > 0, "Increase power must be positive");
    assert!(increase_power <= u32::MAX / 2, "Increase power too large");

    let nft_read = read_nft(&env, owner.clone(), token_id.clone()).unwrap();
    assert!(nft_read.locked_by_action == Action::Stake, "Can't find staked");
    assert!(nft_read.power >= increase_power, "Insufficient NFT power");

    let mut stake = read_stake(&env, owner.clone(), category.clone(), token_id.clone())
        ;

    // Safe addition to prevent overflow
    stake.power = stake
        .power
        .checked_add(increase_power)
        .expect("Stake power overflow");

    let config = read_config(&env);
    let power_fee = config
        .power_action_fee
        .checked_mul(increase_power)
        .and_then(|v| v.checked_div(100))
        .expect("Fee calculation overflow");

    // Safe subtraction to prevent underflow
    stake.power = stake
        .power
        .checked_sub(power_fee)
        .expect("Insufficient stake power for fee");

    // Single Writer Update
    update_nft(&env, owner.clone(), token_id.clone(), |_, card| {
        // Safe subtraction to prevent underflow
        card.power = card
            .power
            .checked_sub(increase_power)
            .expect("Insufficient NFT power");
    });

    update_balance(&env, |_, balance| {
        balance.haw_ai_power += power_fee;
    });

    write_stake(
        &env,
        owner.clone(),
        category.clone(),
        token_id.clone(),
        stake,
    );

    // Emit stake increased event
    emit_stake_increased(&env, &owner);

    // Mint terry to user as rewards
    mint_terry(&env, owner, config.terry_per_stake);

    update_balance(&env, |_, balance| {
        balance.haw_ai_terry += config.terry_per_stake * config.haw_ai_percentage as i128 / 100;
    });
}

pub fn unstake(env: Env, user: Address, category: Category, token_id: TokenId) {
    user.require_auth();
    let owner = read_user(&env, user).owner;
    let nft_read = read_nft(&env, owner.clone(), token_id.clone()).unwrap();
    assert!(nft_read.locked_by_action == Action::Stake, "Can't find staked");

    let current_time: u32 = env
        .ledger()
        .timestamp()
        .try_into()
        .expect("Timestamp exceeds u32 limit");

    let stake = read_stake(&env, owner.clone(), category.clone(), token_id.clone())
        ;
    #[cfg(not(test))]
    {
        assert!(
            stake.staked_time + stake.period <= current_time,
            "Locked Period"
        );
    }

    let interest_amount = stake.power * stake.interest_percentage / 100;
    let _staked_power = stake.power;
    
    // Single Writer Update
    update_nft(&env, owner.clone(), token_id.clone(), |_, card| {
        card.power += stake.power + interest_amount;
        card.locked_by_action = Action::None;
    });

    let config = read_config(&env);
    let terry_amount = config.terry_per_power * interest_amount as i128;

    mint_terry(&env, owner.clone(), terry_amount);

    // Emit unstake event
    emit_unstake(&env, &owner);

    remove_stake(&env, owner.clone(), category.clone(), token_id.clone());

    // Mint terry to user as rewards
    mint_terry(&env, owner, config.terry_per_stake);

    update_balance(&env, |_, balance| {
        balance.haw_ai_terry += config.terry_per_stake * config.haw_ai_percentage as i128 / 100;
    });
}
