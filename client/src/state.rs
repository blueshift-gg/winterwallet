use winterwallet_common::{
    WALLET_ACCOUNT_LEN, WALLET_BUMP_OFFSET, WALLET_ID_OFFSET, WALLET_ROOT_OFFSET,
};
use winterwallet_core::WinternitzRoot;

/// Deserialized WinterWallet account state.
///
/// Mirrors the on-chain `WinterWallet` layout: `id(32) + root(32) + bump(1)`.
#[repr(C)]
pub struct WinterWalletAccount {
    /// Wallet ID — the merklized root of the initial Winternitz pubkey.
    pub id: [u8; 32],
    /// Current Winternitz root the wallet expects for signature verification.
    pub root: WinternitzRoot,
    /// PDA bump seed.
    pub bump: [u8; 1],
}

// Compile-time check that our layout matches the shared constant.
const _: () = assert!(core::mem::size_of::<WinterWalletAccount>() == WALLET_ACCOUNT_LEN);

impl WinterWalletAccount {
    /// Deserialize from raw account data bytes.
    ///
    /// Requires exactly [`WALLET_ACCOUNT_LEN`] (65) bytes.
    pub fn from_bytes(data: &[u8]) -> Result<Self, crate::Error> {
        if data.len() != WALLET_ACCOUNT_LEN {
            return Err(crate::Error::InvalidAccountData);
        }
        let id: [u8; 32] = data[WALLET_ID_OFFSET..WALLET_ID_OFFSET + 32]
            .try_into()
            .unwrap();
        let root_bytes: [u8; 32] = data[WALLET_ROOT_OFFSET..WALLET_ROOT_OFFSET + 32]
            .try_into()
            .unwrap();
        let bump = data[WALLET_BUMP_OFFSET];
        Ok(Self {
            id,
            root: WinternitzRoot::new(root_bytes),
            bump: [bump],
        })
    }
}
