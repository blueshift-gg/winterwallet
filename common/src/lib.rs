#![cfg_attr(any(target_os = "solana", target_arch = "bpf"), no_std)]

//! Shared constants, discriminators, domain tags, and account state layout
//! for the WinterWallet program. Used by both the on-chain program and
//! off-chain clients to eliminate constant drift.

use solana_address::declare_id;

declare_id!("winter5vMwvf51xrSVPTxbAAD6qiSTmPeRTSizMCQCa");

// ── Winternitz parameters ────────────────────────────────────────────

/// Number of Winternitz message scalars (N). Total scalars = N + 2.
pub const WINTERNITZ_SCALARS: usize = 22;

/// Total scalars including the two checksum scalars.
pub const TOTAL_SCALARS: usize = WINTERNITZ_SCALARS + 2;

/// Signature byte length: `(N + 2) * 32`.
pub const SIGNATURE_LEN: usize = TOTAL_SCALARS * 32;

// ── Domain tags ──────────────────────────────────────────────────────

/// Domain tag for the Initialize preimage.
pub const WINTERWALLET_INITIALIZE: &[u8] = b"WINTERWALLET_INITIALIZE";

/// Domain tag for the Advance preimage.
pub const WINTERWALLET_ADVANCE: &[u8] = b"WINTERWALLET_ADVANCE";

// ── PDA ──────────────────────────────────────────────────────────────

/// PDA seed prefix.
pub const WINTERWALLET_SEED: &[u8] = b"winterwallet";

// ── Instruction discriminators ───────────────────────────────────────

/// Instruction discriminator bytes matching the on-chain `match` arms.
pub mod discriminator {
    /// Initialize a new WinterWallet.
    pub const INITIALIZE: u8 = 0;
    /// Advance the wallet root and execute inner CPIs.
    pub const ADVANCE: u8 = 1;
    /// Withdraw lamports (inner CPI via Advance).
    pub const WITHDRAW: u8 = 2;
    /// Close the wallet, sweeping all lamports to a receiver (inner CPI via Advance).
    pub const CLOSE: u8 = 3;
}

// ── Advance limits ───────────────────────────────────────────────────

/// Upper bound on trailing accounts an Advance instruction can commit to.
/// Sizes the stack-allocated signature-preimage buffer used during recovery.
pub const MAX_PASSTHROUGH_ACCOUNTS: usize = 128;

/// Upper bound on account metas per inner CPI'd instruction inside Advance.
pub const MAX_CPI_INSTRUCTION_ACCOUNTS: usize = 16;

// ── Account state layout ─────────────────────────────────────────────

/// On-chain WinterWallet account data length: `id(32) + root(32) + bump(1)`.
pub const WALLET_ACCOUNT_LEN: usize = 65;

/// Byte offset of the `id` field in the WinterWallet account.
pub const WALLET_ID_OFFSET: usize = 0;
/// Byte offset of the `root` field in the WinterWallet account.
pub const WALLET_ROOT_OFFSET: usize = 32;
/// Byte offset of the `bump` field in the WinterWallet account.
pub const WALLET_BUMP_OFFSET: usize = 64;
