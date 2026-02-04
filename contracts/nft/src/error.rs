use soroban_sdk::{self, contracterror};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum MyLabError {
    NotOwner = 300,
    NotNFT = 301,
    NotAuthorized = 302,
    OutOfBounds = 303,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum NFTError {
    AlreadyInitialized = 1,
    RoundAlreadyProcessed = 2,
    NotAuthorized = 3,
    NoRewardsAvailable = 4,
    PotTooLarge = 5,
    PotInProgress = 6,
}
