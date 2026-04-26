use pinocchio::{AccountView, ProgramResult, error::ProgramError};
use winterwallet_core::WinternitzRoot;

#[repr(C)]
pub struct WinterWallet {
    pub id: [u8; 32],         // Wallet ID
    pub root: WinternitzRoot, // Next Address root
    pub bump: [u8; 1],
}

impl WinterWallet {
    pub const LEN: usize = core::mem::size_of::<Self>();
}

// Compile-time check that our layout matches the shared constant.
const _: () = assert!(WinterWallet::LEN == winterwallet_common::WALLET_ACCOUNT_LEN);

/// Reborrow a `WinterWallet` view over an `AccountView`'s data.
///
/// **Invariant:** the runtime borrow flag is released as soon as the
/// `RefMut` local drops (at the end of this function), so the returned
/// `&mut WinterWallet` outlives the runtime borrow check. Don't call
/// `try_into` again on the same account while the previous reference is
/// alive, and don't hand the account to a CPI while it's alive — keep the
/// reborrow scoped to a single block where all reads/writes happen.
impl<'a> TryFrom<&'a mut AccountView> for &'a mut WinterWallet {
    type Error = ProgramError;

    #[inline(always)]
    fn try_from(account: &'a mut AccountView) -> Result<Self, Self::Error> {
        let mut reference = account.try_borrow_mut()?;

        let bytes: &mut [u8; WinterWallet::LEN] = reference
            .as_mut()
            .try_into()
            .map_err(|_| ProgramError::InvalidAccountData)?;

        let wallet: &mut WinterWallet = unsafe { core::mem::transmute(bytes) };

        Ok(wallet)
    }
}

impl WinterWallet {
    pub fn initialize(
        account: &mut AccountView,
        id: &[u8; 32],
        root: &[u8; 32],
        bump: u8,
    ) -> ProgramResult {
        let wallet_account: &mut Self = account.try_into()?;

        *wallet_account = WinterWallet {
            id: *id,
            root: WinternitzRoot::new(*root),
            bump: [bump],
        };

        Ok(())
    }
}
