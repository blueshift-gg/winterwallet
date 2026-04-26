use solana_address::Address;
use solana_instruction::{AccountMeta, Instruction};
use winterwallet_common::SIGNATURE_LEN;

use crate::{
    AdvancePayload, Error, advance, advance_preimage, encode_advance,
    transaction::{
        DEFAULT_ADVANCE_COMPUTE_UNIT_LIMIT, estimate_legacy_transaction_size,
        validate_legacy_transaction_size, with_compute_budget,
    },
    withdraw,
};

/// Fully assembled but unsigned Advance instruction plan.
///
/// A plan owns the encoded CPI payload and passthrough account order, so callers
/// cannot accidentally sign one account order and submit another.
pub struct AdvancePlan {
    wallet_pda: Address,
    new_root: [u8; 32],
    payload: AdvancePayload,
    account_addresses: Vec<[u8; 32]>,
}

impl AdvancePlan {
    /// Build an Advance plan from inner CPI instructions.
    pub fn new(
        wallet_pda: &Address,
        new_root: &[u8; 32],
        inner_instructions: &[Instruction],
    ) -> Result<Self, Error> {
        let payload = encode_advance(inner_instructions)?;
        let account_addresses = payload
            .accounts
            .iter()
            .map(|meta| *meta.pubkey.as_array())
            .collect();

        Ok(Self {
            wallet_pda: *wallet_pda,
            new_root: *new_root,
            payload,
            account_addresses,
        })
    }

    /// Wallet PDA this plan targets.
    pub fn wallet_pda(&self) -> &Address {
        &self.wallet_pda
    }

    /// Build a plan for the built-in lamport withdraw CPI.
    pub fn withdraw(
        wallet_pda: &Address,
        receiver: &Address,
        lamports: u64,
        new_root: &[u8; 32],
    ) -> Result<Self, Error> {
        Self::new(
            wallet_pda,
            new_root,
            &[withdraw(wallet_pda, receiver, lamports)],
        )
    }

    /// The raw Advance payload committed by the signature.
    pub fn payload(&self) -> &[u8] {
        &self.payload.data
    }

    /// Passthrough accounts in the exact order expected by the on-chain handler.
    pub fn passthrough_accounts(&self) -> &[AccountMeta] {
        &self.payload.accounts
    }

    /// Account addresses included in the Advance preimage.
    pub fn account_addresses(&self) -> &[[u8; 32]] {
        &self.account_addresses
    }

    /// New root stored by the Advance instruction.
    pub fn new_root(&self) -> &[u8; 32] {
        &self.new_root
    }

    /// Build the preimage parts to sign for this plan.
    pub fn preimage<'a>(
        &'a self,
        wallet_id: &'a [u8; 32],
        current_root: &'a [u8; 32],
    ) -> Vec<&'a [u8]> {
        advance_preimage(
            wallet_id,
            current_root,
            &self.new_root,
            &self.account_addresses,
            &self.payload.data,
        )
    }

    /// Convert the plan into a signed Advance instruction.
    pub fn instruction(&self, signature_bytes: &[u8; SIGNATURE_LEN]) -> Instruction {
        advance(
            &self.wallet_pda,
            &self.payload.accounts,
            signature_bytes,
            &self.new_root,
            &self.payload.data,
        )
    }

    /// Estimate the legacy transaction size with default Advance compute budget.
    pub fn estimate_transaction_size(
        &self,
        payer: &Address,
        signature_bytes: &[u8; SIGNATURE_LEN],
    ) -> Result<usize, Error> {
        let ix = self.instruction(signature_bytes);
        let ixs = with_compute_budget(&[ix], DEFAULT_ADVANCE_COMPUTE_UNIT_LIMIT, 0);
        estimate_legacy_transaction_size(payer, &ixs)
    }

    /// Validate that the planned Advance fits in a legacy transaction.
    pub fn validate_transaction_size(
        &self,
        payer: &Address,
        signature_bytes: &[u8; SIGNATURE_LEN],
    ) -> Result<usize, Error> {
        let ix = self.instruction(signature_bytes);
        let ixs = with_compute_budget(&[ix], DEFAULT_ADVANCE_COMPUTE_UNIT_LIMIT, 0);
        validate_legacy_transaction_size(payer, &ixs)
    }
}
