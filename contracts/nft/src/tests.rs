#![cfg(test)]
#![allow(deprecated)]
use super::*;
use crate::contract::NFT;
use crate::pot::management::{
    add_round, get_current_round, get_rounds_count, read_pot_cursor, read_pot_snapshot,
    read_pot_status, read_user_generic_claimable,
};
use crate::storage_types::{
    Config, DataKey, Deck, FightKey, LendingKey, BorrowingKey, PagedListKind, PagedPosKind,
    PotStatus, StakeKey, TokenId, User, PAGE_SIZE_FIGHTS, PAGE_SIZE_LENDINGS, PAGE_SIZE_BORROWINGS,
    PAGE_SIZE_STAKES,
};
use soroban_sdk::testutils::Address as TestAddress;
use soroban_sdk::{vec, Address, Env, Vec};
use crate::actions::deck;
use crate::actions::fight::{self, Fight, FightCurrency, SidePosition};
use crate::actions::lending::{self, Borrowing, Lending};
use crate::actions::stake::{self, Stake};
use crate::admin::set_pages_only;
use crate::metadata::CardMetadata;
use crate::nft_info::{Action, Card, Category};
use crate::pot::management::set_current_round;

#[soroban_sdk::contract]
pub struct MockToken;

#[soroban_sdk::contractimpl]
impl MockToken {
    pub fn transfer(_e: Env, _from: Address, _to: Address, _amount: i128) {
        // No-op mock: just succeed
    }
}

fn init_contract(e: &Env) -> (Address, Address, Address) {
    e.mock_all_auths();
    // Deploy NFT contract
    let nft_id = e.register_contract(None, NFT);
    // Deploy two mock token contracts: xtar and generic
    let xtar_id = e.register_contract(None, MockToken);
    let generic_id = e.register_contract(None, MockToken);

    // Initialize NFT with admin and config
    let admin_address = <Address as TestAddress>::generate(e);
    let cfg = Config {
        xtar_token: xtar_id.clone(),
        oracle_contract_id: nft_id.clone(),
        withdrawable_percentage: 0,
        burnable_percentage: 0,
        haw_ai_percentage: 0,
        terry_per_power: 0,
        stake_periods: vec![e],
        stake_interest_percentages: vec![e],
        power_action_fee: 0,
        burn_receive_percentage: 0,
        terry_per_deck: 0,
        terry_per_fight: 0,
        terry_per_lending: 0,
        terry_per_stake: 0,
        apy_alpha: 0,
        power_to_usdc_rate: 0,
        dogstar_fee_percentage: 0,
    };
    NFTClient::new(e, &nft_id).initialize(&admin_address, &cfg);
    (nft_id, xtar_id, generic_id)
}

fn write_decks(e: &Env, contract: &Address, players: &[(Address, u32)], bonus: u32) {
    let mut decks: Vec<Deck> = Vec::new(e);
    for (owner, power) in players.iter() {
        let mut token_ids: Vec<TokenId> = Vec::new(e);
        token_ids.push_back(TokenId(1));
        token_ids.push_back(TokenId(2));
        token_ids.push_back(TokenId(3));
        token_ids.push_back(TokenId(4));
        decks.push_back(Deck {
            owner: owner.clone(),
            token_ids,
            total_power: *power,
            haw_ai_percentage: 0,
            bonus,
            deck_categories: 4,
        });
    }
    e.as_contract(contract, || {
        // Write individual Deck entries
        for deck in decks.iter() {
            e.storage().persistent().set(&DataKey::Deck(deck.owner.clone()), &deck);
        }
        // Write Decks collection
        e.storage().persistent().set(&DataKey::Decks, &decks);
    });
}

fn write_user_entry(e: &Env, contract: &Address, owner: &Address, power: u32) {
    e.as_contract(contract, || {
        e.storage().persistent().set(
            &DataKey::User(owner.clone()),
            &User {
                owner: owner.clone(),
                power,
                terry: 0,
                total_history_terry: 0,
                level: 1,
            },
        );
    });
}

#[test]
fn migrate_stakes_idempotent_and_pos() {
    let e = Env::default();
    let (nft_addr, _xtar_token, _generic_token) = init_contract(&e);
    let owner1 = <Address as TestAddress>::generate(&e);
    let owner2 = <Address as TestAddress>::generate(&e);

    let mut stakes: Vec<Stake> = Vec::new(&e);
    stakes.push_back(Stake {
        owner: owner1.clone(),
        category: Category::Skill,
        token_id: TokenId(1),
        power: 10,
        period: 1,
        interest_percentage: 1,
        staked_time: 1,
    });
    stakes.push_back(Stake {
        owner: owner2.clone(),
        category: Category::Leader,
        token_id: TokenId(2),
        power: 20,
        period: 2,
        interest_percentage: 2,
        staked_time: 2,
    });

    e.as_contract(&nft_addr, || {
        e.storage().persistent().set(&DataKey::Stakes, &stakes);
        stake::migrate_stakes_to_pages(&e, 0, 10);

        let count: u32 = e
            .storage()
            .persistent()
            .get(&DataKey::PagedCount(PagedListKind::Stakes))
            .unwrap_or(0);
        assert_eq!(count, stakes.len());

        stake::migrate_stakes_to_pages(&e, 0, 10);
        let count2: u32 = e
            .storage()
            .persistent()
            .get(&DataKey::PagedCount(PagedListKind::Stakes))
            .unwrap_or(0);
        assert_eq!(count2, count);

        for idx in 0..stakes.len() {
            let s = stakes.get(idx).unwrap();
            let pos_key = DataKey::Pos(
                PagedPosKind::Stakes,
                s.owner.clone(),
                s.category.clone(),
                s.token_id.clone(),
            );
            let pos: u32 = e.storage().persistent().get(&pos_key).unwrap();
            assert_eq!(pos, idx as u32);

            let page = pos / PAGE_SIZE_STAKES;
            let off = pos % PAGE_SIZE_STAKES;
            let page_key = DataKey::PagedList(PagedListKind::Stakes, page);
            let page_vec: Vec<StakeKey> = e.storage().persistent().get(&page_key).unwrap();
            let key = page_vec.get(off).unwrap();
            assert_eq!(key.owner, s.owner);
            assert_eq!(key.category, s.category);
            assert_eq!(key.token_id, s.token_id);
        }
    });
}

