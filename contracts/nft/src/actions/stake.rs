use crate::event::{emit_stake, emit_stake_increased, emit_unstake};
use crate::{user_info::mint_terry, *};
use crate::admin::{is_pages_only, read_config, update_balance, update_state};
use nft_info::{read_nft, update_nft, Action, Category};
use soroban_sdk::{contracttype, vec, Address, Env, Vec};
use storage_types::{
    DataKey, PagedListKind, PagedPosKind, StakeKey, TokenId, PAGE_SIZE_STAKES,
    STORAGE_BUMP_LEDGERS, STORAGE_THRESHOLD_LEDGERS,
};
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

    if !is_pages_only(env) {
        // Legacy global list (kept for migration window)
        let key = DataKey::Stakes;
        let mut stakes = read_stakes(env.clone());
        if let Some(pos) = stakes.iter().position(|stake| {
            stake.owner == owner && stake.category == category && stake.token_id == token_id
        }) {
            stakes.set(pos.try_into().unwrap(), stake.clone())
        } else {
            stakes.push_back(stake.clone())
        }
        env.storage().persistent().set(&key, &stakes);
        env.storage()
            .persistent()
            .extend_ttl(&key, STORAGE_THRESHOLD_LEDGERS, STORAGE_BUMP_LEDGERS);
    }

    // New paged global index (IDs only)
    if !env.storage().persistent().has(&DataKey::Pos(
        PagedPosKind::Stakes,
        owner.clone(),
        category.clone(),
        token_id.clone(),
    ))
    {
        add_stake_page_index(
            env,
            StakeKey {
                owner,
                category,
                token_id,
            },
        );
    }
}

