use crate::event::{
    emit_borrow, emit_index_updated, emit_lend, emit_loan_liquidated, emit_loan_touched,
    emit_repay, emit_withdraw,
};
use crate::{
    admin::{read_state, update_state},
    user_info::mint_terry,
    *,
};
use admin::{is_pages_only, read_balance, read_config, update_balance};
use nft_info::{read_nft, update_nft, write_nft, Action, Category};
use soroban_sdk::{contracttype, vec, Address, Env, Vec};
use storage_types::{
    BorrowMeta, BorrowingKey, DataKey, LendingKey, PagedListKind, PagedPosKind, TokenId,
    PAGE_SIZE_BORROWINGS, PAGE_SIZE_LENDINGS, STORAGE_BUMP_LEDGERS, STORAGE_THRESHOLD_LEDGERS,
};
use user_info::{read_user, update_user};

const SCALE: u64 = 1_000_000; // 6-decimal fixed point
const APY_MIN: u64 = 0; // 0% APY
const APY_MAX: u64 = 300_000; // 30% APY = 0.30 * SCALE
const T_MAX_FP: u64 = 500_000; // 0.5 years in SCALE for reserve horizon

#[contracttype]
#[derive(Clone, PartialEq)]
pub struct Lending {
    pub lender: Address,
    pub category: Category,
    pub token_id: TokenId,
    pub power: u32,
    pub lent_at: u64,
}

#[contracttype]
#[derive(Clone, PartialEq)]
pub struct Borrowing {
    pub borrower: Address,
    pub category: Category,
    pub token_id: TokenId,
    pub power: u32,
    pub borrowed_at: u64,
}

#[contracttype]
#[derive(Clone, PartialEq)]
pub struct BorrowQuote {
    pub allowed: bool,
    pub reason: u32, // 0=OK,1=Zero,2=InsufficientPool,3=ExceedsCollateral,4=InvalidHorizon
    pub apy: u64,
    pub fee: u32,
    pub reserve: u64,
    pub buffer: u64,
    pub borrow_net: u32,
    pub max_suggested_gross: u32,
}

#[contracttype]
#[derive(Clone, PartialEq)]
pub struct RepayQuote {
    pub required_power: u32,
    pub user_power: u32,
    pub collateral_power: u32,
    pub total_available: u32,
    pub sufficient: bool,
}

pub fn write_lending(
    env: Env,
    user: Address,
    category: Category,
    token_id: TokenId,
    lending: Lending,
) {
    let owner = read_user(&env, user).owner;

    let key = DataKey::Lending(owner.clone(), category.clone(), token_id.clone());
    env.storage().persistent().set(&key, &lending);
    env.storage()
        .persistent()
        .extend_ttl(&key, STORAGE_THRESHOLD_LEDGERS, STORAGE_BUMP_LEDGERS);

    if !is_pages_only(&env) {
        let key = DataKey::Lendings;
        let mut lendings = read_lendings(env.clone());
        if let Some(pos) = lendings.iter().position(|lending| {
            lending.lender == owner && lending.category == category && lending.token_id == token_id
        }) {
            lendings.set(pos.try_into().unwrap(), lending)
        } else {
            lendings.push_back(lending)
        }

        env.storage().persistent().set(&key, &lendings);

        env.storage()
            .persistent()
            .extend_ttl(&key, STORAGE_THRESHOLD_LEDGERS, STORAGE_BUMP_LEDGERS);
    }

    // New paged global index (IDs only)
    if !env.storage().persistent().has(&DataKey::Pos(
        PagedPosKind::Lendings,
        owner.clone(),
        category.clone(),
        token_id.clone(),
    ))
    {
        add_lending_page_index(
            &env,
            LendingKey {
                owner,
                category,
                token_id,
            },
        );
    }
}

pub fn read_lending(env: Env, user: Address, category: Category, token_id: TokenId) -> Lending {
    let owner = read_user(&env, user).owner;

    let key = DataKey::Lending(owner.clone(), category.clone(), token_id.clone());
    let lending: Lending = env
        .storage()
        .persistent()
        .get(&key)
        .expect("Lending not found");
    env.storage()
        .persistent()
        .extend_ttl(&key, STORAGE_THRESHOLD_LEDGERS, STORAGE_BUMP_LEDGERS);
    lending
}

pub fn remove_lending(env: Env, user: Address, category: Category, token_id: TokenId) {
    let owner = read_user(&env, user).owner;

    let key = DataKey::Lending(owner.clone(), category.clone(), token_id.clone());
    env.storage().persistent().remove(&key);

    if !is_pages_only(&env) {
        let key = DataKey::Lendings;
        let mut lendings = read_lendings(env.clone());
        if let Some(pos) = lendings.iter().position(|lending| {
            lending.lender == owner && lending.category == category && lending.token_id == token_id
        }) {
            lendings.remove(pos.try_into().unwrap());
        }

        env.storage().persistent().set(&key, &lendings);

        env.storage()
            .persistent()
            .extend_ttl(&key, STORAGE_THRESHOLD_LEDGERS, STORAGE_BUMP_LEDGERS);
    }

    // Remove from paged global index
    remove_lending_page_index(&env, owner, category, token_id);
}

pub fn read_lendings(env: Env) -> Vec<Lending> {
    if is_pages_only(&env) {
        let count = env
            .storage()
            .persistent()
            .get::<_, u32>(&DataKey::PagedCount(PagedListKind::Lendings))
            .unwrap_or(0);
        let mut out = vec![&env.clone()];
        let mut idx = 0;
        while idx < count {
            if let Some(key) = read_lending_key_at(&env, idx) {
                let lending_key =
                    DataKey::Lending(key.owner.clone(), key.category.clone(), key.token_id.clone());
                if let Some(lending) = env.storage().persistent().get(&lending_key) {
                    out.push_back(lending);
                }
            }
            idx += 1;
        }
        return out;
    }
    let key = DataKey::Lendings;
    if let Some(lendings) = env.storage().persistent().get(&key) {
        env.storage().persistent().extend_ttl(
            &key,
            STORAGE_THRESHOLD_LEDGERS,
            STORAGE_BUMP_LEDGERS,
        );
        lendings
    } else {
        vec![&env.clone()]
    }
}

