use core::mem::MaybeUninit;

use pinocchio::{
    AccountView, ProgramResult,
    cpi::{Seed, Signer, invoke_signed_with_bounds},
    error::ProgramError,
    instruction::{InstructionAccount, InstructionView},
};
use winterwallet_core::{WinternitzRoot, WinternitzSignature};

use crate::{
    MAX_CPI_INSTRUCTION_ACCOUNTS, MAX_PASSTHROUGH_ACCOUNTS, WINTERNITZ_SCALARS,
    WINTERWALLET_ADVANCE, WinterWallet,
};

pub struct Advance<'a> {
    wallet: &'a mut AccountView,
    accounts: &'a mut [AccountView],
    signature: &'a WinternitzSignature<WINTERNITZ_SCALARS>,
    root: &'a [u8; 32],
    payload: &'a [u8],
}

impl<'a> TryFrom<(&'a mut [AccountView], &'a [u8])> for Advance<'a> {
    type Error = ProgramError;

    #[inline(always)]
    fn try_from(inputs: (&'a mut [AccountView], &'a [u8])) -> Result<Self, Self::Error> {
        let [wallet, accounts @ ..] = inputs.0 else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };

        let (sig_bytes, rest) = inputs
            .1
            .split_first_chunk::<{ 32 * (WINTERNITZ_SCALARS + 2) }>()
            .ok_or(ProgramError::InvalidInstructionData)?;
        let signature: &WinternitzSignature<WINTERNITZ_SCALARS> =
            sig_bytes
                .as_slice()
                .try_into()
                .map_err(|_| ProgramError::InvalidInstructionData)?;
        let (root, payload) = rest
            .split_first_chunk::<32>()
            .ok_or(ProgramError::InvalidInstructionData)?;

        Ok(Self {
            wallet,
            accounts,
            signature,
            root,
            payload,
        })
    }
}

impl<'a> Advance<'a> {
    #[inline(always)]
    pub fn process(accounts: &'a mut [AccountView], instruction_data: &'a [u8]) -> ProgramResult {
        Self::try_from((accounts, instruction_data))?.execute()
    }

    #[inline(always)]
    pub fn execute(&mut self) -> ProgramResult {
        // Snapshot our Wallet ID, root, and bump. Immediately advance the
        // stored root before CPI so the same signature cannot be replayed
        // if a sub-instruction were to re-enter our program somehow.
        let (id, current_root, bump) = {
            let wallet_account: &mut WinterWallet = (&mut *self.wallet).try_into()?;
            let current_root = wallet_account.root;
            wallet_account.root = WinternitzRoot::new(*self.root);
            (wallet_account.id, current_root, wallet_account.bump)
        };

        // Cached for the signer-promotion check inside the CPI loop.
        let wallet_address = *self.wallet.address().as_array();

        self.verify_signature(&id, &current_root)?;

        let seeds = [
            Seed::from(b"winterwallet"),
            Seed::from(&id),
            Seed::from(&bump),
        ];
        let signers = [Signer::from(&seeds)];

        let mut payload: &[u8] = self.payload;
        let num_instructions = read_u8(&mut payload)? as usize;
        let mut cursor: usize = 0;
        for _ in 0..num_instructions {
            invoke_next(
                &mut payload,
                self.accounts,
                &mut cursor,
                &signers,
                &wallet_address,
            )?;
        }

        if !payload.is_empty() || cursor != self.accounts.len() {
            return Err(ProgramError::InvalidInstructionData);
        }

        Ok(())
    }