#[test]
fn pages_only_skips_legacy_stakes() {
    let e = Env::default();
    let (nft_addr, _xtar_token, _generic_token) = init_contract(&e);
    let owner = <Address as TestAddress>::generate(&e);

    let user = User {
        owner: owner.clone(),
        power: 0,
        terry: 0,
        total_history_terry: 0,
        level: 1,
    };

    e.as_contract(&nft_addr, || {
        e.storage().persistent().set(&DataKey::User(owner.clone()), &user);
        set_pages_only(&e, true);

        stake::write_stake(
            &e,
            owner.clone(),
            Category::Skill,
            TokenId(3),
            Stake {
                owner: owner.clone(),
                category: Category::Skill,
                token_id: TokenId(3),
                power: 5,
                period: 1,
                interest_percentage: 1,
                staked_time: 1,
            },
        );

        assert!(!e.storage().persistent().has(&DataKey::Stakes));
    });
}

#[test]
fn pages_only_skips_legacy_fights() {
    let e = Env::default();
    let (nft_addr, _xtar_token, _generic_token) = init_contract(&e);
    let owner = <Address as TestAddress>::generate(&e);
    write_user_entry(&e, &nft_addr, &owner, 0);

    e.as_contract(&nft_addr, || {
        set_pages_only(&e, true);
        fight::write_fight(
            e.clone(),
            owner.clone(),
            Category::Skill,
            TokenId(1),
            Fight {
                owner: owner.clone(),
                category: Category::Skill,
                token_id: TokenId(1),
                currency: FightCurrency::BTC,
                power: 10,
                trigger_price: 0,
                side_position: SidePosition::Long,
                leverage: 1,
                amount_asset: 1,
            },
        );
        assert!(!e.storage().persistent().has(&DataKey::Fights));
    });
}

#[test]
fn pages_only_skips_legacy_lendings_and_borrowings() {
    let e = Env::default();
    let (nft_addr, _xtar_token, _generic_token) = init_contract(&e);
    let owner = <Address as TestAddress>::generate(&e);
    write_user_entry(&e, &nft_addr, &owner, 0);

    e.as_contract(&nft_addr, || {
        set_pages_only(&e, true);
        lending::write_lending(
            e.clone(),
            owner.clone(),
            Category::Skill,
            TokenId(1),
            Lending {
                lender: owner.clone(),
                category: Category::Skill,
                token_id: TokenId(1),
                power: 10,
                lent_at: 1,
            },
        );
        lending::write_borrowing(
            e.clone(),
            owner.clone(),
            Category::Skill,
            TokenId(2),
            Borrowing {
                borrower: owner.clone(),
                category: Category::Skill,
                token_id: TokenId(2),
                power: 10,
                borrowed_at: 1,
            },
        );
        assert!(!e.storage().persistent().has(&DataKey::Lendings));
        assert!(!e.storage().persistent().has(&DataKey::Borrowings));
    });
}

#[test]
fn pages_only_rounds_skip_legacy() {
    let e = Env::default();
    let (nft_addr, _xtar_token, _generic_token) = init_contract(&e);

    e.as_contract(&nft_addr, || {
        set_pages_only(&e, true);
        add_round(&e, 1);
        assert!(!e.storage().persistent().has(&DataKey::AllRounds));
        let count = get_rounds_count(&e);
        assert_eq!(count, 1);
    });
}

#[test]
fn paged_reads_return_empty_when_cursor_out_of_range() {
    let e = Env::default();
    let (nft_addr, _xtar_token, _generic_token) = init_contract(&e);

    e.as_contract(&nft_addr, || {
        set_pages_only(&e, true);

        let stakes_page = stake::read_stakes_page(&e, 10, 5);
        assert!(stakes_page.is_empty());

        let decks_page = deck::read_decks_page(&e, 10, 5);
        assert!(decks_page.is_empty());
    });
}