pub fn read_lendings_count(env: &Env) -> u32 {
    let count = env
        .storage()
        .persistent()
        .get::<_, u32>(&DataKey::PagedCount(PagedListKind::Lendings))
        .unwrap_or(0);
    if count == 0 && !is_pages_only(env) {
        let legacy = read_lendings(env.clone());
        if !legacy.is_empty() {
            return legacy.len();
        }
    }
    count
}

pub fn read_lendings_page(env: &Env, cursor: u32, limit: u32) -> Vec<LendingKey> {
    if limit == 0 {
        return Vec::new(env);
    }
    let count = read_lendings_count(env);
    if !is_pages_only(env)
        && !env
            .storage()
            .persistent()
            .has(&DataKey::PagedCount(PagedListKind::Lendings))
    {
        let legacy = read_lendings(env.clone());
        let total = legacy.len();
        if cursor >= total {
            return Vec::new(env);
        }
        let end = (cursor.saturating_add(limit)).min(total);
        let mut out: Vec<LendingKey> = Vec::new(env);
        let mut idx = cursor;
        while idx < end {
            let lending = legacy.get(idx).unwrap();
            out.push_back(LendingKey {
                owner: lending.lender.clone(),
                category: lending.category.clone(),
                token_id: lending.token_id.clone(),
            });
            idx += 1;
        }
        return out;
    }
    if cursor >= count {
        return Vec::new(env);
    }
    let end = (cursor.saturating_add(limit)).min(count);
    let mut out: Vec<LendingKey> = Vec::new(env);
    let mut idx = cursor;
    while idx < end {
        if let Some(key) = read_lending_key_at(env, idx) {
            out.push_back(key);
        }
        idx += 1;
    }
    out
}

fn read_lending_key_at(env: &Env, idx: u32) -> Option<LendingKey> {
    let page = idx / PAGE_SIZE_LENDINGS;
    let off = idx % PAGE_SIZE_LENDINGS;
    let key = DataKey::PagedList(PagedListKind::Lendings, page);
    let page_vec: Vec<LendingKey> = env.storage().persistent().get(&key).unwrap_or(Vec::new(env));
    if off < page_vec.len() {
        Some(page_vec.get(off).unwrap())
    } else {
        None
    }
}

fn write_lendings_count(env: &Env, count: u32) {
    env.storage()
        .persistent()
        .set(&DataKey::PagedCount(PagedListKind::Lendings), &count);
    env.storage().persistent().extend_ttl(
        &DataKey::PagedCount(PagedListKind::Lendings),
        STORAGE_THRESHOLD_LEDGERS,
        STORAGE_BUMP_LEDGERS,
    );
}

fn add_lending_page_index(env: &Env, key: LendingKey) {
    let count = env
        .storage()
        .persistent()
        .get::<_, u32>(&DataKey::PagedCount(PagedListKind::Lendings))
        .unwrap_or(0);
    let page = count / PAGE_SIZE_LENDINGS;
    let off = count % PAGE_SIZE_LENDINGS;
    let page_key = DataKey::PagedList(PagedListKind::Lendings, page);
    let mut page_vec: Vec<LendingKey> =
        env.storage().persistent().get(&page_key).unwrap_or(Vec::new(env));
    if off == page_vec.len() {
        page_vec.push_back(key.clone());
    } else if off < page_vec.len() {
        page_vec.set(off, key.clone());
    } else {
        panic!("PagedList(Lendings) corrupted: offset beyond page length");
    }
    env.storage().persistent().set(&page_key, &page_vec);
    env.storage().persistent().extend_ttl(
        &page_key,
        STORAGE_THRESHOLD_LEDGERS,
        STORAGE_BUMP_LEDGERS,
    );
    env.storage().persistent().set(
        &DataKey::Pos(
            PagedPosKind::Lendings,
            key.owner.clone(),
            key.category.clone(),
            key.token_id.clone(),
        ),
        &count,
    );
    env.storage().persistent().extend_ttl(
        &DataKey::Pos(PagedPosKind::Lendings, key.owner, key.category, key.token_id),
        STORAGE_THRESHOLD_LEDGERS,
        STORAGE_BUMP_LEDGERS,
    );
    write_lendings_count(env, count + 1);
}

fn remove_lending_page_index(env: &Env, owner: Address, category: Category, token_id: TokenId) {
    let pos_key =
        DataKey::Pos(PagedPosKind::Lendings, owner.clone(), category.clone(), token_id.clone());
    let pos = env.storage().persistent().get::<_, u32>(&pos_key);
    if pos.is_none() {
        return;
    }
    let pos = pos.unwrap();
    let count = read_lendings_count(env);
    if count == 0 {
        return;
    }
    let last = count - 1;
    if pos != last {
        if let Some(last_key) = read_lending_key_at(env, last) {
            let dst_page = pos / PAGE_SIZE_LENDINGS;
            let dst_off = pos % PAGE_SIZE_LENDINGS;
            let dst_page_key = DataKey::PagedList(PagedListKind::Lendings, dst_page);
            let mut dst_vec: Vec<LendingKey> =
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
                    PagedPosKind::Lendings,
                    last_key.owner.clone(),
                    last_key.category.clone(),
                    last_key.token_id.clone(),
                ),
                &pos,
            );
            env.storage().persistent().extend_ttl(
                &DataKey::Pos(
                    PagedPosKind::Lendings,
                    last_key.owner,
                    last_key.category,
                    last_key.token_id,
                ),
                STORAGE_THRESHOLD_LEDGERS,
                STORAGE_BUMP_LEDGERS,
            );
        }
    }
    let last_page = last / PAGE_SIZE_LENDINGS;
    let last_off = last % PAGE_SIZE_LENDINGS;
    let last_page_key = DataKey::PagedList(PagedListKind::Lendings, last_page);
    let mut last_vec: Vec<LendingKey> =
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
    write_lendings_count(env, count - 1);
}

