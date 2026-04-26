use pinocchio::{
    AccountView, ProgramResult,
    error::ProgramError,
    sysvars::{Sysvar, rent::Rent},
};

use crate::WinterWallet;

pub struct Withdraw<'a> {
    wallet: &'a mut AccountView,
    receiver: &'a mut AccountView,
    lamports: u64,
}

impl<'a> TryFrom<(&'a mut [AccountView], &'a [u8])> for Withdraw<'a> {
    type Error = ProgramError;

    #[inline(always)]
    fn try_from(inputs: (&'a mut [AccountView], &'a [u8])) -> Result<Self, Self::Error> {
        let [wallet, receiver] = inputs.0 else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };

        // Ensure `wallet` account is a signer
        if !wallet.is_signer() {
            return Err(ProgramError::MissingRequiredSignature);
        }

        // Safety: We can technically ignore on-curve keypairs assigned to our
        // program invoking `withdraw` as a top-level instruction, as it isn't
        // dangerous, however this is a simple way to stop them from doing so.
        if wallet.data_len() == 0 {
            return Err(ProgramError::InvalidAccountData);
        }

        let lamports: u64 = u64::from_le_bytes(
            inputs
                .1
                .try_into()
                .map_err(|_| ProgramError::InvalidInstructionData)?,
        );

        Ok(Self {
            wallet,
            receiver,
            lamports,
        })
    }
}

impl<'a> Withdraw<'a> {
    #[inline(always)]
    pub fn process(accounts: &'a mut [AccountView], instruction_data: &'a [u8]) -> ProgramResult {
        Self::try_from((accounts, instruction_data))?.execute()
    }

    #[inline(always)]
    pub fn execute(&mut self) -> ProgramResult {
        let balance = self.wallet.lamports();

        // Reject withdrawals that would bring the account below rent-exempt.
        let rent_exempt = Rent::get()?.try_minimum_balance(WinterWallet::LEN)?;
        if balance.saturating_sub(self.lamports) < rent_exempt {
            return Err(ProgramError::InsufficientFunds);
        }

        self.wallet.set_lamports(balance - self.lamports);
        self.receiver
            .set_lamports(self.receiver.lamports().saturating_add(self.lamports));
        Ok(())
    }
}
