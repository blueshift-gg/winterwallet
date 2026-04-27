use pinocchio::{AccountView, ProgramResult, error::ProgramError};

pub struct Close<'a> {
    wallet: &'a mut AccountView,
    receiver: &'a mut AccountView,
}

impl<'a> TryFrom<(&'a mut [AccountView], &'a [u8])> for Close<'a> {
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

        // Error if `wallet` and `receiver` are the same account to prevent
        // permanent destroying our lamports.
        if wallet.address().eq(receiver.address()) {
            return Err(ProgramError::InvalidAccountData);
        }

        // Safety: We can technically ignore on-curve keypairs assigned to our
        // program invoking `close` as a top-level instruction, as it isn't
        // dangerous, however this is a simple way to stop them from doing so.
        if wallet.data_len().eq(&0) {
            return Err(ProgramError::InvalidAccountData);
        }

        if !inputs.1.is_empty() {
            return Err(ProgramError::InvalidInstructionData);
        }

        Ok(Self { wallet, receiver })
    }
}

impl<'a> Close<'a> {
    #[inline(always)]
    pub fn process(accounts: &'a mut [AccountView], instruction_data: &'a [u8]) -> ProgramResult {
        Self::try_from((accounts, instruction_data))?.execute()
    }

    #[inline(always)]
    pub fn execute(&mut self) -> ProgramResult {
        let balance = self.wallet.lamports();
        self.receiver
            .set_lamports(self.receiver.lamports().saturating_add(balance));
        // Zero data_len, lamports, and reassign owner to System so a follow-up
        // CPI in the same tx that re-funds the PDA can't revive the wallet
        // with stale id/root/bump still owned by us.
        self.wallet.close()
    }
}
