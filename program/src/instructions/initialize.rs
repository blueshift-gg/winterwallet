use pinocchio::{
    AccountView, Address, ProgramResult,
    cpi::{Seed, Signer},
    error::ProgramError,
    sysvars::{Sysvar, rent::Rent},
};
use pinocchio_system::instructions::{Allocate, Assign, CreateAccount, Transfer};
use winterwallet_core::{WinternitzRoot, WinternitzSignature};

use crate::{WINTERNITZ_SCALARS, WINTERWALLET_INITIALIZE, WinterWallet};

pub struct Initialize<'a> {
    payer: &'a mut AccountView,
    wallet: &'a mut AccountView,
    signature: &'a WinternitzSignature<WINTERNITZ_SCALARS>,
    root: &'a [u8; 32], // Next wallet address
}

impl<'a> TryFrom<(&'a mut [AccountView], &'a [u8])> for Initialize<'a> {
    type Error = ProgramError;

    #[inline(always)]
    fn try_from(inputs: (&'a mut [AccountView], &'a [u8])) -> Result<Self, Self::Error> {
        let [payer, wallet, _system_program] = inputs.0 else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };

        // Safety: We can skip these checks
        // if !payer.is_signer() {
        //     return Err(ProgramError::MissingRequiredSignature);
        // }
        // if !payer.is_writable() {
        //     return Err(ProgramError::InvalidAccountData);
        // }

        // Ensure wallet address is uninitialized
        if !wallet.owned_by(&pinocchio_system::ID) {
            return Err(ProgramError::InvalidAccountOwner);
        }

        let (sig_bytes, root_bytes) = inputs
            .1
            .split_first_chunk::<{ 32 * 24 }>()
            .ok_or(ProgramError::InvalidInstructionData)?;
        let signature: &WinternitzSignature<WINTERNITZ_SCALARS> =
            sig_bytes
                .as_slice()
                .try_into()
                .map_err(|_| ProgramError::InvalidInstructionData)?;
        let root: &[u8; 32] = root_bytes
            .try_into()
            .map_err(|_| ProgramError::InvalidInstructionData)?;

        Ok(Self {
            payer,
            wallet,
            signature,
            root,
        })
    }
}

impl<'a> Initialize<'a> {
    #[inline(always)]
    pub fn process(accounts: &'a mut [AccountView], instruction_data: &'a [u8]) -> ProgramResult {
        Self::try_from((accounts, instruction_data))?.execute()
    }

    #[inline(always)]
    pub fn execute(&mut self) -> ProgramResult {
        // Recover the ID of the Winternitz signer
        let id = self.recover_initialize_id();

        // Verify wallet address and calculate bump
        let (wallet_address, bump) =
            Address::derive_program_address(&[b"winterwallet", id.as_bytes()], &crate::ID)
                .ok_or(ProgramError::InvalidSeeds)?;

        if wallet_address.ne(self.wallet.address()) {
            return Err(ProgramError::InvalidSeeds);
        }

        // Setup wallet signer seeds
        let binding = [bump];

        let seeds = [
            Seed::from(b"winterwallet"),
            Seed::from(id.as_bytes()),
            Seed::from(&binding),
        ];

        let signers = [Signer::from(&seeds)];

        // Create wallet account
        self.create_account(&signers)?;

        // Initialize WinterWallet account
        WinterWallet::initialize(self.wallet, id.as_bytes(), self.root, bump)
    }

    fn recover_initialize_id(&self) -> WinternitzRoot {
        let pubkey = self.signature.recover_pubkey(&[WINTERWALLET_INITIALIZE]);
        pubkey.merklize()
    }

    fn create_account(&self, signers: &[Signer]) -> ProgramResult {
        let lamports = self.wallet.lamports();

        let rent_exempt_lamports = Rent::get()?.try_minimum_balance(WinterWallet::LEN)?;

        if lamports == 0 {
            CreateAccount {
                from: self.payer,
                to: self.wallet,
                lamports: rent_exempt_lamports,
                space: WinterWallet::LEN as u64,
                owner: &crate::ID,
            }
            .invoke_signed(signers)
        } else {
            // Transfer remaining rent exempt lamports if required
            if lamports < rent_exempt_lamports {
                Transfer {
                    from: self.payer,
                    to: self.wallet,
                    lamports: rent_exempt_lamports.saturating_sub(lamports),
                }
                .invoke_signed(signers)?;
            }
            // Allocate space for WinterWallet account
            Allocate {
                account: self.wallet,
                space: WinterWallet::LEN as u64,
            }
            .invoke_signed(signers)?;

            // Assign account to WinterWallet program
            Assign {
                account: self.wallet,
                owner: &crate::ID,
            }
            .invoke_signed(signers)
        }
    }
}
