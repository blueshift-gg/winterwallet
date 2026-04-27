//! Off-chain client for the WinterWallet program.
//!
//! Provides instruction builders, PDA derivation, preimage construction,
//! CPI payload encoding, and on-chain state deserialization.
//!
//! # Example: Build an Advance(Withdraw) plan
//!
//! ```ignore
//! use winterwallet_client::*;
//! use winterwallet_common::WINTERNITZ_SCALARS;
//!
//! // 1. Build a plan. It owns the payload + account order.
//! let plan = AdvancePlan::withdraw(&wallet_pda, &receiver, lamports, &new_root)?;
//!
//! // 2. Compute preimage and sign.
//! let preimage = plan.preimage(&id, current_root);
//! let sig = keypair.sign_and_increment::<WINTERNITZ_SCALARS>(&preimage);
//!
//! // 3. Build the Advance instruction.
//! let ix = plan.instruction(sig.as_bytes().try_into().unwrap());
//! ```

mod error;
pub mod instruction;
mod pda;
mod plan;
mod preimage;
mod state;
pub mod transaction;
mod wallet;

pub use error::Error;
pub use instruction::{AdvancePayload, advance, close, encode_advance, initialize, withdraw};
pub use pda::{find_wallet_address, wallet_id_from_mnemonic};
pub use plan::AdvancePlan;
pub use preimage::{advance_preimage, initialize_preimage};
pub use state::WinterWalletAccount;
pub use transaction::{
    AccountEntry, DEFAULT_ADVANCE_COMPUTE_UNIT_LIMIT, LEGACY_TRANSACTION_SIZE_LIMIT,
    estimate_legacy_transaction_size, set_compute_unit_limit, set_compute_unit_price, upsert,
    validate_legacy_transaction_size, validate_payer_only_signers, with_compute_budget,
};
pub use wallet::{
    AdvancePersistence, AdvanceSender, PersistedAdvance, SignedAdvance, SigningPosition,
    UnsignedAdvance, WinterWallet, token_transfer,
};

// Re-export commonly used items from winterwallet-common for convenience.
pub use winterwallet_common::{
    ID, MAX_CPI_INSTRUCTION_ACCOUNTS, MAX_PASSTHROUGH_ACCOUNTS, SIGNATURE_LEN, TOTAL_SCALARS,
    WALLET_ACCOUNT_LEN, WINTERNITZ_SCALARS, WINTERWALLET_ADVANCE, WINTERWALLET_INITIALIZE,
    WINTERWALLET_SEED, discriminator,
};