pub fn write_borrowing(
    env: Env,
    user: Address,
    category: Category,
    token_id: TokenId,
    borrowing: Borrowing,
) {
    let owner = read_user(&env, user).owner;

    let key = DataKey::Borrowing(owner.clone(), category.clone(), token_id.clone());
    env.storage().persistent().set(&key, &borrowing);
    env.storage()
        .persistent()
        .extend_ttl(&key, STORAGE_THRESHOLD_LEDGERS, STORAGE_BUMP_LEDGERS);

    if !is_pages_only(&env) {
        let key = DataKey::Borrowings;
        let mut borrowings = read_borrowings(env.clone());
        if let Some(pos) = borrowings.iter().position(|borrowing| {
            borrowing.borrower == owner
                && borrowing.category == category
                && borrowing.token_id == token_id
        }) {
            borrowings.set(pos.try_into().unwrap(), borrowing.clone())
        } else {
            borrowings.push_back(borrowing.clone())
        }

        env.storage().persistent().set(&key, &borrowings);

        env.storage()
            .persistent()
            .extend_ttl(&key, STORAGE_THRESHOLD_LEDGERS, STORAGE_BUMP_LEDGERS);
    }

    // New paged global index (IDs only)
    if !env.storage().persistent().has(&DataKey::Pos(
        PagedPosKind::Borrowings,
        owner.clone(),
        category.clone(),
        token_id.clone(),
    ))
    {
        add_borrowing_page_index(
            &env,
            BorrowingKey {
                owner,
                category,
                token_id,
            },
        );
    }
}

pub fn read_borrowing(env: Env, user: Address, category: Category, token_id: TokenId) -> Borrowing {
    let owner = read_user(&env, user).owner;

    let key = DataKey::Borrowing(owner.clone(), category.clone(), token_id.clone());
    let borrowing: Borrowing = env
        .storage()
        .persistent()
        .get(&key)
        .expect("Borrowing not found");
    env.storage()
        .persistent()
        .extend_ttl(&key, STORAGE_THRESHOLD_LEDGERS, STORAGE_BUMP_LEDGERS);
    borrowing
}

pub fn remove_borrowing(env: Env, user: Address, category: Category, token_id: TokenId) {
    let owner = read_user(&env, user.clone()).owner;

    if !is_pages_only(&env) {
        let key = DataKey::Borrowings;
        let mut borrowings = read_borrowings(env.clone());
        if let Some(pos) = borrowings.iter().position(|borrowing| {
            borrowing.borrower == owner
                && borrowing.category == category
                && borrowing.token_id == token_id
        }) {
            borrowings.remove(pos.try_into().unwrap());
        }

        env.storage().persistent().set(&key, &borrowings);

        env.storage()
            .persistent()
            .extend_ttl(&key, STORAGE_THRESHOLD_LEDGERS, STORAGE_BUMP_LEDGERS);
    }

    let key = DataKey::Borrowing(owner.clone(), category.clone(), token_id.clone());
    env.storage().persistent().remove(&key);

    // Remove from paged global index
    remove_borrowing_page_index(&env, owner, category, token_id);
}

pub fn read_borrowings(env: Env) -> Vec<Borrowing> {
    if is_pages_only(&env) {
        let count = env
            .storage()
            .persistent()
            .get::<_, u32>(&DataKey::PagedCount(PagedListKind::Borrowings))
            .unwrap_or(0);
        let mut out = vec![&env.clone()];
        let mut idx = 0;
        while idx < count {
            if let Some(key) = read_borrowing_key_at(&env, idx) {
                let borrowing_key = DataKey::Borrowing(
                    key.owner.clone(),
                    key.category.clone(),
                    key.token_id.clone(),
                );
                if let Some(borrowing) = env.storage().persistent().get(&borrowing_key) {
                    out.push_back(borrowing);
                }
            }
            idx += 1;
        }
        return out;
    }
    let key = DataKey::Borrowings;
    if let Some(borrowings) = env.storage().persistent().get(&key) {
        env.storage().persistent().extend_ttl(
            &key,
            STORAGE_THRESHOLD_LEDGERS,
            STORAGE_BUMP_LEDGERS,
        );
        borrowings
    } else {
        vec![&env.clone()]
    }
}

pub fn read_borrowings_count(env: &Env) -> u32 {
    let count = env
        .storage()
        .persistent()
        .get::<_, u32>(&DataKey::PagedCount(PagedListKind::Borrowings))
        .unwrap_or(0);
    if count == 0 && !is_pages_only(env) {
        let legacy = read_borrowings(env.clone());
        if !legacy.is_empty() {
            return legacy.len();
        }
    }
    count
}

pub fn read_borrowings_page(env: &Env, cursor: u32, limit: u32) -> Vec<BorrowingKey> {
    if limit == 0 {
        return Vec::new(env);
    }
    let count = read_borrowings_count(env);
    if !is_pages_only(env)
        && !env
            .storage()
            .persistent()
            .has(&DataKey::PagedCount(PagedListKind::Borrowings))
    {
        let legacy = read_borrowings(env.clone());
        let total = legacy.len();
        if cursor >= total {
            return Vec::new(env);
        }
        let end = (cursor.saturating_add(limit)).min(total);
        let mut out: Vec<BorrowingKey> = Vec::new(env);
        let mut idx = cursor;
        while idx < end {
            let borrowing = legacy.get(idx).unwrap();
            out.push_back(BorrowingKey {
                owner: borrowing.borrower.clone(),
                category: borrowing.category.clone(),
                token_id: borrowing.token_id.clone(),
            });
            idx += 1;
        }
        return out;
    }
    if cursor >= count {
        return Vec::new(env);
    }
    let end = (cursor.saturating_add(limit)).min(count);
    let mut out: Vec<BorrowingKey> = Vec::new(env);
    let mut idx = cursor;
    while idx < end {
        if let Some(key) = read_borrowing_key_at(env, idx) {
            out.push_back(key);
        }
        idx += 1;
    }
    out
}

fn read_borrowing_key_at(env: &Env, idx: u32) -> Option<BorrowingKey> {
    let page = idx / PAGE_SIZE_BORROWINGS;
    let off = idx % PAGE_SIZE_BORROWINGS;
    let key = DataKey::PagedList(PagedListKind::Borrowings, page);
    let page_vec: Vec<BorrowingKey> =
        env.storage().persistent().get(&key).unwrap_or(Vec::new(env));
    if off < page_vec.len() {
        Some(page_vec.get(off).unwrap())
    } else {
        None
    }
}

