use crate::nft_info::Category;
use soroban_sdk::{contracttype, Address, String, Vec};

pub(crate) const DAY_IN_LEDGERS: u32 = 17280;
pub(crate) const INSTANCE_BUMP_AMOUNT: u32 = 7 * DAY_IN_LEDGERS;
pub(crate) const INSTANCE_LIFETIME_THRESHOLD: u32 = INSTANCE_BUMP_AMOUNT - DAY_IN_LEDGERS;

pub(crate) const BALANCE_BUMP_AMOUNT: u32 = 30 * DAY_IN_LEDGERS;
pub(crate) const BALANCE_LIFETIME_THRESHOLD: u32 = BALANCE_BUMP_AMOUNT - DAY_IN_LEDGERS;

pub(crate) const LEDGERS_PER_DAY: u32 = 17280;
pub(crate) const STORAGE_BUMP_LEDGERS: u32 = 180 * LEDGERS_PER_DAY; // ~180 days ~3,110,400
pub(crate) const STORAGE_THRESHOLD_LEDGERS: u32 = 30 * LEDGERS_PER_DAY; // ~30 days ~518,400

#[derive(Debug, Clone, PartialEq)]
#[contracttype]
pub struct TokenId(pub u32);

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct User {
    pub owner: Address,
    pub power: u32,
    pub terry: i128,
    pub total_history_terry: i128,
    pub level: u32,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct Level {
    pub minimum_terry: i128,
    pub maximum_terry: i128,
    pub name: String,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct PotBalance {
    pub accumulated_terry: i128,
    pub accumulated_power: u32,
    pub accumulated_xtar: i128,
    pub last_opening_round: u32,
    pub total_openings: u32, // Number of times the pot has been opened
    pub last_updated: u64,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct PlayerReward {
    pub share_percentage: u32,
    pub effective_power: u32,
    pub round_number: u32,
    pub deck_bonus: u32, // Bonus from the deck diversity
    pub deck_categories: u32,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct PotSnapshot {
    pub round_number: u32,
    pub total_terry: i128,
    pub total_power: u32,
    pub total_xtar: i128,
    pub timestamp: u64,
    pub total_participants: u32,
    pub total_effective_power: u32,
    pub generic_tokens: Vec<GenericTokenAmount>,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct PendingReward {
    pub round_number: u32,
    pub terry_amount: i128,
    pub power_amount: u32,
    pub xtar_amount: i128,
    pub generic_tokens: Vec<GenericTokenAmount>,
    pub status: RewardStatus,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub enum RewardStatus {
    Pending,
    AwaitingTrustLine,
    Claimed,
    Failed,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct RewardClaim {
    pub player: Address,
    pub rewards: Vec<PendingReward>,
    pub claimed_at: u64,
}

#[derive(Clone)]
#[contracttype]
pub struct DogstarBalance {
    pub terry: i128,
    pub power: u32,
    pub xtar: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractVault {
    pub haw_ai_pot_terry: i128,
    pub haw_ai_pot_power: u32,
    pub haw_ai_pot_xtar: i128,
    pub dogstar_terry: i128,
    pub dogstar_power: u32,
    pub dogstar_xtar: i128,
    pub total_claimable_terry: i128,
    pub total_claimable_power: u32,
    pub total_claimable_xtar: i128,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct UserClaimableBalance {
    pub terry: i128,
    pub power: u32,
    pub xtar: i128,
    pub last_claim_round: u32,
    pub last_claim_timestamp: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GenericTokenAmount {
    pub token: Address,
    pub amount: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PotOverview {
    // Current totals accumulated in pot since last opening
    pub total_terry: i128,
    pub total_power: u32,
    pub total_xtar: i128,
    pub total_generics: Vec<GenericTokenAmount>,
    // Per-user claimable amounts (not yet claimed)
    pub user_terry: i128,
    pub user_power: u32,
    pub user_xtar: i128,
    pub user_generics: Vec<GenericTokenAmount>,
}

#[contracttype]
#[derive(Clone, Eq, PartialEq)]
pub struct Deck {
    pub owner: Address,
    pub token_ids: Vec<TokenId>,
    pub total_power: u32,
    pub haw_ai_percentage: u32,
    pub bonus: u32,
    pub deck_categories: u32, // Number of unique categories (1–4)
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Config {
    pub xtar_token: Address,
    pub oracle_contract_id: Address,
    pub withdrawable_percentage: u32,
    pub burnable_percentage: u32,
    pub haw_ai_percentage: u32,
    pub terry_per_power: i128,
    pub stake_periods: Vec<u32>,
    pub stake_interest_percentages: Vec<u32>,
    pub power_action_fee: u32,
    pub burn_receive_percentage: u32,
    pub terry_per_deck: i128,
    pub terry_per_fight: i128,
    pub terry_per_lending: i128,
    pub terry_per_stake: i128,
    pub apy_alpha: u32,
    pub power_to_usdc_rate: i128, // e.g., 1000 for 0.10 USDC per POWER (1000/10000 = 0.10)
    pub dogstar_fee_percentage: u32, // Basis points (e.g., 500 = 5%)
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Balance {
    pub admin_terry: i128,
    pub admin_power: u32,
    pub haw_ai_terry: i128,
    pub haw_ai_power: u32,
    pub haw_ai_xtar: i128,
    pub total_deck_power: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct State {
    pub total_offer: u64,
    pub total_demand: u64,
    pub total_interest: u64,
    pub total_loan_duration: u64,
    pub total_loan_count: u64,
    pub total_staked_power: u64,
    pub total_borrowed_power: u64,
    // Realtime accumulators (seconds-based)
    pub borrowed_time_seconds: u64,
    pub loans_time_seconds: u64,
    pub last_update_ts: u64,
    pub active_loans: u64,
    // Lazy pro‑rata liquidation index and total weight (sum of reserve_remaining)
    pub l_index: u64, // fixed-point SCALE=1e6
    pub w_total: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BorrowMeta {
    pub last_l_index: u64,      // last applied liquidation index (SCALE)
    pub weight: u32,            // equals reserve_remaining at last update
    pub reserve_remaining: u32, // remaining reserve earmarked for interest/haircuts
}

#[derive(Clone, Debug)]
#[contracttype]
pub enum DataKey {
    // Administration and configuration
    Admin,
    Config,
    // Logical balances since last opening, per asset
    PotBalance,
    Balance,
    BalanceSC(Category),
    DogstarBalance,
    ContractVault,
    // Global state and whitelists
    State,
    Whitelist(Address),
    User(Address),
    OwnerOwnedCardIds(Address),
    Card(Address, TokenId),
    AllCardIds,
    Decks,
    Deck(Address),
    Stakes,
    Stake(Address, Category, TokenId),
    Lendings,
    Lending(Address, Category, TokenId),
    Borrowings,
    Borrowing(Address, Category, TokenId),
    BorrowMeta(Address, Category, TokenId),
    Fights,
    Fight(Address, Category, TokenId),
    // NFT metadata and ID management
    LevelId,
    Level(u32),
    TokenId(u32),
    TokenIdCounter,
    Metadata(TokenId),
    // Pot-specific snapshots and reward tracking
    /// Total snapshot per asset at pot opening for round
    PotSnapshotAsset(u32, TokenId),
    PotSnapshotSC(u32, Category),
    OpeningSnapshot(u32),
    PlayerShare(u32, Address),
    PlayerPower(u32, Address),
    PendingReward(u32, Address),
    UserClaimableBalance(Address),
    DogstarClaimableBalance,
    CurrentRound,
    AllRounds,
    RewardClaim(Address, u64),
    // Multi‑asset pot extensions
    RegisteredTokens,
    AccumulatedByToken(Address),
    UserGenericClaimable(Address, Address),
}