pub fn read_stakes(env: Env) -> Vec<Stake> {
    if is_pages_only(&env) {
        let count = env
            .storage()
            .persistent()
            .get::<_, u32>(&DataKey::PagedCount(PagedListKind::Stakes))
            .unwrap_or(0);
        let mut out = vec![&env.clone()];
        let mut idx = 0;
        while idx < count {
            if let Some(key) = read_stake_key_at(&env, idx) {
                let stake_key =
                    DataKey::Stake(key.owner.clone(), key.category.clone(), key.token_id.clone());
                if let Some(stake) = env.storage().persistent().get(&stake_key) {
                    out.push_back(stake);
                }
            }
            idx += 1;
        }
        return out;
    }
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

pub fn read_stakes_count(env: &Env) -> u32 {
    let count = env
        .storage()
        .persistent()
        .get::<_, u32>(&DataKey::PagedCount(PagedListKind::Stakes))
        .unwrap_or(0);
    if count == 0 && !is_pages_only(env) {
        let legacy = read_stakes(env.clone());
        if !legacy.is_empty() {
            return legacy.len();
        }
    }
    count
}

pub fn read_stakes_page(env: &Env, cursor: u32, limit: u32) -> Vec<StakeKey> {
    if limit == 0 {
        return Vec::new(env);
    }
    let count = read_stakes_count(env);
    if !is_pages_only(env)
        && !env
            .storage()
            .persistent()
            .has(&DataKey::PagedCount(PagedListKind::Stakes))
    {
        let legacy = read_stakes(env.clone());
        let total = legacy.len();
        if cursor >= total {
            return Vec::new(env);
        }
        let end = (cursor.saturating_add(limit)).min(total);
        let mut out: Vec<StakeKey> = Vec::new(env);
        let mut idx = cursor;
        while idx < end {
            let stake = legacy.get(idx).unwrap();
            out.push_back(StakeKey {
                owner: stake.owner.clone(),
                category: stake.category.clone(),
                token_id: stake.token_id.clone(),
            });
            idx += 1;
        }
        return out;
    }
    if cursor >= count {
        return Vec::new(env);
    }
    let end = (cursor.saturating_add(limit)).min(count);
    let mut out: Vec<StakeKey> = Vec::new(env);
    let mut idx = cursor;
    while idx < end {
        if let Some(key) = read_stake_key_at(env, idx) {
            out.push_back(key);
        }
        idx += 1;
    }
    out
}

fn read_stake_key_at(env: &Env, idx: u32) -> Option<StakeKey> {
    let page = idx / PAGE_SIZE_STAKES;
    let off = idx % PAGE_SIZE_STAKES;
    let key = DataKey::PagedList(PagedListKind::Stakes, page);
    let page_vec: Vec<StakeKey> = env.storage().persistent().get(&key).unwrap_or(Vec::new(env));
    if off < page_vec.len() {
        Some(page_vec.get(off).unwrap())
    } else {
        None
    }
}

fn write_stakes_count(env: &Env, count: u32) {
    env.storage()
        .persistent()
        .set(&DataKey::PagedCount(PagedListKind::Stakes), &count);
    env.storage().persistent().extend_ttl(
        &DataKey::PagedCount(PagedListKind::Stakes),
        STORAGE_THRESHOLD_LEDGERS,
        STORAGE_BUMP_LEDGERS,
    );
}

fn add_stake_page_index(env: &Env, key: StakeKey) {
    let count = env
        .storage()
        .persistent()
        .get::<_, u32>(&DataKey::PagedCount(PagedListKind::Stakes))
        .unwrap_or(0);
    let page = count / PAGE_SIZE_STAKES;
    let off = count % PAGE_SIZE_STAKES;
    let page_key = DataKey::PagedList(PagedListKind::Stakes, page);
    let mut page_vec: Vec<StakeKey> = env.storage().persistent().get(&page_key).unwrap_or(Vec::new(env));
    if off == page_vec.len() {
        page_vec.push_back(key.clone());
    } else if off < page_vec.len() {
        page_vec.set(off, key.clone());
    } else {
        panic!("PagedList(Stakes) corrupted: offset beyond page length");
    }
    env.storage().persistent().set(&page_key, &page_vec);
    env.storage().persistent().extend_ttl(
        &page_key,
        STORAGE_THRESHOLD_LEDGERS,
        STORAGE_BUMP_LEDGERS,
    );
    env.storage().persistent().set(
        &DataKey::Pos(
            PagedPosKind::Stakes,
            key.owner.clone(),
            key.category.clone(),
            key.token_id.clone(),
        ),
        &count,
    );
    env.storage().persistent().extend_ttl(
        &DataKey::Pos(PagedPosKind::Stakes, key.owner, key.category, key.token_id),
        STORAGE_THRESHOLD_LEDGERS,
        STORAGE_BUMP_LEDGERS,
    );
    write_stakes_count(env, count + 1);
}

fn remove_stake_page_index(env: &Env, owner: Address, category: Category, token_id: TokenId) {
    let pos_key =
        DataKey::Pos(PagedPosKind::Stakes, owner.clone(), category.clone(), token_id.clone());
    let pos = env.storage().persistent().get::<_, u32>(&pos_key);
    if pos.is_none() {
        return;
    }
    let pos = pos.unwrap();
    let count = read_stakes_count(env);
    if count == 0 {
        return;
    }
    let last = count - 1;
    if pos != last {
        if let Some(last_key) = read_stake_key_at(env, last) {
            let dst_page = pos / PAGE_SIZE_STAKES;
            let dst_off = pos % PAGE_SIZE_STAKES;
            let dst_page_key = DataKey::PagedList(PagedListKind::Stakes, dst_page);
            let mut dst_vec: Vec<StakeKey> =
                env.storage().persistent().get(&dst_page_key).unwrap_or(Vec::new(env));
            dst_vec.set(dst_off, last_key.clone());
            env.storage().persistent().set(&dst_page_key, &dst_vec);
            env.storage().persistent().extend_ttl(
                &dst_page_key,
                STORAGE_THRESHOLD_LEDGERS,
                STORAGE_BUMP_LEDGERS,
            );
            env.storage().persistent().set(
                &DataKey::Pos(
                    PagedPosKind::Stakes,
                    last_key.owner.clone(),
                    last_key.category.clone(),
                    last_key.token_id.clone(),
                ),
                &pos,
            );
            env.storage().persistent().extend_ttl(
                &DataKey::Pos(
                    PagedPosKind::Stakes,
                    last_key.owner,
                    last_key.category,
                    last_key.token_id,
                ),
                STORAGE_THRESHOLD_LEDGERS,
                STORAGE_BUMP_LEDGERS,
            );
        }
    }
    let last_page = last / PAGE_SIZE_STAKES;
    let last_off = last % PAGE_SIZE_STAKES;
    let last_page_key = DataKey::PagedList(PagedListKind::Stakes, last_page);
    let mut last_vec: Vec<StakeKey> =
        env.storage().persistent().get(&last_page_key).unwrap_or(Vec::new(env));
    if last_off + 1 == last_vec.len() {
        last_vec.pop_back();
    } else {
        last_vec.remove(last_off.into());
    }
    if last_vec.is_empty() {
        env.storage().persistent().remove(&last_page_key);
    } else {
        env.storage().persistent().set(&last_page_key, &last_vec);
        env.storage().persistent().extend_ttl(
            &last_page_key,
            STORAGE_THRESHOLD_LEDGERS,
            STORAGE_BUMP_LEDGERS,
        );
    }
    env.storage().persistent().remove(&pos_key);
    write_stakes_count(env, count - 1);
}

pub fn migrate_stakes_to_pages(env: &Env, start: u32, limit: u32) -> u32 {
    let stakes = read_stakes(env.clone());
    let total = stakes.len();
    if start >= total {
        return total;
    }
    let end = (start.saturating_add(limit)).min(total);
    let mut idx = start;
    while idx < end {
        let stake = stakes.get(idx).unwrap();
        let pos_key = DataKey::Pos(
            PagedPosKind::Stakes,
            stake.owner.clone(),
            stake.category.clone(),
            stake.token_id.clone(),
        );
        if !env.storage().persistent().has(&pos_key) {
            add_stake_page_index(
                env,
                StakeKey {
                    owner: stake.owner.clone(),
                    category: stake.category.clone(),
                    token_id: stake.token_id.clone(),
                },
            );
        }
        idx += 1;
    }
    end
}

pub fn remove_stake(env: &Env, user: Address, category: Category, token_id: TokenId) {
    let owner = read_user(&env, user).owner;

    let key = DataKey::Stake(owner.clone(), category.clone(), token_id.clone());
    env.storage().persistent().remove(&key);

    if !is_pages_only(env) {
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

    // Remove from paged global index
    remove_stake_page_index(env, owner, category, token_id);
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
            category: category.clone(),
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
    emit_stake(&env, &owner, &category, &token_id);

    // Mint terry to user as rewards
    mint_terry(&env, owner.clone(), config.terry_per_stake);

    update_balance(&env, |_, balance| {
        balance.haw_ai_terry += config.terry_per_stake * config.haw_ai_percentage as i128 / 100;
    });

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
    emit_stake_increased(&env, &owner, &category, &token_id);

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

    let stake = read_stake(&env, owner.clone(), category.clone(), token_id.clone())
        ;
    #[cfg(not(test))]
    {
        let current_time: u32 = env
            .ledger()
            .timestamp()
            .try_into()
            .expect("Timestamp exceeds u32 limit");
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
    emit_unstake(&env, &owner, &category, &token_id);

    remove_stake(&env, owner.clone(), category.clone(), token_id.clone());

    // Mint terry to user as rewards
    mint_terry(&env, owner, config.terry_per_stake);

    update_balance(&env, |_, balance| {
        balance.haw_ai_terry += config.terry_per_stake * config.haw_ai_percentage as i128 / 100;
    });
}
