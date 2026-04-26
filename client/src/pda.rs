use solana_address::Address;
use winterwallet_common::{ID, WINTERWALLET_SEED};

/// Derive the WinterWallet PDA from a wallet ID.
///
/// This wraps [`Address::find_program_address`] and searches for the valid
/// bump seed. The on-chain program uses `derive_program_address` with the
/// stored bump. Use this when deriving the PDA for the first time or when
/// the bump is unknown.
pub fn find_wallet_address(wallet_id: &[u8; 32]) -> (Address, u8) {
    Address::find_program_address(&[WINTERWALLET_SEED, wallet_id], &ID)
}

/// Derive the wallet ID from a mnemonic at wallet index 0.
///
/// The wallet ID is the merklized root of the Winternitz pubkey at the
/// initial derivation position `(wallet=0, parent=0, child=0)`. This is
/// the value stored on-chain as `WinterWallet.id` and used as a PDA seed.
pub fn wallet_id_from_mnemonic(mnemonic: &str) -> Result<[u8; 32], crate::Error> {
    let keypair = winterwallet_core::WinternitzKeypair::from_mnemonic(mnemonic, 0)?;
    let privkey = keypair.derive::<{ winterwallet_common::WINTERNITZ_SCALARS }>();
    let pubkey = privkey.to_pubkey();
    Ok(*pubkey.merklize().as_bytes())
}
