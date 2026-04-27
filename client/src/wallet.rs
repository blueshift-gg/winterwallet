use solana_address::Address;
use solana_instruction::{AccountMeta, Instruction};
use winterwallet_common::{SIGNATURE_LEN, WINTERNITZ_SCALARS};
use winterwallet_core::WinternitzKeypair;

use crate::{
    AdvancePlan, Error, WinterWalletAccount, find_wallet_address,
    transaction::{DEFAULT_ADVANCE_COMPUTE_UNIT_LIMIT, with_compute_budget},
};

/// Current one-time-signature derivation position.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SigningPosition {
    wallet: u32,
    parent: u32,
    child: u32,
}

impl SigningPosition {
    /// Construct a signing position.
    pub const fn new(wallet: u32, parent: u32, child: u32) -> Self {
        Self {
            wallet,
            parent,
            child,
        }
    }

    /// Read the current position from a Winternitz keypair.
    pub fn from_keypair(keypair: &WinternitzKeypair) -> Self {
        Self::new(keypair.wallet(), keypair.parent(), keypair.child())
    }

    /// Wallet derivation index.
    pub const fn wallet(&self) -> u32 {
        self.wallet
    }

    /// Parent derivation index.
    pub const fn parent(&self) -> u32 {
        self.parent
    }

    /// Child derivation index.
    pub const fn child(&self) -> u32 {
        self.child
    }

    /// Return the next signing position.
    pub fn next(&self) -> Result<Self, Error> {
        match self.child.checked_add(1) {
            Some(child) => Ok(Self::new(self.wallet, self.parent, child)),
            None => Ok(Self::new(
                self.wallet,
                self.parent.checked_add(1).ok_or(Error::PositionOverflow)?,
                0,
            )),
        }
    }

    fn tuple(&self) -> (u32, u32, u32) {
        (self.wallet, self.parent, self.child)
    }

    fn ensure_can_advance(&self) -> Result<(), Error> {
        self.next().map(|_| ())
    }
}

/// High-level view of a WinterWallet account plus local signer position.
pub struct WinterWallet {
    id: [u8; 32],
    pda: Address,
    current_root: [u8; 32],
    position: SigningPosition,
}

impl WinterWallet {
    /// Build a wallet facade from explicit account state and local position.
    pub fn new(id: [u8; 32], current_root: [u8; 32], position: SigningPosition) -> Self {
        let (pda, _bump) = find_wallet_address(&id);
        Self {
            id,
            pda,
            current_root,
            position,
        }
    }

    /// Build a wallet facade from a deserialized on-chain account.
    pub fn from_account(account: &WinterWalletAccount, position: SigningPosition) -> Self {
        Self::new(account.id, *account.root.as_bytes(), position)
    }

    /// Wallet ID committed by the account.
    pub fn id(&self) -> &[u8; 32] {
        &self.id
    }

    /// Wallet PDA.
    pub fn pda(&self) -> &Address {
        &self.pda
    }

    /// Current root loaded from chain.
    pub fn current_root(&self) -> &[u8; 32] {
        &self.current_root
    }

    /// Local signer position expected for the next signature.
    pub fn position(&self) -> SigningPosition {
        self.position
    }

    /// Build an unsigned Advance from arbitrary inner CPI instructions.
    pub fn advance_plan(
        &self,
        new_root: &[u8; 32],
        inner_instructions: &[Instruction],
    ) -> Result<UnsignedAdvance, Error> {
        let plan = AdvancePlan::new(&self.pda, new_root, inner_instructions)?;
        Ok(UnsignedAdvance {
            wallet_id: self.id,
            current_root: self.current_root,
            position: self.position,
            plan,
        })
    }

    /// Build an unsigned Advance wrapping the built-in lamport withdraw CPI.
    pub fn withdraw_plan(
        &self,
        receiver: &Address,
        lamports: u64,
        new_root: &[u8; 32],
    ) -> Result<UnsignedAdvance, Error> {
        let plan = AdvancePlan::withdraw(&self.pda, receiver, lamports, new_root)?;
        Ok(UnsignedAdvance {
            wallet_id: self.id,
            current_root: self.current_root,
            position: self.position,
            plan,
        })
    }

    /// Build an unsigned Advance wrapping the built-in close CPI: sweeps all
    /// lamports to `receiver` and tears the wallet PDA down.
    pub fn close_plan(
        &self,
        receiver: &Address,
        new_root: &[u8; 32],
    ) -> Result<UnsignedAdvance, Error> {
        let plan = AdvancePlan::close(&self.pda, receiver, new_root)?;
        Ok(UnsignedAdvance {
            wallet_id: self.id,
            current_root: self.current_root,
            position: self.position,
            plan,
        })
    }

    /// Build an unsigned Advance wrapping an SPL Token transfer CPI.
    pub fn transfer_plan(
        &self,
        source_token_account: &Address,
        destination_token_account: &Address,
        token_program: &Address,
        amount: u64,
        new_root: &[u8; 32],
    ) -> Result<UnsignedAdvance, Error> {
        self.advance_plan(
            new_root,
            &[token_transfer(
                source_token_account,
                destination_token_account,
                &self.pda,
                amount,
                token_program,
            )],
        )
    }
}

/// Fully constructed Advance that has not burned a Winternitz position yet.
pub struct UnsignedAdvance {
    wallet_id: [u8; 32],
    current_root: [u8; 32],
    position: SigningPosition,
    plan: AdvancePlan,
}

impl UnsignedAdvance {
    /// Inner plan with payload/account order already fixed.
    pub fn plan(&self) -> &AdvancePlan {
        &self.plan
    }