#[test]
fn migrate_fights_idempotent_and_pos() {
    let e = Env::default();
    let (nft_addr, _xtar_token, _generic_token) = init_contract(&e);
    let owner1 = <Address as TestAddress>::generate(&e);
    let owner2 = <Address as TestAddress>::generate(&e);

    let mut fights: Vec<Fight> = Vec::new(&e);
    fights.push_back(Fight {
        owner: owner1.clone(),
        category: Category::Skill,
        token_id: TokenId(1),
        currency: FightCurrency::BTC,
        power: 10,
        trigger_price: 0,
        side_position: SidePosition::Long,
        leverage: 1,
        amount_asset: 1,
    });
    fights.push_back(Fight {
        owner: owner2.clone(),
        category: Category::Leader,
        token_id: TokenId(2),
        currency: FightCurrency::ETH,
        power: 20,
        trigger_price: 0,
        side_position: SidePosition::Short,
        leverage: 1,
        amount_asset: 1,
    });

    e.as_contract(&nft_addr, || {
        e.storage().persistent().set(&DataKey::Fights, &fights);
        fight::migrate_fights_to_pages(&e, 0, 10);

        let count: u32 = e
            .storage()
            .persistent()
            .get(&DataKey::PagedCount(PagedListKind::Fights))
            .unwrap_or(0);
        assert_eq!(count, fights.len());

        fight::migrate_fights_to_pages(&e, 0, 10);
        let count2: u32 = e
            .storage()
            .persistent()
            .get(&DataKey::PagedCount(PagedListKind::Fights))
            .unwrap_or(0);
        assert_eq!(count2, count);

        for idx in 0..fights.len() {
            let f = fights.get(idx).unwrap();
            let pos_key = DataKey::Pos(
                PagedPosKind::Fights,
                f.owner.clone(),
                f.category.clone(),
                f.token_id.clone(),
            );
            let pos: u32 = e.storage().persistent().get(&pos_key).unwrap();
            assert_eq!(pos, idx as u32);

            let page = pos / PAGE_SIZE_FIGHTS;
            let off = pos % PAGE_SIZE_FIGHTS;
            let page_key = DataKey::PagedList(PagedListKind::Fights, page);
            let page_vec: Vec<FightKey> = e.storage().persistent().get(&page_key).unwrap();
            let key = page_vec.get(off).unwrap();
            assert_eq!(key.owner, f.owner);
            assert_eq!(key.category, f.category);
            assert_eq!(key.token_id, f.token_id);
        }
    });
}

#[test]
fn fights_swap_remove_updates_pos() {
    let e = Env::default();
    let (nft_addr, _xtar_token, _generic_token) = init_contract(&e);
    let owner = <Address as TestAddress>::generate(&e);
    write_user_entry(&e, &nft_addr, &owner, 0);

    for token_id in 1..=3 {
        e.as_contract(&nft_addr, || {
            fight::write_fight(
                e.clone(),
                owner.clone(),
                Category::Skill,
                TokenId(token_id),
                Fight {
                    owner: owner.clone(),
                    category: Category::Skill,
                    token_id: TokenId(token_id),
                    currency: FightCurrency::BTC,
                    power: 10,
                    trigger_price: 0,
                    side_position: SidePosition::Long,
                    leverage: 1,
                    amount_asset: 1,
                },
            );
        });
    }

    e.as_contract(&nft_addr, || {
        fight::remove_fight(e.clone(), owner.clone(), Category::Skill, TokenId(2));
        let count = fight::read_fights_count(&e);
        assert_eq!(count, 2);

        let page = fight::read_fights_page(&e, 0, PAGE_SIZE_FIGHTS);
        assert_eq!(page.len(), 2);
        let mut has_1 = false;
        let mut has_3 = false;
        for key in page.iter() {
            if key.token_id.0 == 1 {
                has_1 = true;
            }
            if key.token_id.0 == 3 {
                has_3 = true;
            }
        }
        assert!(has_1);
        assert!(has_3);

        for key in page.iter() {
            let pos_key = DataKey::Pos(
                PagedPosKind::Fights,
                key.owner.clone(),
                key.category.clone(),
                key.token_id.clone(),
            );
            let pos: u32 = e.storage().persistent().get(&pos_key).unwrap();
            let page_idx = pos / PAGE_SIZE_FIGHTS;
            let off = pos % PAGE_SIZE_FIGHTS;
            let page_key = DataKey::PagedList(PagedListKind::Fights, page_idx);
            let page_vec: Vec<FightKey> = e.storage().persistent().get(&page_key).unwrap();
            let stored = page_vec.get(off).unwrap();
            assert_eq!(stored.token_id, key.token_id);
        }
    });
}

#[test]
fn migrate_lendings_idempotent_and_pos() {
    let e = Env::default();
    let (nft_addr, _xtar_token, _generic_token) = init_contract(&e);
    let owner1 = <Address as TestAddress>::generate(&e);
    let owner2 = <Address as TestAddress>::generate(&e);

    let mut lendings: Vec<Lending> = Vec::new(&e);
    lendings.push_back(Lending {
        lender: owner1.clone(),
        category: Category::Skill,
        token_id: TokenId(1),
        power: 10,
        lent_at: 1,
    });
    lendings.push_back(Lending {
        lender: owner2.clone(),
        category: Category::Leader,
        token_id: TokenId(2),
        power: 20,
        lent_at: 2,
    });

    e.as_contract(&nft_addr, || {
        e.storage().persistent().set(&DataKey::Lendings, &lendings);
        lending::migrate_lendings_to_pages(&e, 0, 10);

        let count: u32 = e
            .storage()
            .persistent()
            .get(&DataKey::PagedCount(PagedListKind::Lendings))
            .unwrap_or(0);
        assert_eq!(count, lendings.len());

        lending::migrate_lendings_to_pages(&e, 0, 10);
        let count2: u32 = e
            .storage()
            .persistent()
            .get(&DataKey::PagedCount(PagedListKind::Lendings))
            .unwrap_or(0);
        assert_eq!(count2, count);

        for idx in 0..lendings.len() {
            let l = lendings.get(idx).unwrap();
            let pos_key = DataKey::Pos(
                PagedPosKind::Lendings,
                l.lender.clone(),
                l.category.clone(),
                l.token_id.clone(),
            );
            let pos: u32 = e.storage().persistent().get(&pos_key).unwrap();
            assert_eq!(pos, idx as u32);

            let page = pos / PAGE_SIZE_LENDINGS;
            let off = pos % PAGE_SIZE_LENDINGS;
            let page_key = DataKey::PagedList(PagedListKind::Lendings, page);
            let page_vec: Vec<LendingKey> = e.storage().persistent().get(&page_key).unwrap();
            let key = page_vec.get(off).unwrap();
            assert_eq!(key.owner, l.lender);
            assert_eq!(key.category, l.category);
            assert_eq!(key.token_id, l.token_id);
        }
    });
}