fn write_borrowings_count(env: &Env, count: u32) {
    env.storage()
        .persistent()
        .set(&DataKey::PagedCount(PagedListKind::Borrowings), &count);
    env.storage().persistent().extend_ttl(
        &DataKey::PagedCount(PagedListKind::Borrowings),
        STORAGE_THRESHOLD_LEDGERS,
        STORAGE_BUMP_LEDGERS,
    );
}

fn add_borrowing_page_index(env: &Env, key: BorrowingKey) {
    let count = env
        .storage()
        .persistent()
        .get::<_, u32>(&DataKey::PagedCount(PagedListKind::Borrowings))
        .unwrap_or(0);
    let page = count / PAGE_SIZE_BORROWINGS;
    let off = count % PAGE_SIZE_BORROWINGS;
    let page_key = DataKey::PagedList(PagedListKind::Borrowings, page);
    let mut page_vec: Vec<BorrowingKey> =
        env.storage().persistent().get(&page_key).unwrap_or(Vec::new(env));
    if off == page_vec.len() {
        page_vec.push_back(key.clone());
    } else if off < page_vec.len() {
        page_vec.set(off, key.clone());
    } else {
        panic!("PagedList(Borrowings) corrupted: offset beyond page length");
    }
    env.storage().persistent().set(&page_key, &page_vec);
    env.storage().persistent().extend_ttl(
        &page_key,
        STORAGE_THRESHOLD_LEDGERS,
        STORAGE_BUMP_LEDGERS,
    );
    env.storage().persistent().set(
        &DataKey::Pos(
            PagedPosKind::Borrowings,
            key.owner.clone(),
            key.category.clone(),
            key.token_id.clone(),
        ),
        &count,
    );
    env.storage().persistent().extend_ttl(
        &DataKey::Pos(
            PagedPosKind::Borrowings,
            key.owner,
            key.category,
            key.token_id,
        ),
        STORAGE_THRESHOLD_LEDGERS,
        STORAGE_BUMP_LEDGERS,
    );
    write_borrowings_count(env, count + 1);
}

fn remove_borrowing_page_index(env: &Env, owner: Address, category: Category, token_id: TokenId) {
    let pos_key = DataKey::Pos(
        PagedPosKind::Borrowings,
        owner.clone(),
        category.clone(),
        token_id.clone(),
    );
    let pos = env.storage().persistent().get::<_, u32>(&pos_key);
    if pos.is_none() {
        return;
    }
    let pos = pos.unwrap();
    let count = read_borrowings_count(env);
    if count == 0 {
        return;
    }
    let last = count - 1;
    if pos != last {
        if let Some(last_key) = read_borrowing_key_at(env, last) {
            let dst_page = pos / PAGE_SIZE_BORROWINGS;
            let dst_off = pos % PAGE_SIZE_BORROWINGS;
            let dst_page_key = DataKey::PagedList(PagedListKind::Borrowings, dst_page);
            let mut dst_vec: Vec<BorrowingKey> =
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
                    PagedPosKind::Borrowings,
                    last_key.owner.clone(),
                    last_key.category.clone(),
                    last_key.token_id.clone(),
                ),
                &pos,
            );
            env.storage().persistent().extend_ttl(
                &DataKey::Pos(
                    PagedPosKind::Borrowings,
                    last_key.owner,
                    last_key.category,
                    last_key.token_id,
                ),
                STORAGE_THRESHOLD_LEDGERS,
                STORAGE_BUMP_LEDGERS,
            );
        }
    }
    let last_page = last / PAGE_SIZE_BORROWINGS;
    let last_off = last % PAGE_SIZE_BORROWINGS;
    let last_page_key = DataKey::PagedList(PagedListKind::Borrowings, last_page);
    let mut last_vec: Vec<BorrowingKey> =
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
    write_borrowings_count(env, count - 1);
}

pub fn migrate_lendings_to_pages(env: &Env, start: u32, limit: u32) -> u32 {
    let lendings = read_lendings(env.clone());
    let total = lendings.len();
    if start >= total {
        return total;
    }
    let end = (start.saturating_add(limit)).min(total);
    let mut idx = start;
    while idx < end {
        let lending = lendings.get(idx).unwrap();
        let pos_key = DataKey::Pos(
            PagedPosKind::Lendings,
            lending.lender.clone(),
            lending.category.clone(),
            lending.token_id.clone(),
        );
        if !env.storage().persistent().has(&pos_key) {
            add_lending_page_index(
                env,
                LendingKey {
                    owner: lending.lender.clone(),
                    category: lending.category.clone(),
                    token_id: lending.token_id.clone(),
                },
            );
        }
        idx += 1;
    }
    end
}

pub fn migrate_borrowings_to_pages(env: &Env, start: u32, limit: u32) -> u32 {
    let borrowings = read_borrowings(env.clone());
    let total = borrowings.len();
    if start >= total {
        return total;
    }
    let end = (start.saturating_add(limit)).min(total);
    let mut idx = start;
    while idx < end {
        let borrowing = borrowings.get(idx).unwrap();
        let pos_key = DataKey::Pos(
            PagedPosKind::Borrowings,
            borrowing.borrower.clone(),
            borrowing.category.clone(),
            borrowing.token_id.clone(),
        );
        if !env.storage().persistent().has(&pos_key) {
            add_borrowing_page_index(
                env,
                BorrowingKey {
                    owner: borrowing.borrower.clone(),
                    category: borrowing.category.clone(),
                    token_id: borrowing.token_id.clone(),
                },
            );
        }
        idx += 1;
    }
    end
}

pub fn calculate_apy(
    total_borrowed_power: u64,
    total_offer: u64,
    loans_time_seconds: u64,
    active_loans: u64,
    alpha: u64,
) -> u64 {
    // Utilization U = B / (B + S + eps)
    let denom = (total_borrowed_power as u128)
        .saturating_add(total_offer as u128)
        .saturating_add(1); // epsilon to avoid div-by-zero
    let mut u_fp = ((total_borrowed_power as u128) * (SCALE as u128)) / denom;
    if u_fp > SCALE as u128 {
        u_fp = SCALE as u128;
    }

    // Average loan time in years with a floor T_floor_seconds
    const SECONDS_PER_YEAR: u64 = 31_536_000;
    const T_FLOOR_SECONDS: u64 = 3_600; // 1 hour
    let n = active_loans.max(1);
    let avg_seconds = loans_time_seconds / n;
    let t_seconds = avg_seconds.max(T_FLOOR_SECONDS);
    let t_years_fp = ((t_seconds as u128) * (SCALE as u128)) / (SECONDS_PER_YEAR as u128);

    // APY = APY_min + (APY_max-APY_min) * U * 1/(1+alpha*T)
    let one = SCALE as u128;
    let time_denom = one.saturating_add(((alpha as u128) * t_years_fp) / one);
    let time_factor = (one * one) / time_denom;
    let mul = (u_fp * time_factor) / one;
    let apy_range = (APY_MAX - APY_MIN) as u128;
    let mut apy = (APY_MIN as u128) + (apy_range * mul) / one;
    if apy > APY_MAX as u128 {
        apy = APY_MAX as u128;
    }
    apy as u64
}

