use solana_address::Address;
use solana_instruction::{AccountMeta, Instruction};
use winterwallet_common::{
    ID, MAX_CPI_INSTRUCTION_ACCOUNTS, MAX_PASSTHROUGH_ACCOUNTS, SIGNATURE_LEN, WINTERNITZ_SCALARS,
    discriminator,
};

use crate::Error;

// Compile-time check that SIGNATURE_LEN matches the core type size.
const _: () = assert!(
    SIGNATURE_LEN == (WINTERNITZ_SCALARS + 2) * 32,
    "SIGNATURE_LEN must equal (WINTERNITZ_SCALARS + 2) * 32"
);

// ── Instruction builders ─────────────────────────────────────────────

/// Build an Initialize instruction.
///
/// Accounts: `[payer (signer, writable), wallet_pda (writable), system_program]`.
pub fn initialize(
    payer: &Address,
    wallet_pda: &Address,
    signature_bytes: &[u8; SIGNATURE_LEN],
    next_root: &[u8; 32],
) -> Instruction {
    let mut data = Vec::with_capacity(1 + SIGNATURE_LEN + 32);
    data.push(discriminator::INITIALIZE);
    data.extend_from_slice(signature_bytes);
    data.extend_from_slice(next_root);

    Instruction {
        program_id: ID,
        accounts: vec![
            AccountMeta::new(*payer, true),
            AccountMeta::new(*wallet_pda, false),
            AccountMeta::new_readonly(solana_system_interface::program::id(), false),
        ],
        data,
    }
}

/// Build an Advance instruction from pre-encoded payload and account list.
///
/// Use [`encode_advance`] to produce `payload` and `passthrough_accounts`
/// atomically from inner instructions. Passing hand-crafted values risks
/// ordering mismatches.
pub fn advance(
    wallet_pda: &Address,
    passthrough_accounts: &[AccountMeta],
    signature_bytes: &[u8; SIGNATURE_LEN],
    new_root: &[u8; 32],
    payload: &[u8],
) -> Instruction {
    let mut data = Vec::with_capacity(1 + SIGNATURE_LEN + 32 + payload.len());
    data.push(discriminator::ADVANCE);
    data.extend_from_slice(signature_bytes);
    data.extend_from_slice(new_root);
    data.extend_from_slice(payload);

    let mut accounts = Vec::with_capacity(1 + passthrough_accounts.len());
    accounts.push(AccountMeta::new(*wallet_pda, false));
    accounts.extend_from_slice(passthrough_accounts);

    Instruction {
        program_id: ID,
        accounts,
        data,
    }
}

/// Build a Withdraw instruction (for use as an inner CPI inside Advance).
///
/// This returns an [`Instruction`] targeting the WinterWallet program itself.
/// It is intended to be passed to [`encode_advance`], NOT submitted as a
/// top-level instruction. The wallet PDA is marked `is_signer = false`
/// here — the on-chain Advance handler promotes it to signer via
/// `invoke_signed`.
pub fn withdraw(wallet_pda: &Address, receiver: &Address, lamports: u64) -> Instruction {
    let mut data = Vec::with_capacity(1 + 8);
    data.push(discriminator::WITHDRAW);
    data.extend_from_slice(&lamports.to_le_bytes());

    Instruction {
        program_id: ID,
        accounts: vec![
            AccountMeta::new(*wallet_pda, false),
            AccountMeta::new(*receiver, false),
        ],
        data,
    }
}

// ── CPI payload encoding ─────────────────────────────────────────────

/// The result of encoding inner CPI instructions for the Advance payload.
///
/// Both `data` and `accounts` are produced atomically — they MUST stay in
/// sync. The `accounts` are ordered to match how the on-chain program
/// consumes them: for each inner instruction, the program account comes
/// first, then the instruction's account metas.
pub struct AdvancePayload {
    /// The encoded CPI payload bytes to append to Advance instruction data.
    pub data: Vec<u8>,
    /// The passthrough accounts to include in the Advance instruction,
    /// ordered to match the payload.
    pub accounts: Vec<AccountMeta>,
}

/// Encode inner CPI instructions into an Advance payload + ordered account list.
///
/// # Wire format
///
/// ```text
/// num_instructions: u8
/// per instruction:
///   num_accounts: u8
///   data_len: u16 LE
///   data: [u8; data_len]
/// ```
///
/// Accounts are NOT encoded in the payload bytes — they are passed
/// positionally in the transaction's account list. This function produces
/// both the payload and the correctly-ordered account metas as a single
/// atomic return value, making it impossible to get them out of sync.
pub fn encode_advance(inner_instructions: &[Instruction]) -> Result<AdvancePayload, Error> {
    if inner_instructions.len() > 255 {
        return Err(Error::PayloadTooLarge("more than 255 inner instructions"));
    }

    let total_accounts: usize = inner_instructions
        .iter()
        .map(|ix| 1 + ix.accounts.len())
        .sum();
    if total_accounts > MAX_PASSTHROUGH_ACCOUNTS {
        return Err(Error::PayloadTooLarge(
            "total passthrough accounts exceeds MAX_PASSTHROUGH_ACCOUNTS (128)",
        ));
    }

    let payload_len: usize = 1 + inner_instructions
        .iter()
        .map(|ix| 1 + 2 + ix.data.len())
        .sum::<usize>();

    let mut data = Vec::with_capacity(payload_len);
    let mut accounts = Vec::with_capacity(total_accounts);

    data.push(inner_instructions.len() as u8);

    for ix in inner_instructions {
        if ix.accounts.len() > MAX_CPI_INSTRUCTION_ACCOUNTS {
            return Err(Error::PayloadTooLarge(
                "inner instruction exceeds MAX_CPI_INSTRUCTION_ACCOUNTS (16)",
            ));
        }
        if ix.data.len() > u16::MAX as usize {
            return Err(Error::PayloadTooLarge(
                "inner instruction data exceeds u16::MAX",
            ));
        }

        data.push(ix.accounts.len() as u8);
        data.extend_from_slice(&(ix.data.len() as u16).to_le_bytes());
        data.extend_from_slice(&ix.data);

        accounts.push(AccountMeta::new_readonly(ix.program_id, false));
        // Inner instruction accounts: signer flags are always false in
        // the outer transaction. The on-chain Advance handler promotes
        // the wallet PDA to signer via invoke_signed.
        for meta in &ix.accounts {
            if meta.is_writable {
                accounts.push(AccountMeta::new(meta.pubkey, false));
            } else {
                accounts.push(AccountMeta::new_readonly(meta.pubkey, false));
            }
        }
    }

    Ok(AdvancePayload { data, accounts })
}