    /// Recover the pubkey implied by the signature over the canonical Advance
    /// preimage and check its merklized root matches the wallet's stored root.
    ///
    /// Preimage parts (concatenated under `solana_sha256_hasher::hashv`):
    /// `WINTERWALLET_ADVANCE`, wallet id, current root, new root, every
    /// trailing account address in order, then the raw replay payload.
    #[inline(never)]
    fn verify_signature(&self, id: &[u8; 32], current_root: &WinternitzRoot) -> ProgramResult {
        if self.accounts.len() > MAX_PASSTHROUGH_ACCOUNTS {
            return Err(ProgramError::InvalidArgument);
        }

        // 4 leading parts (tag, id, current root, new root) + N account
        // addresses + 1 trailing payload.
        let mut parts: [MaybeUninit<&[u8]>; 5 + MAX_PASSTHROUGH_ACCOUNTS] =
            [const { MaybeUninit::uninit() }; 5 + MAX_PASSTHROUGH_ACCOUNTS];
        parts[0].write(WINTERWALLET_ADVANCE);
        parts[1].write(id);
        parts[2].write(current_root.as_bytes());
        parts[3].write(self.root);
        let mut idx = 4;
        for acc in self.accounts.iter() {
            parts[idx].write(acc.address().as_array());
            idx += 1;
        }
        parts[idx].write(self.payload);
        idx += 1;

        // SAFETY: parts[..idx] were each written exactly once above, and
        // `MaybeUninit<&[u8]>` shares layout with `&[u8]`.
        let parts_init: &[&[u8]] =
            unsafe { core::slice::from_raw_parts(parts.as_ptr() as *const &[u8], idx) };

        if !self.signature.verify(parts_init, current_root) {
            return Err(ProgramError::MissingRequiredSignature);
        }
        Ok(())
    }
}

/// Decode the next compiled instruction from `payload`, consume its accounts
/// from `remaining` via `cursor`, and CPI-invoke it under `signers`.
fn invoke_next(
    payload: &mut &[u8],
    remaining: &[AccountView],
    cursor: &mut usize,
    signers: &[Signer],
    wallet_address: &[u8; 32],
) -> ProgramResult {
    let num_accounts = read_u8(payload)? as usize;
    let data_len = read_u16(payload)? as usize;

    if num_accounts > MAX_CPI_INSTRUCTION_ACCOUNTS || payload.len() < data_len {
        return Err(ProgramError::InvalidInstructionData);
    }
    let (data, rest) = payload.split_at(data_len);
    *payload = rest;

    let end = cursor
        .checked_add(1 + num_accounts)
        .ok_or(ProgramError::InvalidInstructionData)?;
    if end > remaining.len() {
        return Err(ProgramError::NotEnoughAccountKeys);
    }
    let program_view = &remaining[*cursor];
    if !program_view.executable() {
        return Err(ProgramError::IncorrectProgramId);
    }
    let account_views = &remaining[*cursor + 1..end];
    *cursor = end;

    let mut metas: [MaybeUninit<InstructionAccount>; MAX_CPI_INSTRUCTION_ACCOUNTS] =
        [const { MaybeUninit::uninit() }; MAX_CPI_INSTRUCTION_ACCOUNTS];
    for (slot, view) in metas.iter_mut().zip(account_views) {
        let mut meta = InstructionAccount::from(view);
        // Promote wallet appearances to signer so the inner instruction sees
        // the PDA signature provided by `invoke_signed_with_bounds`.
        if view.address().as_array() == wallet_address {
            meta.is_signer = true;
        }
        slot.write(meta);
    }
    // SAFETY: the loop initialised exactly `num_accounts` entries, and
    // `MaybeUninit<InstructionAccount>` shares layout with `InstructionAccount`.
    let metas_slice: &[InstructionAccount] =
        unsafe { core::slice::from_raw_parts(metas.as_ptr().cast(), num_accounts) };

    let instruction = InstructionView {
        program_id: program_view.address(),
        accounts: metas_slice,
        data,
    };

    invoke_signed_with_bounds::<MAX_CPI_INSTRUCTION_ACCOUNTS, AccountView>(
        &instruction,
        account_views,
        signers,
    )
}

#[inline(always)]
fn read_u8(payload: &mut &[u8]) -> Result<u8, ProgramError> {
    let (first, rest) = payload
        .split_first()
        .ok_or(ProgramError::InvalidInstructionData)?;
    *payload = rest;
    Ok(*first)
}

#[inline(always)]
fn read_u16(payload: &mut &[u8]) -> Result<u16, ProgramError> {
    let (chunk, rest) = payload
        .split_first_chunk::<2>()
        .ok_or(ProgramError::InvalidInstructionData)?;
    *payload = rest;
    Ok(u16::from_le_bytes(*chunk))
}
