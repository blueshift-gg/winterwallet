use crate::{WinternitzError, WinternitzPrivkey};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256, Sha512};
use zeroize::{Zeroize, ZeroizeOnDrop};

const MAX_PASSPHRASE_LEN: usize = 256;

/// Hierarchical Winternitz keypair derived from a BIP-39 mnemonic.
///
/// Derivation is hardened-only HMAC-SHA512 (SLIP-0010-style) with magic string
/// `"Winternitz seed"`, **not** BIP-32. The four hardened levels are
/// `master / wallet' / parent' / child'`, with `N + 2` further hardened leaves
/// at indices `0..N+2` for each Winternitz scalar.
///
/// **Security:** each `(wallet, parent, child)` position must sign at most one
/// message. Use [`Self::sign_and_increment`] to enforce this. Calling
/// [`Self::derive`] twice at the same position produces the same privkey
/// (intentional — for inspection); signing twice with that privkey is a
/// catastrophic key compromise.
///
/// All secret material is zeroized on drop.
#[derive(Default, Zeroize, ZeroizeOnDrop)]
pub struct WinternitzKeypair {
    key: [u8; 32],
    chain_code: [u8; 32],
    wallet: u32,
    parent: u32,
    child: u32,
}

impl WinternitzKeypair {
    /// The English BIP-39 wordlist (2048 newline-separated words).
    pub const WORDLIST: &str = include_str!("wordlist.txt");

    /// Build a keypair from a BIP-39 mnemonic with the given wallet index.
    /// Initial position is `(wallet, parent=0, child=0)`. The mnemonic is
    /// validated against the BIP-39 wordlist and checksum.
    pub fn from_mnemonic(
        mnemonic: &str,
        wallet: u32,
    ) -> Result<WinternitzKeypair, WinternitzError> {
        Self::validate(mnemonic)?;
        Ok(Self::from_mnemonic_unchecked(mnemonic, wallet))
    }

    /// Build a keypair at an arbitrary `(wallet, parent, child)` position.
    /// Use this to resume a CLI session from persisted position state.
    pub fn from_mnemonic_at(
        mnemonic: &str,
        wallet: u32,
        parent: u32,
        child: u32,
    ) -> Result<WinternitzKeypair, WinternitzError> {
        Self::validate(mnemonic)?;
        let mut kp = Self::from_mnemonic_unchecked(mnemonic, wallet);
        kp.parent = parent;
        kp.child = child;
        Ok(kp)
    }

    /// Current wallet index.
    pub fn wallet(&self) -> u32 {
        self.wallet
    }

    /// Current parent index.
    pub fn parent(&self) -> u32 {
        self.parent
    }

    /// Current child index.
    pub fn child(&self) -> u32 {
        self.child
    }

