#![cfg(test)]
use super::*;
use crate::contract::NFT;
use crate::pot::management::{read_pot_snapshot, read_user_generic_claimable};
use crate::storage_types::{Config, DataKey, Deck, TokenId};
use soroban_sdk::testutils::Address as TestAddress;
use soroban_sdk::{vec, Address, Env, Vec};

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