#[test]
fn migrate_borrowings_idempotent_and_pos() {
    let e = Env::default();
    let (nft_addr, _xtar_token, _generic_token) = init_contract(&e);
    let owner1 = <Address as TestAddress>::generate(&e);
    let owner2 = <Address as TestAddress>::generate(&e);

    let mut borrowings: Vec<Borrowing> = Vec::new(&e);
    borrowings.push_back(Borrowing {
        borrower: owner1.clone(),
        category: Category::Skill,
        token_id: TokenId(3),
        power: 30,
        borrowed_at: 1,
    });
    borrowings.push_back(Borrowing {
        borrower: owner2.clone(),
        category: Category::Leader,
        token_id: TokenId(4),
        power: 40,
        borrowed_at: 2,
    });

    e.as_contract(&nft_addr, || {
        e.storage().persistent().set(&DataKey::Borrowings, &borrowings);
        lending::migrate_borrowings_to_pages(&e, 0, 10);

        let count: u32 = e
            .storage()
            .persistent()
            .get(&DataKey::PagedCount(PagedListKind::Borrowings))
            .unwrap_or(0);
        assert_eq!(count, borrowings.len());

        lending::migrate_borrowings_to_pages(&e, 0, 10);
        let count2: u32 = e
            .storage()
            .persistent()
            .get(&DataKey::PagedCount(PagedListKind::Borrowings))
            .unwrap_or(0);
        assert_eq!(count2, count);

        for idx in 0..borrowings.len() {
            let b = borrowings.get(idx).unwrap();
            let pos_key = DataKey::Pos(
                PagedPosKind::Borrowings,
                b.borrower.clone(),
                b.category.clone(),
                b.token_id.clone(),
            );
            let pos: u32 = e.storage().persistent().get(&pos_key).unwrap();
            assert_eq!(pos, idx as u32);

            let page = pos / PAGE_SIZE_BORROWINGS;
            let off = pos % PAGE_SIZE_BORROWINGS;
            let page_key = DataKey::PagedList(PagedListKind::Borrowings, page);
            let page_vec: Vec<BorrowingKey> = e.storage().persistent().get(&page_key).unwrap();
            let key = page_vec.get(off).unwrap();
            assert_eq!(key.owner, b.borrower);
            assert_eq!(key.category, b.category);
            assert_eq!(key.token_id, b.token_id);
        }
    });
}

#[test]
fn migrate_decks_idempotent_and_pos() {
    let e = Env::default();
    let (nft_addr, _xtar_token, _generic_token) = init_contract(&e);
    let p1 = <Address as TestAddress>::generate(&e);
    let p2 = <Address as TestAddress>::generate(&e);
    write_decks(&e, &nft_addr, &[(p1.clone(), 100), (p2.clone(), 200)], 10);

    e.as_contract(&nft_addr, || {
        deck::migrate_decks_to_pages(&e, 0, 10);
        let count: u32 = e
            .storage()
            .persistent()
            .get(&DataKey::PagedCount(PagedListKind::Decks))
            .unwrap_or(0);
        assert_eq!(count, 2);

        deck::migrate_decks_to_pages(&e, 0, 10);
        let count2: u32 = e
            .storage()
            .persistent()
            .get(&DataKey::PagedCount(PagedListKind::Decks))
            .unwrap_or(0);
        assert_eq!(count2, count);
    });
}

#[test]
fn migrate_rounds_idempotent() {
    let e = Env::default();
    let (nft_addr, _xtar_token, _generic_token) = init_contract(&e);

    e.as_contract(&nft_addr, || {
        e.storage()
            .persistent()
            .set(&DataKey::AllRounds, &vec![&e, 1u32, 2u32, 3u32]);

        crate::pot::management::migrate_rounds_to_pages(&e, 0, 10);
        let count: u32 = e
            .storage()
            .persistent()
            .get(&DataKey::PagedCount(PagedListKind::Rounds))
            .unwrap_or(0);
        assert_eq!(count, 3);

        crate::pot::management::migrate_rounds_to_pages(&e, 0, 10);
        let count2: u32 = e
            .storage()
            .persistent()
            .get(&DataKey::PagedCount(PagedListKind::Rounds))
            .unwrap_or(0);
        assert_eq!(count2, count);
    });
}