    /// Position that will be consumed by signing.
    pub fn signing_position(&self) -> SigningPosition {
        self.position
    }

    /// Build the preimage parts that will be signed.
    pub fn preimage(&self) -> Vec<&[u8]> {
        self.plan.preimage(&self.wallet_id, &self.current_root)
    }

    /// Sign the plan and advance the supplied keypair.
    ///
    /// This consumes the unsigned value. The returned [`SignedAdvance`] cannot
    /// be sent until it has been persisted into a [`PersistedAdvance`].
    pub fn sign(self, keypair: &mut WinternitzKeypair) -> Result<SignedAdvance, Error> {
        let actual = SigningPosition::from_keypair(keypair);
        if actual != self.position {
            return Err(Error::SignerPositionMismatch {
                expected: self.position.tuple(),
                actual: actual.tuple(),
            });
        }
        actual.ensure_can_advance()?;

        let derived_root = keypair
            .derive::<WINTERNITZ_SCALARS>()
            .to_pubkey()
            .merklize();
        if derived_root.as_bytes() != &self.current_root {
            return Err(Error::RootMismatch);
        }

        let signature = {
            let preimage = self.preimage();
            let sig = keypair.sign_and_increment::<WINTERNITZ_SCALARS>(&preimage);
            let mut bytes = [0u8; SIGNATURE_LEN];
            bytes.copy_from_slice(sig.as_bytes());
            bytes
        };
        let next_position = SigningPosition::from_keypair(keypair);

        Ok(SignedAdvance {
            wallet_id: self.wallet_id,
            signing_position: self.position,
            next_position,
            signature,
            plan: self.plan,
        })
    }
}

/// Advance after a Winternitz position has been consumed, before persistence.
pub struct SignedAdvance {
    wallet_id: [u8; 32],
    signing_position: SigningPosition,
    next_position: SigningPosition,
    signature: [u8; SIGNATURE_LEN],
    plan: AdvancePlan,
}

impl SignedAdvance {
    /// Wallet ID for the signed operation.
    pub fn wallet_id(&self) -> &[u8; 32] {
        &self.wallet_id
    }

    /// Wallet PDA for the signed operation.
    pub fn wallet_pda(&self) -> &Address {
        self.plan.wallet_pda()
    }

    /// Position consumed by this signature.
    pub fn signing_position(&self) -> SigningPosition {
        self.signing_position
    }

    /// Next position that must be persisted before network submission.
    pub fn next_position(&self) -> SigningPosition {
        self.next_position
    }

    /// Raw Winternitz signature bytes.
    pub fn signature_bytes(&self) -> &[u8; SIGNATURE_LEN] {
        &self.signature
    }

    /// Persist the advanced signer position before network submission.
    pub fn persist<P>(self, persistence: &mut P) -> Result<PersistedAdvance, P::Error>
    where
        P: AdvancePersistence,
    {
        persistence.persist_signed_advance(&self)?;
        Ok(PersistedAdvance { signed: self })
    }
}

/// Persistence adapter for advancing local one-time-signature state.
pub trait AdvancePersistence {
    /// Persistence error type.
    type Error;

    /// Persist the signed operation's next position durably.
    fn persist_signed_advance(&mut self, advance: &SignedAdvance) -> Result<(), Self::Error>;
}

/// Advance whose consumed position has been durably recorded.
pub struct PersistedAdvance {
    signed: SignedAdvance,
}

impl PersistedAdvance {
    /// Access the signed operation metadata.
    pub fn signed(&self) -> &SignedAdvance {
        &self.signed
    }

    /// Build the signed Advance instruction.
    pub fn advance_instruction(&self) -> Instruction {
        self.signed.plan.instruction(&self.signed.signature)
    }

    /// Build transaction instructions with compute-budget prefix.
    pub fn transaction_instructions(
        &self,
        unit_limit: u32,
        unit_price_micro_lamports: u64,
    ) -> Vec<Instruction> {
        with_compute_budget(
            &[self.advance_instruction()],
            unit_limit,
            unit_price_micro_lamports,
        )
    }

    /// Build transaction instructions using the SDK default compute limit.
    pub fn default_transaction_instructions(
        &self,
        unit_price_micro_lamports: u64,
    ) -> Vec<Instruction> {
        self.transaction_instructions(
            DEFAULT_ADVANCE_COMPUTE_UNIT_LIMIT,
            unit_price_micro_lamports,
        )
    }

    /// Send through an adapter that only accepts persisted advances.
    pub fn send<S>(&self, sender: &mut S) -> Result<String, S::Error>
    where
        S: AdvanceSender,
    {
        sender.send_persisted_advance(self)
    }
}

/// Sender adapter for persisted advances.
pub trait AdvanceSender {
    /// Sender error type.
    type Error;

    /// Submit a persisted operation.
    fn send_persisted_advance(&mut self, advance: &PersistedAdvance)
    -> Result<String, Self::Error>;
}

/// Build an SPL Token `Transfer` instruction for use inside Advance.
pub fn token_transfer(
    source: &Address,
    destination: &Address,
    authority: &Address,
    amount: u64,
    token_program: &Address,
) -> Instruction {
    let mut data = Vec::with_capacity(9);
    data.push(3);
    data.extend_from_slice(&amount.to_le_bytes());

    Instruction {
        program_id: *token_program,
        accounts: vec![
            AccountMeta::new(*source, false),
            AccountMeta::new(*destination, false),
            AccountMeta::new_readonly(*authority, false),
        ],
        data,
    }
}
