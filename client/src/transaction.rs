use solana_address::Address;
use solana_instruction::Instruction;

use crate::Error;

/// Solana legacy transaction wire-size limit.
pub const LEGACY_TRANSACTION_SIZE_LIMIT: usize = 1232;

/// Default compute unit limit used for dry-run previews where simulation is not
/// available. Live transactions should use simulation-based CU estimation instead.
pub const DEFAULT_ADVANCE_COMPUTE_UNIT_LIMIT: u32 = 800_000;

/// Build a ComputeBudget `SetComputeUnitLimit` instruction.
pub fn set_compute_unit_limit(units: u32) -> Instruction {
    let mut data = Vec::with_capacity(5);
    data.push(0x02);
    data.extend_from_slice(&units.to_le_bytes());
    Instruction {
        program_id: compute_budget_program_id(),
        accounts: vec![],
        data,
    }
}

/// Build a ComputeBudget `SetComputeUnitPrice` instruction.
pub fn set_compute_unit_price(micro_lamports: u64) -> Instruction {
    let mut data = Vec::with_capacity(9);
    data.push(0x03);
    data.extend_from_slice(&micro_lamports.to_le_bytes());
    Instruction {
        program_id: compute_budget_program_id(),
        accounts: vec![],
        data,
    }
}

/// Prefix an instruction set with compute-budget instructions.
pub fn with_compute_budget(
    instructions: &[Instruction],
    unit_limit: u32,
    unit_price_micro_lamports: u64,
) -> Vec<Instruction> {
    let mut out = Vec::with_capacity(instructions.len() + 2);
    out.push(set_compute_unit_limit(unit_limit));
    out.push(set_compute_unit_price(unit_price_micro_lamports));
    out.extend_from_slice(instructions);
    out
}

/// Estimate the serialized size of a legacy transaction with the given payer.
///
/// The estimate is exact for transactions with at most 256 account keys, which
/// is comfortably above WinterWallet's current account limits.
pub fn estimate_legacy_transaction_size(
    payer: &Address,
    instructions: &[Instruction],
) -> Result<usize, Error> {
    let accounts = collect_accounts(payer, instructions)?;
    let required_signatures = accounts.iter().filter(|a| a.is_signer).count();
    let message_size = legacy_message_size(&accounts, instructions)?;
    Ok(compact_u16_len(required_signatures) + required_signatures * 64 + message_size)
}

/// Validate legacy transaction size against Solana's 1232-byte wire limit.
pub fn validate_legacy_transaction_size(
    payer: &Address,
    instructions: &[Instruction],
) -> Result<usize, Error> {
    let estimated = estimate_legacy_transaction_size(payer, instructions)?;
    if estimated > LEGACY_TRANSACTION_SIZE_LIMIT {
        return Err(Error::TransactionTooLarge {
            estimated,
            limit: LEGACY_TRANSACTION_SIZE_LIMIT,
        });
    }
    Ok(estimated)
}

/// Ensure a transaction only requires the payer's signature.
pub fn validate_payer_only_signers(
    payer: &Address,
    instructions: &[Instruction],
) -> Result<(), Error> {
    for ix in instructions {
        for meta in &ix.accounts {
            if meta.is_signer && meta.pubkey != *payer {
                return Err(Error::UnsupportedTransaction(
                    "transaction requires a non-payer signature",
                ));
            }
        }
    }
    Ok(())
}

fn compute_budget_program_id() -> Address {
    solana_address::address!("ComputeBudget111111111111111111111111111111")
}

#[derive(Clone)]
pub struct AccountEntry {
    pub pubkey: Address,
    pub is_signer: bool,
    pub is_writable: bool,
}

fn collect_accounts(
    payer: &Address,
    instructions: &[Instruction],
) -> Result<Vec<AccountEntry>, Error> {
    let mut accounts = Vec::new();
    upsert(&mut accounts, payer, true, true);

    for ix in instructions {
        upsert(&mut accounts, &ix.program_id, false, false);
        for meta in &ix.accounts {
            upsert(
                &mut accounts,
                &meta.pubkey,
                meta.is_signer,
                meta.is_writable,
            );
        }
    }

    accounts.sort_by_key(|entry| match (entry.is_signer, entry.is_writable) {
        (true, true) => 0,
        (true, false) => 1,
        (false, true) => 2,
        (false, false) => 3,
    });

    if accounts.len() > u8::MAX as usize + 1 {
        return Err(Error::UnsupportedTransaction(
            "legacy message account indexes exceed u8 range",
        ));
    }

    Ok(accounts)
}

fn legacy_message_size(
    accounts: &[AccountEntry],
    instructions: &[Instruction],
) -> Result<usize, Error> {
    let mut size = 3; // header
    size += compact_u16_len(accounts.len());
    size += accounts.len() * 32;
    size += 32; // recent blockhash
    size += compact_u16_len(instructions.len());

    for ix in instructions {
        size += 1; // program id index
        size += compact_u16_len(ix.accounts.len());
        size += ix.accounts.len(); // account indexes
        size += compact_u16_len(ix.data.len());
        size += ix.data.len();

        if accounts.iter().all(|a| a.pubkey != ix.program_id) {
            return Err(Error::UnsupportedTransaction(
                "instruction program id missing from account list",
            ));
        }
        for meta in &ix.accounts {
            if accounts.iter().all(|a| a.pubkey != meta.pubkey) {
                return Err(Error::UnsupportedTransaction(
                    "instruction account missing from account list",
                ));
            }
        }
    }

    Ok(size)
}

pub fn upsert(
    accounts: &mut Vec<AccountEntry>,
    pubkey: &Address,
    is_signer: bool,
    is_writable: bool,
) {
    if let Some(existing) = accounts.iter_mut().find(|a| a.pubkey == *pubkey) {
        existing.is_signer |= is_signer;
        existing.is_writable |= is_writable;
    } else {
        accounts.push(AccountEntry {
            pubkey: *pubkey,
            is_signer,
            is_writable,
        });
    }
}

fn compact_u16_len(value: usize) -> usize {
    let mut value = value;
    let mut len = 1;
    while value >= 0x80 {
        value >>= 7;
        len += 1;
    }
    len
}