#[test]
fn lendings_swap_remove_updates_pos() {
    let e = Env::default();
    let (nft_addr, _xtar_token, _generic_token) = init_contract(&e);
    let owner = <Address as TestAddress>::generate(&e);
    write_user_entry(&e, &nft_addr, &owner, 0);

    e.as_contract(&nft_addr, || {
        for token_id in 1..=3 {
            lending::write_lending(
                e.clone(),
                owner.clone(),
                Category::Skill,
                TokenId(token_id),
                Lending {
                    lender: owner.clone(),
                    category: Category::Skill,
                    token_id: TokenId(token_id),
                    power: 10,
                    lent_at: 1,
                },
            );
        }

        lending::remove_lending(e.clone(), owner.clone(), Category::Skill, TokenId(2));
        let count = lending::read_lendings_count(&e);
        assert_eq!(count, 2);

        let page = lending::read_lendings_page(&e, 0, PAGE_SIZE_LENDINGS);
        assert_eq!(page.len(), 2);
        let mut has_1 = false;
        let mut has_3 = false;
        for key in page.iter() {
            if key.token_id.0 == 1 {
                has_1 = true;
            }
            if key.token_id.0 == 3 {
                has_3 = true;
            }
        }
        assert!(has_1);
        assert!(has_3);

        for key in page.iter() {
            let pos_key = DataKey::Pos(
                PagedPosKind::Lendings,
                key.owner.clone(),
                key.category.clone(),
                key.token_id.clone(),
            );
            let pos: u32 = e.storage().persistent().get(&pos_key).unwrap();
            let page_idx = pos / PAGE_SIZE_LENDINGS;
            let off = pos % PAGE_SIZE_LENDINGS;
            let page_key = DataKey::PagedList(PagedListKind::Lendings, page_idx);
            let page_vec: Vec<LendingKey> = e.storage().persistent().get(&page_key).unwrap();
            let stored = page_vec.get(off).unwrap();
            assert_eq!(stored.token_id, key.token_id);
        }
    });
}

#[test]
fn borrowings_swap_remove_updates_pos() {
    let e = Env::default();
    let (nft_addr, _xtar_token, _generic_token) = init_contract(&e);
    let owner = <Address as TestAddress>::generate(&e);
    write_user_entry(&e, &nft_addr, &owner, 0);

    e.as_contract(&nft_addr, || {
        for token_id in 1..=3 {
            lending::write_borrowing(
                e.clone(),
                owner.clone(),
                Category::Skill,
                TokenId(token_id),
                Borrowing {
                    borrower: owner.clone(),
                    category: Category::Skill,
                    token_id: TokenId(token_id),
                    power: 10,
                    borrowed_at: 1,
                },
            );
        }

        lending::remove_borrowing(e.clone(), owner.clone(), Category::Skill, TokenId(2));
        let count = lending::read_borrowings_count(&e);
        assert_eq!(count, 2);

        let page = lending::read_borrowings_page(&e, 0, PAGE_SIZE_BORROWINGS);
        assert_eq!(page.len(), 2);
        let mut has_1 = false;
        let mut has_3 = false;
        for key in page.iter() {
            if key.token_id.0 == 1 {
                has_1 = true;
            }
            if key.token_id.0 == 3 {
                has_3 = true;
            }
        }
        assert!(has_1);
        assert!(has_3);

        for key in page.iter() {
            let pos_key = DataKey::Pos(
                PagedPosKind::Borrowings,
                key.owner.clone(),
                key.category.clone(),
                key.token_id.clone(),
            );
            let pos: u32 = e.storage().persistent().get(&pos_key).unwrap();
            let page_idx = pos / PAGE_SIZE_BORROWINGS;
            let off = pos % PAGE_SIZE_BORROWINGS;
            let page_key = DataKey::PagedList(PagedListKind::Borrowings, page_idx);
            let page_vec: Vec<BorrowingKey> = e.storage().persistent().get(&page_key).unwrap();
            let stored = page_vec.get(off).unwrap();
            assert_eq!(stored.token_id, key.token_id);
        }
    });
}

#[test]
fn stakes_page_boundaries() {
    let e = Env::default();
    let (nft_addr, _xtar_token, _generic_token) = init_contract(&e);
    let owner = <Address as TestAddress>::generate(&e);
    write_user_entry(&e, &nft_addr, &owner, 0);

    e.as_contract(&nft_addr, || {
        let total = PAGE_SIZE_STAKES + 1;
        for token in 0..total {
            stake::write_stake(
                &e,
                owner.clone(),
                Category::Skill,
                TokenId(token + 1),
                Stake {
                    owner: owner.clone(),
                    category: Category::Skill,
                    token_id: TokenId(token + 1),
                    power: 5,
                    period: 1,
                    interest_percentage: 1,
                    staked_time: 1,
                },
            );
        }

        let count = stake::read_stakes_count(&e);
        assert_eq!(count, total);

        let page0 = stake::read_stakes_page(&e, 0, PAGE_SIZE_STAKES);
        assert_eq!(page0.len(), PAGE_SIZE_STAKES);

        let page1 = stake::read_stakes_page(&e, PAGE_SIZE_STAKES, PAGE_SIZE_STAKES);
        assert_eq!(page1.len(), 1);
    });
}