fn calculate_interest(principal: u64, apy: u64, duration_seconds: u64) -> u64 {
    const SECONDS_PER_YEAR: u64 = 31_536_000;
    principal
        .saturating_mul(apy)
        .saturating_mul(duration_seconds)
        / SECONDS_PER_YEAR
        / SCALE
}

pub fn lend(env: Env, user: Address, category: Category, token_id: TokenId, power: u32) {
    // update accumulators
    update_state(&env, |e, st| {
        let now = e.ledger().timestamp();
        let dt = now.saturating_sub(st.last_update_ts);
        st.borrowed_time_seconds = st
            .borrowed_time_seconds
            .saturating_add((st.total_borrowed_power as u64).saturating_mul(dt));
        st.loans_time_seconds = st
            .loans_time_seconds
            .saturating_add(st.active_loans.saturating_mul(dt));
        st.last_update_ts = now;
    });

    user.require_auth();
    let owner = read_user(&env, user).owner;
    let config = read_config(&env);
    let power_fee: u32 = power.saturating_mul(config.power_action_fee) / 100;
    let lend_amount: u32 = power.saturating_sub(power_fee);
    assert!(
        category == Category::Resource || category == Category::Leader,
        "Invalid Category to lend"
    );

    let nft_read = read_nft(&env.clone(), owner.clone(), token_id.clone()).unwrap();
    assert!(
        nft_read.locked_by_action == Action::None,
        "Card is locked by another action"
    );
    assert!(nft_read.power >= power, "Exceed power amount to lend");

    // Move gross power out of the card; fee goes to pot, net to pool (state.offer)
    update_nft(&env, owner.clone(), token_id.clone(), |_, card| {
        card.power = card.power.saturating_sub(power);
        card.locked_by_action = Action::Lend;
    });

    update_balance(&env, |_, balance| {
        balance.haw_ai_power += power_fee;
    });

    update_state(&env, |_, state| {
        state.total_offer += lend_amount as u64; // principal_net supplied to pool
    });

    let lent_at = env.ledger().timestamp();
    let lending = Lending {
        lender: owner.clone(),
        category: category.clone(),
        token_id: token_id.clone(),
        power: lend_amount,
        lent_at,
    };

    write_lending(
        env.clone(),
        owner.clone(),
        category.clone(),
        token_id.clone(),
        lending,
    );

    // Emit lend event
    emit_lend(&env, &owner, &category, &token_id);

    // Mint terry to user as rewards
    mint_terry(&env, owner.clone(), config.terry_per_lending);

    update_balance(&env, |_, balance| {
        balance.haw_ai_terry += config.terry_per_lending * config.haw_ai_percentage as i128 / 100;
    });
}

pub fn borrow(env: Env, user: Address, category: Category, token_id: TokenId, power: u32) {
    // update accumulators
    update_state(&env, |e, st| {
        let now = e.ledger().timestamp();
        let dt = now.saturating_sub(st.last_update_ts);
        st.borrowed_time_seconds = st
            .borrowed_time_seconds
            .saturating_add((st.total_borrowed_power as u64).saturating_mul(dt));
        st.loans_time_seconds = st
            .loans_time_seconds
            .saturating_add(st.active_loans.saturating_mul(dt));
        st.last_update_ts = now;
    });

    user.require_auth();
    let mut user_info = read_user(&env, user.clone());
    let owner = user_info.owner.clone();
    let config = read_config(&env);
    let power_fee: u32 = power.saturating_mul(config.power_action_fee) / 100;
    let borrow_amount: u32 = power.saturating_sub(power_fee);

    // Borrow > 0 validations
    assert!(power > 0, "Invalid borrow: zero");
    assert!(borrow_amount > 0, "Invalid borrow: net <= 0");

    assert!(
        category == Category::Resource || category == Category::Leader,
        "Invalid Category to borrow"
    );

    let nft_read = read_nft(&env.clone(), owner.clone(), token_id.clone()).unwrap();
    assert!(
        nft_read.locked_by_action == Action::None,
        "Card is locked by another action"
    );

    // config already read above

    let mut state = read_state(&env);
    assert!(
        state.total_offer >= borrow_amount as u64,
        "Insufficient power to borrow"
    );

    // Pre-validate using hypothetical post-borrow state (no mutation yet)
    let offer_after = state.total_offer.saturating_sub(borrow_amount as u64);
    let borrowed_after = state
        .total_borrowed_power
        .saturating_add(borrow_amount as u64);
    let active_loans_after = state.active_loans.saturating_add(1);

    // Compute APY and reserve using horizon T_MAX and guard k<1
    let apy = calculate_apy(
        borrowed_after,
        offer_after,
        state.loans_time_seconds,
        active_loans_after,
        config.apy_alpha as u64,
    );

    // k = APY * T_MAX in fixed point
    let k_fp = (apy as u128).saturating_mul(T_MAX_FP as u128) / (SCALE as u128);
    assert!(k_fp < SCALE as u128, "Invalid horizon: APY*T_max >= 1");
    // Reserve = P * k / (1 - k)
    let reserve =
        (borrow_amount as u128).saturating_mul(k_fp) / ((SCALE as u128).saturating_sub(k_fp));

    // Fee and buffer checks
    let buffer_bps: u32 = 500; // 5% default safety buffer
    let collateral_net = (nft_read.power as u128).saturating_sub(power_fee as u128);
    let buffer = (collateral_net.saturating_mul(buffer_bps as u128)) / 10_000u128;
    let lhs = (borrow_amount as u128)
        .saturating_add(reserve)
        .saturating_add(power_fee as u128)
        .saturating_add(buffer);
    assert!(lhs <= nft_read.power as u128, "Exceeds collateral capacity");

    // Now commit the state mutations after successful validation
    update_state(&env, |_, state| {
        state.total_offer = offer_after;
        state.total_borrowed_power = borrowed_after;
        state.active_loans = active_loans_after;
    });

    // Deduct fee immediately from collateral card and lock
    update_nft(&env, owner.clone(), token_id.clone(), |_, card| {
        card.locked_by_action = Action::Borrow;
        card.power = card.power.saturating_sub(power_fee);
    });

    update_balance(&env, |_, balance| {
        balance.haw_ai_power += power_fee;
    });

    update_user(&env, owner.clone(), |_, user| {
        user.power += borrow_amount;
    });

    let borrowing = Borrowing {
        borrower: owner.clone(),
        category: category.clone(),
        token_id: token_id.clone(),
        power: borrow_amount,
        borrowed_at: env.ledger().timestamp(),
    };

    write_borrowing(
        env.clone(),
        owner.clone(),
        category.clone(),
        token_id.clone(),
        borrowing,
    );

    // initialize BorrowMeta (weight=reserve_remaining)
    update_state(&env, |e, st| {
        let meta = crate::storage_types::BorrowMeta {
            last_l_index: st.l_index,
            weight: (reserve as u32),
            reserve_remaining: (reserve as u32),
        };
        let key = crate::storage_types::DataKey::BorrowMeta(
            owner.clone(),
            category.clone(),
            token_id.clone(),
        );
        e.storage().persistent().set(&key, &meta);
        e.storage().persistent().extend_ttl(
            &key,
            STORAGE_THRESHOLD_LEDGERS,
            STORAGE_BUMP_LEDGERS,
        );
        st.w_total = st.w_total.saturating_add(reserve as u64);
    });

    // Emit borrow event
    emit_borrow(&env, &owner, &category, &token_id);

    // Mint terry to user as rewards
    mint_terry(&env, owner.clone(), config.terry_per_lending);

    update_balance(&env, |_, balance| {
        balance.haw_ai_terry += config.terry_per_lending * config.haw_ai_percentage as i128 / 100;
    });
}