    /// Convert 32 bytes of caller-supplied entropy into a 24-word BIP-39
    /// mnemonic. The caller is responsible for sourcing entropy from a CSPRNG.
    pub fn generate_mnemonic(entropy: [u8; 32]) -> [&'static str; 24] {
        let mut bits = [0u8; 33];
        bits[..32].copy_from_slice(&entropy);
        bits[32] = Sha256::digest(entropy)[0];

        let mut words: [&'static str; 24] = [""; 24];
        let mut bit_pos = 0usize;
        for slot in &mut words {
            let mut idx = 0u16;
            for _ in 0..11 {
                idx = (idx << 1) | (((bits[bit_pos / 8] >> (7 - (bit_pos % 8))) & 1) as u16);
                bit_pos += 1;
            }
            *slot = Self::WORDLIST
                .lines()
                .nth(idx as usize)
                .expect("11-bit index always < 2048");
        }
        words
    }

    fn validate(mnemonic: &str) -> Result<(), WinternitzError> {
        let count = mnemonic.split_ascii_whitespace().count();
        let total_bits = match count {
            12 => 132,
            15 => 165,
            18 => 198,
            21 => 231,
            24 => 264,
            _ => return Err(WinternitzError::InvalidMnemonic),
        };
        let entropy_bits = total_bits * 32 / 33;
        let cs_bits = total_bits - entropy_bits;

        let mut bits = [0u8; 33];
        let mut bit_pos = 0usize;
        for word in mnemonic.split_ascii_whitespace() {
            let idx = Self::WORDLIST
                .lines()
                .position(|line| line == word)
                .ok_or(WinternitzError::InvalidMnemonic)? as u16;
            for b in (0..11).rev() {
                if (idx >> b) & 1 == 1 {
                    bits[bit_pos / 8] |= 1 << (7 - (bit_pos % 8));
                }
                bit_pos += 1;
            }
        }

        let entropy_bytes = entropy_bits / 8;
        let hash = Sha256::digest(&bits[..entropy_bytes]);
        let mask = 0xFFu8 << (8 - cs_bits);
        if (bits[entropy_bytes] & mask) != (hash[0] & mask) {
            return Err(WinternitzError::InvalidMnemonic);
        }
        Ok(())
    }

    /// Compute the BIP-39 seed for a mnemonic with no passphrase. Equivalent
    /// to `seed_with_passphrase(mnemonic, "")`.
    pub fn seed(mnemonic: &str) -> Result<[u8; 64], WinternitzError> {
        Self::seed_with_passphrase(mnemonic, "")
    }

    /// Compute the BIP-39 seed via PBKDF2-HMAC-SHA512 (2048 iterations) with
    /// salt `"mnemonic" + passphrase`. Returns
    /// [`WinternitzError::InvalidLength`] if the passphrase exceeds 256 bytes.
    pub fn seed_with_passphrase(
        mnemonic: &str,
        passphrase: &str,
    ) -> Result<[u8; 64], WinternitzError> {
        if passphrase.len() > MAX_PASSPHRASE_LEN {
            return Err(WinternitzError::InvalidLength);
        }
        Self::validate(mnemonic)?;
        Ok(Self::raw_seed(mnemonic, passphrase))
    }

    fn raw_seed(mnemonic: &str, passphrase: &str) -> [u8; 64] {
        debug_assert!(passphrase.len() <= MAX_PASSPHRASE_LEN);
        const PREFIX: &[u8] = b"mnemonic";
        let mut salt = [0u8; PREFIX.len() + MAX_PASSPHRASE_LEN];
        salt[..PREFIX.len()].copy_from_slice(PREFIX);
        salt[PREFIX.len()..PREFIX.len() + passphrase.len()].copy_from_slice(passphrase.as_bytes());
        let mut seed = [0u8; 64];
        pbkdf2::pbkdf2_hmac::<Sha512>(
            mnemonic.as_bytes(),
            &salt[..PREFIX.len() + passphrase.len()],
            2048,
            &mut seed,
        );
        seed
    }

    fn from_mnemonic_unchecked(mnemonic: &str, wallet: u32) -> WinternitzKeypair {
        let seed = Self::raw_seed(mnemonic, "");

        let mut mac = <Hmac<Sha512>>::new_from_slice(b"Winternitz seed")
            .expect("HMAC accepts any key length");
        mac.update(&seed);
        let i = mac.finalize().into_bytes();

        let mut key = [0u8; 32];
        let mut chain_code = [0u8; 32];
        key.copy_from_slice(&i[..32]);
        chain_code.copy_from_slice(&i[32..]);

        WinternitzKeypair {
            key,
            chain_code,
            wallet,
            parent: 0,
            child: 0,
        }
    }

    fn derive_child(key: &[u8; 32], chain_code: &[u8; 32], index: u32) -> ([u8; 32], [u8; 32]) {
        let mut mac =
            <Hmac<Sha512>>::new_from_slice(chain_code).expect("HMAC accepts any key length");
        mac.update(&[0u8]);
        mac.update(key);
        mac.update(&(index | 0x8000_0000).to_be_bytes());
        let i = mac.finalize().into_bytes();

        let mut child_key = [0u8; 32];
        let mut child_chain = [0u8; 32];
        child_key.copy_from_slice(&i[..32]);
        child_chain.copy_from_slice(&i[32..]);
        (child_key, child_chain)
    }

    /// Derive the privkey at the current `(wallet, parent, child)` position.
    /// Walks four hardened HMAC levels then derives `N + 2` further hardened
    /// children at indices `0..N+2`. Idempotent — calling twice at the same
    /// position returns the same scalars.
    pub fn derive<const N: usize>(&self) -> WinternitzPrivkey<N> {
        const { crate::assert_n::<N>() };
        let (mut k, mut c) = (self.key, self.chain_code);
        for idx in [self.wallet, self.parent, self.child] {
            let (nk, nc) = Self::derive_child(&k, &c, idx);
            k = nk;
            c = nc;
        }

        let mut scalars = [[0u8; 32]; N];
        for (i, slot) in scalars.iter_mut().enumerate() {
            *slot = Self::derive_child(&k, &c, i as u32).0;
        }
        let checksum = [
            Self::derive_child(&k, &c, N as u32).0,
            Self::derive_child(&k, &c, (N + 1) as u32).0,
        ];

        WinternitzPrivkey::new(scalars, checksum)
    }

    /// Derive the privkey at the current position, sign, then advance to the
    /// next position. This is the safe sign primitive — once called, the
    /// privkey at that position is consumed and the keypair cannot produce
    /// another signature at the same position via this method.
    pub fn sign_and_increment<const N: usize>(
        &mut self,
        message: &[&[u8]],
    ) -> crate::WinternitzSignature<N> {
        let sig = self.derive::<N>().sign(message);
        self.increment_child();
        sig
    }

    /// Advance to the next child position. On `child == u32::MAX` overflow,
    /// resets child to 0 and increments parent (which itself panics on
    /// overflow).
    pub fn increment_child(&mut self) {
        match self.child.checked_add(1) {
            Some(c) => self.child = c,
            None => {
                self.child = 0;
                self.increment_parent()
            }
        }
    }

    /// Advance to the next parent position and reset child to 0. Panics on
    /// `parent == u32::MAX` overflow.
    pub fn increment_parent(&mut self) {
        self.parent = self.parent.checked_add(1).expect("parent overflow");
        self.child = 0;
    }
}