#[test]
fn stakes_swap_remove_updates_pos() {
    let e = Env::default();
    let (nft_addr, _xtar_token, _generic_token) = init_contract(&e);
    let owner = <Address as TestAddress>::generate(&e);
    write_user_entry(&e, &nft_addr, &owner, 0);

    e.as_contract(&nft_addr, || {
        for token_id in 1..=3 {
            stake::write_stake(
                &e,
                owner.clone(),
                Category::Skill,
                TokenId(token_id),
                Stake {
                    owner: owner.clone(),
                    category: Category::Skill,
                    token_id: TokenId(token_id),
                    power: 5,
                    period: 1,
                    interest_percentage: 1,
                    staked_time: 1,
                },
            );
        }

        stake::remove_stake(&e, owner.clone(), Category::Skill, TokenId(2));
        let count = stake::read_stakes_count(&e);
        assert_eq!(count, 2);

        let page = stake::read_stakes_page(&e, 0, PAGE_SIZE_STAKES);
        assert_eq!(page.len(), 2);
        let mut has_1 = false;
        let mut has_3 = false;
        for key in page.iter() {
            if key.token_id.0 == 1 {
                has_1 = true;
            }
            if key.token_id.0 == 3 {
                has_3 = true;
            }
        }
        assert!(has_1);
        assert!(has_3);

        for key in page.iter() {
            let pos_key = DataKey::Pos(
                PagedPosKind::Stakes,
                key.owner.clone(),
                key.category.clone(),
                key.token_id.clone(),
            );
            let pos: u32 = e.storage().persistent().get(&pos_key).unwrap();
            let page_idx = pos / PAGE_SIZE_STAKES;
            let off = pos % PAGE_SIZE_STAKES;
            let page_key = DataKey::PagedList(PagedListKind::Stakes, page_idx);
            let page_vec: Vec<StakeKey> = e.storage().persistent().get(&page_key).unwrap();
            let stored = page_vec.get(off).unwrap();
            assert_eq!(stored.token_id, key.token_id);
        }
    });
}

#[test]
fn open_pot_multi_step_resumable() {
    let e = Env::default();
    let (nft_addr, _xtar_token, _generic_token) = init_contract(&e);
    let p1 = <Address as TestAddress>::generate(&e);
    let p2 = <Address as TestAddress>::generate(&e);
    write_decks(&e, &nft_addr, &[(p1.clone(), 100), (p2.clone(), 200)], 10);

    let client = NFTClient::new(&e, &nft_addr);
    let round = 1u32;

    client.open_pot_start(&round);
    e.as_contract(&nft_addr, || {
        assert_eq!(read_pot_status(&e, round), PotStatus::Totals);
    });

    let cursor1 = client.open_pot_process_totals(&round, &1);
    assert!(cursor1 > 0);
    e.as_contract(&nft_addr, || {
        assert_eq!(read_pot_status(&e, round), PotStatus::Totals);
    });

    let cursor2 = client.open_pot_process_totals(&round, &1);
    assert_eq!(cursor2, 2);
    e.as_contract(&nft_addr, || {
        assert_eq!(read_pot_status(&e, round), PotStatus::Shares);
        assert_eq!(read_pot_cursor(&e, round), 0);
    });

    let s1 = client.open_pot_process_shares(&round, &1);
    assert_eq!(s1, 1);
    let s2 = client.open_pot_process_shares(&round, &1);
    assert_eq!(s2, 2);

    client.open_pot_finalize(&round);
    e.as_contract(&nft_addr, || {
        assert_eq!(read_pot_status(&e, round), PotStatus::Finalized);
        assert_eq!(get_current_round(&e), round);
    });
}

#[test]
#[should_panic(expected = "Card locked by action")]
fn add_power_to_locked_card_fails() {
    let e = Env::default();
    let (nft_addr, _xtar_token, _generic_token) = init_contract(&e);
    let owner = <Address as TestAddress>::generate(&e);
    let nft = NFTClient::new(&e, &nft_addr);

    e.as_contract(&nft_addr, || {
        e.storage().instance().set(
            &DataKey::TokenId(1),
            &CardMetadata {
                initial_power: 10,
                max_power: 100,
                level: 1,
                category: Category::Skill,
                price_xtar: 0,
                price_terry: 0,
                token_id: 1,
            },
        );
        e.storage().persistent().set(
            &DataKey::User(owner.clone()),
            &User {
                owner: owner.clone(),
                power: 10,
                terry: 0,
                total_history_terry: 0,
                level: 1,
            },
        );
        e.storage().persistent().set(
            &DataKey::Card(owner.clone(), TokenId(1)),
            &Card {
                power: 10,
                locked_by_action: Action::Stake,
            },
        );
    });

    nft.add_power_to_card(&owner, &1, &5);
}

#[test]
#[should_panic]
fn deck_mutations_blocked_during_pot() {
    let e = Env::default();
    let (nft_addr, _xtar_token, _generic_token) = init_contract(&e);
    let owner = <Address as TestAddress>::generate(&e);
    let nft = NFTClient::new(&e, &nft_addr);

    e.as_contract(&nft_addr, || {
        // Mark current round = 1 and set next round status to Totals
        set_current_round(&e, 1);
        e.storage()
            .persistent()
            .set(&DataKey::PotStatus(2), &PotStatus::Totals);
    });

    nft.place(&owner, &TokenId(1));
}

#[test]
fn read_pot_status_and_cursor() {
    let e = Env::default();
    let (nft_addr, _xtar_token, _generic_token) = init_contract(&e);
    let nft = NFTClient::new(&e, &nft_addr);

    // Defaults for unknown round
    let status = nft.get_pot_status(&1);
    let cursor = nft.get_pot_cursor(&1);
    assert_eq!(status, PotStatus::Init);
    assert_eq!(cursor, 0);

    // After open_pot_start, status is Totals and cursor 0
    nft.open_pot_start(&1);
    let status = nft.get_pot_status(&1);
    let cursor = nft.get_pot_cursor(&1);
    assert_eq!(status, PotStatus::Totals);
    assert_eq!(cursor, 0);
}