pub fn borrow_quote(
    env: Env,
    user: Address,
    _category: Category,
    token_id: TokenId,
    power: u32,
) -> BorrowQuote {
    let owner = read_user(&env, user).owner;
    let config = read_config(&env);
    let fee = power.saturating_mul(config.power_action_fee) / 100;
    let borrow_net_req: u32 = power.saturating_sub(fee);

    if power == 0 || borrow_net_req == 0 {
        return BorrowQuote {
            allowed: false,
            reason: 1,
            apy: 0,
            fee,
            reserve: 0,
            buffer: 0,
            borrow_net: borrow_net_req,
            max_suggested_gross: 0,
        };
    }

    let nft = read_nft(&env, owner.clone(), token_id.clone()).unwrap();
    let st = read_state(&env);

    // Cap by available liquidity to avoid blocking when partial liquidity exists
    let borrow_net_cap = st.total_offer.min(borrow_net_req as u64) as u32;
    if borrow_net_cap == 0 {
        return BorrowQuote {
            allowed: false,
            reason: 2,
            apy: 0,
            fee,
            reserve: 0,
            buffer: 0,
            borrow_net: borrow_net_req,
            max_suggested_gross: 0,
        };
    }

    let offer_after = st.total_offer.saturating_sub(borrow_net_cap as u64);
    let borrowed_after = st.total_borrowed_power.saturating_add(borrow_net_cap as u64);
    let active_loans_after = st.active_loans.saturating_add(1);

    let apy = calculate_apy(
        borrowed_after,
        offer_after,
        st.loans_time_seconds,
        active_loans_after,
        config.apy_alpha as u64,
    );

    let k_fp = (apy as u128).saturating_mul(T_MAX_FP as u128) / (SCALE as u128);
    if k_fp >= SCALE as u128 {
        return BorrowQuote {
            allowed: false,
            reason: 4,
            apy,
            fee,
            reserve: 0,
            buffer: 0,
            borrow_net: borrow_net_cap,
            max_suggested_gross: 0,
        };
    }

    let reserve = ((borrow_net_cap as u128).saturating_mul(k_fp))
        / ((SCALE as u128).saturating_sub(k_fp));
    let buffer_bps: u32 = 500; // 5%
    let collateral_net = (nft.power as u128).saturating_sub(fee as u128);
    let buffer = (collateral_net.saturating_mul(buffer_bps as u128)) / 10_000u128;
    let lhs = (borrow_net_cap as u128)
        .saturating_add(reserve)
        .saturating_add(fee as u128)
        .saturating_add(buffer);

    if lhs > nft.power as u128 {
        // Conservative suggestion assuming APY fixed at this quote
        let numer = (nft.power as u128)
            .saturating_sub(fee as u128)
            .saturating_sub(buffer);
        let borrow_net_max =
            (numer.saturating_mul((SCALE as u128).saturating_sub(k_fp))) / (SCALE as u128);
        let gross_suggested = ((borrow_net_max as u128) * 100u128)
            / ((100u128).saturating_sub(config.power_action_fee as u128));
        return BorrowQuote {
            allowed: false,
            reason: 3,
            apy,
            fee,
            reserve: reserve as u64,
            buffer: buffer as u64,
            borrow_net: borrow_net_cap,
            max_suggested_gross: gross_suggested as u32,
        };
    }

    // Also cap by liquidity (net)
    let gross_cap = ((borrow_net_cap as u128) * 100u128)
        / ((100u128).saturating_sub(config.power_action_fee as u128));

    BorrowQuote {
        allowed: true,
        reason: 0,
        apy,
        fee,
        reserve: reserve as u64,
        buffer: buffer as u64,
        borrow_net: borrow_net_cap,
        max_suggested_gross: gross_cap as u32,
    }
}

