use pinocchio::{AccountView, ProgramResult, error::ProgramError};
use winterwallet_core::WinternitzRoot;

#[repr(C)]
pub struct WinterWallet {
    pub id: [u8;32], // Wallet ID
    pub root: WinternitzRoot, // Next Address root
    pub bump: [u8;1],
}

impl WinterWallet {
    pub const LEN: usize = 64 + 8 + 1;
}

impl<'a> TryFrom<&'a mut AccountView> for &'a mut WinterWallet {
    type Error = ProgramError;

    #[inline(always)]
    fn try_from(account: &'a mut AccountView) -> Result<Self, Self::Error> {
        let mut reference = account.try_borrow_mut()?;
        
        let bytes: &mut [u8;WinterWallet::LEN] = reference.as_mut().try_into().map_err(|_| ProgramError::InvalidAccountData)?;

        let wallet: &mut WinterWallet = unsafe { core::mem::transmute(bytes) };

        Ok(wallet)
    }
}


impl WinterWallet {
    pub fn initialize(account: &mut AccountView, id: &[u8;32], root: &[u8;32], bump: u8) -> ProgramResult {
        let wallet_account: &mut Self = account.try_into()?;

        *wallet_account = WinterWallet {
            id: *id,
            root: WinternitzRoot::new(*root),
            bump: [bump]
        };

        Ok(())
    }
}