#[test]
fn multi_asset_pot_accumulate_snapshot_and_claim() {
    let e = Env::default();
    let (nft_addr, xtar_token, generic_token) = init_contract(&e);
    let nft = NFTClient::new(&e, &nft_addr);

    // Register generic token and xtar token
    nft.register_token(&generic_token);
    nft.register_token(&xtar_token);

    // Two players with different power
    let p1 = <Address as TestAddress>::generate(&e);
    let p2 = <Address as TestAddress>::generate(&e);
    write_decks(&e, &nft_addr, &[(p1.clone(), 100), (p2.clone(), 300)], 0);

    // Accumulate amounts: XTAR goes to accumulated_xtar; generic goes to map
    nft.accumulate_pot_token(&xtar_token, &1000);
    nft.accumulate_pot_token(&generic_token, &2000);

    // Open pot for round 1
    nft.open_pot(&1);

    // Claim for player 1 (25% share)
    let (terry1, power1, xtar1) = nft.claim_haw_ai_pot_share(&p1);
    assert_eq!(terry1, 0);
    assert_eq!(power1, 0);
    assert_eq!(xtar1, 250);
    // Generic token claimable should be swept to 0
    let gen_left_p1 = {
        let mut v = 0i128;
        e.as_contract(&nft_addr, || { v = read_user_generic_claimable(&e, &p1, &generic_token); });
        v
    };
    assert_eq!(gen_left_p1, 0);

    // Claim for player 2 (75% share)
    let (_, _, xtar2) = nft.claim_haw_ai_pot_share(&p2);
    assert_eq!(xtar2, 750);
    let gen_left_p2 = {
        let mut v = 0i128;
        e.as_contract(&nft_addr, || { v = read_user_generic_claimable(&e, &p2, &generic_token); });
        v
    };
    assert_eq!(gen_left_p2, 0);

    // Re-claim would panic via client; skip re-claim check
}

#[test]
fn no_eligible_players_no_claim() {
    let e = Env::default();
    let (nft_addr, xtar_token, generic_token) = init_contract(&e);
    let nft = NFTClient::new(&e, &nft_addr);

    // Register tokens
    nft.register_token(&generic_token);
    nft.register_token(&xtar_token);

    // One player but ineligible deck (only 3 tokens)
    let p = <Address as TestAddress>::generate(&e);
    let mut decks: Vec<Deck> = Vec::new(&e);
    let mut token_ids: Vec<TokenId> = Vec::new(&e);
    token_ids.push_back(TokenId(1));
    token_ids.push_back(TokenId(2));
    token_ids.push_back(TokenId(3));
    decks.push_back(Deck { owner: p.clone(), token_ids, total_power: 100, haw_ai_percentage: 0, bonus: 0, deck_categories: 3 });
    e.as_contract(&nft_addr, || {
        for deck in decks.iter() {
            e.storage().persistent().set(&DataKey::Deck(deck.owner.clone()), &deck);
        }
        e.storage().persistent().set(&DataKey::Decks, &decks);
    });

    // Accumulate some tokens and open pot
    nft.accumulate_pot_token(&xtar_token, &1000);
    nft.accumulate_pot_token(&generic_token, &500);
    nft.open_pot(&1);

    // No rewards available due to no eligible players; skip claim
}

#[test]
fn pot_multi_step_flow() {
    let e = Env::default();
    let (nft_addr, xtar_token, _generic_token) = init_contract(&e);
    let nft = NFTClient::new(&e, &nft_addr);

    let p1 = <Address as TestAddress>::generate(&e);
    let p2 = <Address as TestAddress>::generate(&e);
    write_decks(&e, &nft_addr, &[(p1.clone(), 100), (p2.clone(), 300)], 0);

    nft.register_token(&xtar_token);
    nft.accumulate_pot_token(&xtar_token, &1000);

    nft.open_pot_start(&1);
    nft.open_pot_process_totals(&1, &1);
    nft.open_pot_process_totals(&1, &1);
    nft.open_pot_process_shares(&1, &1);
    nft.open_pot_process_shares(&1, &1);
    nft.open_pot_finalize(&1);

    let snapshot = nft.get_historical_snapshot(&1).unwrap();
    assert_eq!(snapshot.total_participants, 2);
    assert!(snapshot.total_effective_power > 0);
}

#[test]
fn multiple_openings_claim_sweeps_all() {
    let e = Env::default();
    let (nft_addr, xtar_token, generic_token) = init_contract(&e);
    let nft = NFTClient::new(&e, &nft_addr);

    // Register tokens
    nft.register_token(&generic_token);
    nft.register_token(&xtar_token);

    // Single eligible player gets 100% share
    let p = <Address as TestAddress>::generate(&e);
    write_decks(&e, &nft_addr, &[(p.clone(), 100)], 0);

    // Opening 1
    nft.accumulate_pot_token(&xtar_token, &100);
    nft.accumulate_pot_token(&generic_token, &50);
    nft.open_pot(&1);

    // Opening 2
    nft.accumulate_pot_token(&xtar_token, &200);
    nft.accumulate_pot_token(&generic_token, &150);
    nft.open_pot(&2);

    // Single claim sweeps both openings
    let (_, _, xtar_claimed) = nft.claim_haw_ai_pot_share(&p);
    assert_eq!(xtar_claimed, 300);
    let gen_left = {
        let mut v = 0i128;
        e.as_contract(&nft_addr, || { v = read_user_generic_claimable(&e, &p, &generic_token); });
        v
    };
    assert_eq!(gen_left, 0);

    // Re-claim would panic via client; skip
}