pub fn repay_quote(env: Env, user: Address, category: Category, token_id: TokenId) -> RepayQuote {
    let owner = read_user(&env, user).owner;
    let borrowing = read_borrowing(env.clone(), owner.clone(), category.clone(), token_id.clone());
    let config = read_config(&env);
    let state = read_state(&env);
    let loan_duration_seconds = env
        .ledger()
        .timestamp()
        .saturating_sub(borrowing.borrowed_at);
    let apy = calculate_apy(
        state.total_borrowed_power,
        state.total_offer,
        state.loans_time_seconds,
        state.active_loans,
        config.apy_alpha as u64,
    );
    let interest_amount = calculate_interest(borrowing.power as u64, apy, loan_duration_seconds);
    let required_power = borrowing.power + interest_amount as u32;
    let user_info = read_user(&env, owner.clone());
    let user_power = user_info.power;
    let collateral_power = read_nft(&env, owner, token_id.clone())
        .map(|card| card.power)
        .unwrap_or(0);
    let total_available = user_power.saturating_add(collateral_power);
    RepayQuote {
        required_power,
        user_power,
        collateral_power,
        total_available,
        sufficient: total_available >= required_power,
    }
}

pub fn repay(env: Env, user: Address, category: Category, token_id: TokenId) {
    // update accumulators
    update_state(&env, |e, st| {
        let now = e.ledger().timestamp();
        let dt = now.saturating_sub(st.last_update_ts);
        st.borrowed_time_seconds = st
            .borrowed_time_seconds
            .saturating_add((st.total_borrowed_power as u64).saturating_mul(dt));
        st.loans_time_seconds = st
            .loans_time_seconds
            .saturating_add(st.active_loans.saturating_mul(dt));
        st.last_update_ts = now;
    });

    user.require_auth();
    let user_info = read_user(&env, user);
    let owner = user_info.owner.clone();

    assert!(
        category == Category::Resource || category == Category::Leader,
        "Invalid Category to repay"
    );

    let nft_read = read_nft(&env.clone(), owner.clone(), token_id.clone()).unwrap();
    assert!(
        nft_read.locked_by_action == Action::Borrow,
        "Card is not locked by borrow action"
    );

    let borrowing = read_borrowing(
        env.clone(),
        owner.clone(),
        category.clone(),
        token_id.clone(),
    );

    let config = read_config(&env);

    let mut state = read_state(&env);

    let loan_duration_seconds = env
        .ledger()
        .timestamp()
        .saturating_sub(borrowing.borrowed_at);

    let apy = calculate_apy(
        state.total_borrowed_power,
        state.total_offer,
        state.loans_time_seconds,
        state.active_loans,
        config.apy_alpha as u64,
    );
    let interest_amount = calculate_interest(borrowing.power as u64, apy, loan_duration_seconds);
    
    update_state(&env, |_, state| {
        state.total_interest += interest_amount as u64;
        state.total_offer += borrowing.power as u64;
        state.total_borrowed_power -= borrowing.power as u64;

        state.active_loans = state.active_loans.saturating_sub(1);
    });

    let required_power = borrowing.power + interest_amount as u32;
    let user_power = user_info.power;
    let collateral_power = nft_read.power;
    assert!(
        user_power.saturating_add(collateral_power) >= required_power,
        "Insufficient fund to repay (user+collateral)"
    );
    let from_user = required_power.min(user_power);
    let from_collateral = required_power.saturating_sub(from_user);

    update_nft(&env, owner.clone(), token_id.clone(), |_, card| {
        if from_collateral > 0 {
            card.power = card
                .power
                .checked_sub(from_collateral)
                .expect("Insufficient collateral power");
        }
        card.locked_by_action = Action::None;
    });

    update_user(&env, owner.clone(), |_, user| {
        if from_user > 0 {
            user.power = user
                .power
                .checked_sub(from_user)
                .expect("Insufficient user power");
        }
    });

    // Emit repay event
    emit_repay(&env, &owner, &category, &token_id);

    remove_borrowing(
        env.clone(),
        owner.clone(),
        category.clone(),
        token_id.clone(),
    );
    // cleanup meta and w_total
    update_state(&env, |e, st| {
        let key = crate::storage_types::DataKey::BorrowMeta(
            owner.clone(),
            category.clone(),
            token_id.clone(),
        );
        if let Some(meta) = e
            .storage()
            .persistent()
            .get::<_, crate::storage_types::BorrowMeta>(&key)
        {
            st.w_total = st.w_total.saturating_sub(meta.reserve_remaining as u64);
        }
        e.storage().persistent().remove(&key);
    });

    // Mint terry to user as rewards
    let config = read_config(&env);
    mint_terry(&env, owner.clone(), config.terry_per_lending);

    update_balance(&env, |_, balance| {
        balance.haw_ai_terry += config.terry_per_lending * config.haw_ai_percentage as i128 / 100;
    });
}

fn check_liquidations(env: Env) {
    for borrowing in read_borrowings(env.clone()) {
        let nft = read_nft(&env, borrowing.borrower.clone(), borrowing.token_id.clone()).unwrap();

        let config = read_config(&env);
        let mut state = read_state(&env);
        let apy = calculate_apy(
            state.total_demand,
            state.total_offer,
            state.total_loan_duration,
            state.total_loan_count,
            config.apy_alpha as u64,
        );
        let loan_duration_seconds = env
            .ledger()
            .timestamp()
            .saturating_sub(borrowing.borrowed_at);
        let interest_amount =
            calculate_interest(borrowing.power as u64, apy, loan_duration_seconds);

        if nft.power < borrowing.power + interest_amount as u32 {
            update_state(&env, |_, state| {
                 state.total_interest += nft.power as u64;
            });

            remove_borrowing(
                env.clone(),
                borrowing.borrower,
                borrowing.category,
                borrowing.token_id,
            );
        }
    }
}

fn liquidate(env: Env, user: Address, category: Category, token_id: TokenId) {
    user.require_auth();
    let user = read_user(&env, user);
    let owner = user.owner.clone();

    let borrowing = read_borrowing(
        env.clone(),
        owner.clone(),
        category.clone(),
        token_id.clone(),
    );
    if borrowing.borrower == owner {
        let nft = read_nft(&env, borrowing.borrower.clone(), borrowing.token_id.clone()).unwrap();

        let config = read_config(&env);
        let mut state = read_state(&env);
        let apy = calculate_apy(
            state.total_demand,
            state.total_offer,
            state.total_loan_duration,
            state.total_loan_count,
            config.apy_alpha as u64,
        );
        let loan_duration = (env.ledger().timestamp() - borrowing.borrowed_at) / 3_600;
        let interest_amount = calculate_interest(borrowing.power as u64, apy, loan_duration);

        if nft.power < borrowing.power + interest_amount as u32 {
            update_state(&env, |_, state| {
                 state.total_interest += nft.power as u64;
            });

            remove_borrowing(
                env.clone(),
                borrowing.borrower,
                borrowing.category,
                borrowing.token_id,
            );
        }
    }
}