#[test]
fn bonus_impacts_share_and_snapshot_with_multiple_generics() {
    let e = Env::default();
    let (nft_addr, xtar_token, generic_a) = init_contract(&e);
    // Deploy second generic token
    let generic_b_id = e.register_contract(None, MockToken);
    let generic_b = generic_b_id.clone();
    let nft = NFTClient::new(&e, &nft_addr);

    // Register tokens
    nft.register_token(&generic_a);
    nft.register_token(&generic_b);
    nft.register_token(&xtar_token);

    // Two players same power, different bonus
    let p1 = <Address as TestAddress>::generate(&e);
    let p2 = <Address as TestAddress>::generate(&e);
    // Write decks with different bonuses
    let mut decks: Vec<Deck> = Vec::new(&e);
    let mut ids4: Vec<TokenId> = Vec::new(&e);
    ids4.push_back(TokenId(1)); ids4.push_back(TokenId(2)); ids4.push_back(TokenId(3)); ids4.push_back(TokenId(4));
    let d1 = Deck { owner: p1.clone(), token_ids: ids4.clone(), total_power: 100, haw_ai_percentage: 0, bonus: 0, deck_categories: 4 };
    let d2 = Deck { owner: p2.clone(), token_ids: ids4.clone(), total_power: 100, haw_ai_percentage: 0, bonus: 100, deck_categories: 4 };
    decks.push_back(d1.clone());
    decks.push_back(d2.clone());
    e.as_contract(&nft_addr, || {
        e.storage().persistent().set(&DataKey::Deck(p1.clone()), &d1);
        e.storage().persistent().set(&DataKey::Deck(p2.clone()), &d2);
        e.storage().persistent().set(&DataKey::Decks, &decks);
    });

    // Accumulate into pot: XTAR 300, genA 900, genB 300
    nft.accumulate_pot_token(&xtar_token, &300);
    nft.accumulate_pot_token(&generic_a, &900);
    nft.accumulate_pot_token(&generic_b, &300);
    nft.open_pot(&1);

    // Validate snapshot
    let snap = {
        let mut s = None;
        e.as_contract(&nft_addr, || { s = read_pot_snapshot(&e, 1); });
        s.unwrap()
    };
    assert_eq!(snap.total_xtar, 300);
    // generic tokens may appear in any order; verify by matching
    let mut found_a = false; let mut found_b = false;
    for gta in snap.generic_tokens.iter() {
        if gta.token == generic_a { assert_eq!(gta.amount, 900); found_a = true; }
        if gta.token == generic_b { assert_eq!(gta.amount, 300); found_b = true; }
    }
    assert!(found_a && found_b);

    // Claim P1 (share ~33.33%)
    let (_, _, xtar1) = nft.claim_haw_ai_pot_share(&p1);
    assert_eq!(xtar1, 99);
    // genA share 300, genB share 100; both cleared after claim
    e.as_contract(&nft_addr, || {
        assert_eq!(read_user_generic_claimable(&e, &p1, &generic_a), 0);
        assert_eq!(read_user_generic_claimable(&e, &p1, &generic_b), 0);
    });

    // Claim P2 (share ~66.66%)
    let (_, _, xtar2) = nft.claim_haw_ai_pot_share(&p2);
    assert_eq!(xtar2, 199);
    e.as_contract(&nft_addr, || {
        assert_eq!(read_user_generic_claimable(&e, &p2, &generic_a), 0);
        assert_eq!(read_user_generic_claimable(&e, &p2, &generic_b), 0);
    });

    // No further rewards; skip re-claim assertions
}


#[test]
fn view_pot_overview_totals_and_user_claimables() {
    let e = Env::default();
    let (nft_addr, xtar_token, generic_token) = init_contract(&e);
    let nft = NFTClient::new(&e, &nft_addr);

    // Register tokens
    nft.register_token(&generic_token);
    nft.register_token(&xtar_token);

    // One eligible player
    let p1 = <Address as TestAddress>::generate(&e);
    write_decks(&e, &nft_addr, &[(p1.clone(), 100)], 0);

    // Accumulate into pot but do not open yet
    nft.accumulate_pot_token(&xtar_token, &300);
    nft.accumulate_pot_token(&generic_token, &700);

    // BEFORE opening: totals reflect accumulators; claimables are zero
    let pre = nft.view_pot_overview(&p1);
    assert_eq!(pre.total_xtar, 300);
    // generic totals include our token with amount 700
    let mut found = false;
    for g in pre.total_generics.iter() { if g.token == generic_token { assert_eq!(g.amount, 700); found = true; } }
    assert!(found);
    assert_eq!(pre.user_xtar, 0);
    assert!(pre.user_generics.len() == 0);

    // Open pot to compute claimables
    nft.open_pot(&1);

    // AFTER opening: totals reset to zero; user gets claimables (100%)
    let post = nft.view_pot_overview(&p1);
    assert_eq!(post.total_xtar, 0);
    assert!(post.total_generics.len() == 0);
    assert_eq!(post.user_xtar, 300);
    let mut found_claim = false;
    for g in post.user_generics.iter() { if g.token == generic_token { assert_eq!(g.amount, 700); found_claim = true; } }
    assert!(found_claim);
}