/// Materialize pending haircuts for a batch of loans (permissionless keeper)
pub fn touch_loans(env: Env, loans: Vec<(Address, Category, TokenId)>) {
    update_state(&env, |e, state| {
        for (addr, cat, tid) in loans.iter() {
            let key = DataKey::BorrowMeta(addr.clone(), cat.clone(), tid.clone());
            if let Some(mut meta) = e.storage().persistent().get::<_, BorrowMeta>(&key) {
                // pending = (L - lastL) * weight / SCALE
                let l_delta = state.l_index.saturating_sub(meta.last_l_index);
                if l_delta == 0 || meta.weight == 0 || meta.reserve_remaining == 0 {
                    continue;
                }
                let pending = ((l_delta as u128) * (meta.weight as u128) / (SCALE as u128)) as u32;
                if pending == 0 {
                    continue;
                }
                // Apply haircut bounded by reserve_remaining
                let haircut = pending.min(meta.reserve_remaining);
                meta.reserve_remaining = meta.reserve_remaining.saturating_sub(haircut);
                // Reduce weight to reflect less reserve
                state.w_total = state.w_total.saturating_sub(haircut as u64);
                meta.weight = meta.reserve_remaining;
                meta.last_l_index = state.l_index;
                e.storage().persistent().set(&key, &meta);
                e.storage().persistent().extend_ttl(
                    &key,
                    STORAGE_THRESHOLD_LEDGERS,
                    STORAGE_BUMP_LEDGERS,
                );

                // Reduce collateral POWER if reserve agotada
                let mut ownership_lost = false;
                if meta.reserve_remaining == 0 {
                    if let Some(nft_read) = read_nft(e, addr.clone(), tid.clone()) {
                        if nft_read.power > 0 {
                            let cut = haircut.min(nft_read.power);
                            
                            update_nft(e, addr.clone(), tid.clone(), |_, card| {
                                card.power = card.power.saturating_sub(cut);
                            });

                            if cut == nft_read.power {
                                ownership_lost = true;
                                emit_loan_liquidated(e, &addr);
                            }
                        }
                    }
                }
                emit_loan_touched(e, &addr, haircut, meta.reserve_remaining, ownership_lost);
            }
        }
    });
}

pub fn withdraw(env: Env, user: Address, category: Category, token_id: TokenId) {
    // update accumulators
    update_state(&env, |e, st| {
        let now = e.ledger().timestamp();
        let dt = now.saturating_sub(st.last_update_ts);
        st.borrowed_time_seconds = st
            .borrowed_time_seconds
            .saturating_add((st.total_borrowed_power as u64).saturating_mul(dt));
        st.loans_time_seconds = st
            .loans_time_seconds
            .saturating_add(st.active_loans.saturating_mul(dt));
        st.last_update_ts = now;
    });

    user.require_auth();
    let user_info = read_user(&env, user);
    let owner = user_info.owner.clone();

    assert!(
        category == Category::Resource || category == Category::Leader,
        "Invalid Category to withdraw"
    );

    let nft_read = read_nft(&env.clone(), owner.clone(), token_id.clone()).unwrap();
    assert!(
        nft_read.locked_by_action == Action::Lend,
        "Card is not locked by lend action"
    );

    let lending = read_lending(
        env.clone(),
        owner.clone(),
        category.clone(),
        token_id.clone(),
    );

    let config = read_config(&env);

    let mut state = read_state(&env);

    let loan_duration_seconds = env.ledger().timestamp().saturating_sub(lending.lent_at);
    let apy = calculate_apy(
        state.total_borrowed_power,
        state.total_offer,
        state.loans_time_seconds,
        state.active_loans,
        config.apy_alpha as u64,
    );
    let interest_amount = calculate_interest(lending.power as u64, apy, loan_duration_seconds);

    update_state(&env, |_, state| {
        if state.total_interest < interest_amount {
            // Emit index update for lazy pro‑rata: deficit -> dL = Δ / W
            let deficit = interest_amount.saturating_sub(state.total_interest);
            if state.w_total > 0 {
                let d_l = ((deficit as u128) * (SCALE as u128) / (state.w_total as u128)) as u64;
                state.l_index = state.l_index.saturating_add(d_l);
                emit_index_updated(&env, state.l_index, d_l, deficit as u64, state.w_total);
            }
            state.total_interest = 0;
        } else {
            state.total_interest -= interest_amount;
        }

        state.total_offer -= lending.power as u64;
    });

    let power_fee: u32 =
        (interest_amount.saturating_mul(config.power_action_fee as u64) / 100) as u32;
    let reward_interest: u64 = interest_amount.saturating_sub(power_fee as u64);

    // Return principal_net + intereses al NFT y desbloquear
    update_nft(&env, owner.clone(), token_id.clone(), |_, card| {
        card.power = card
            .power
            .saturating_add(lending.power.saturating_add(reward_interest as u32));
        card.locked_by_action = Action::None;
    });

    // Emit withdraw event
    emit_withdraw(&env, &owner, &category, &token_id);

    remove_lending(env.clone(), owner.clone(), category, token_id);

    // Mint terry to user as rewards
    let config = read_config(&env);
    mint_terry(&env, owner.clone(), config.terry_per_lending);

    update_balance(&env, |_, balance| {
        balance.haw_ai_power += power_fee;
        balance.haw_ai_terry += config.terry_per_lending * config.haw_ai_percentage as i128 / 100;
    });
}

pub fn get_current_apy(env: Env) -> u64 {
    let config = read_config(&env);

    let state = read_state(&env);

    calculate_apy(
        state.total_borrowed_power,
        state.total_offer,
        state.loans_time_seconds,
        state.active_loans,
        config.apy_alpha as u64,
    )
}
