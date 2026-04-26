use crate::{WinternitzError, WinternitzPubkey, WinternitzRoot};

/// Winternitz one-time signature: `N` message scalars and 2 checksum scalars,
/// each 32 bytes. Total size is `(N + 2) * 32` bytes.
///
/// Verify with [`Self::verify`] against a [`WinternitzRoot`] and the original
/// message. Verification reconstructs the corresponding pubkey by completing
/// each chain, merklizes, and compares to the supplied root.
#[repr(C)]
pub struct WinternitzSignature<const N: usize> {
    scalars: [[u8; 32]; N],
    checksum: [[u8; 32]; 2],
}

impl<const N: usize> WinternitzSignature<N> {
    /// Create a signature from `N * [u8;32]` scalars + 2 checksum scalars.
    pub fn new(scalars: [[u8; 32]; N], checksum: [[u8; 32]; 2]) -> Self {
        const { crate::assert_n::<N>() };
        Self { scalars, checksum }
    }

    /// Return the signature's `(N + 2) * 32` raw bytes (message scalars then
    /// checksum scalars), with no copy.
    pub fn as_bytes(&self) -> &[u8] {
        // SAFETY: #[repr(C)], all fields are [u8; _] with align 1, no padding.
        unsafe {
            core::slice::from_raw_parts(
                self as *const Self as *const u8,
                core::mem::size_of::<Self>(),
            )
        }
    }

    /// Verify this signature against a message and a stored root. Returns
    /// `true` iff the signature is valid. The message is supplied as a slice
    /// of byte slices (matching `solana_sha256_hasher::hashv`), so callers
    /// can mix domain-separation tags or context bytes with the payload
    /// without an intermediate allocation.
    pub fn verify(&self, message: &[&[u8]], root: &WinternitzRoot) -> bool {
        self.recover_pubkey(message).merklize() == *root
    }

    /// Verify against a pre-hashed message digest. See [`Self::hash`] for the
    /// expected digest construction.
    #[inline(always)]
    pub fn verify_prehashed(&self, hash: &[u8; N], root: &WinternitzRoot) -> bool {
        self.recover_pubkey_prehashed(hash).merklize() == *root
    }

    /// Recover the [`WinternitzPubkey`] implied by this signature over the
    /// given message. No verification is performed — pair with
    /// [`WinternitzPubkey::merklize`] (or rely on the `Into<WinternitzRoot>`
    /// impl) and compare against a trusted root to verify.
    ///
    /// The message is supplied as a slice of byte slices so callers can
    /// include domain-separation tags or context bytes alongside the payload
    /// (matching `solana_sha256_hasher::hashv`).
    #[inline(always)]
    pub fn recover_pubkey(&self, message: &[&[u8]]) -> WinternitzPubkey<N> {
        const { crate::assert_n::<N>() };
        let h = crate::hash::<N>(message);
        self.recover_pubkey_prehashed(&h)
    }

    /// Recover the [`WinternitzPubkey`] from a pre-hashed message digest. See
    /// [`Self::hash`] for the expected digest construction.
    #[inline(never)]
    pub fn recover_pubkey_prehashed(&self, hash: &[u8; N]) -> WinternitzPubkey<N> {
        const { crate::assert_n::<N>() };
        let mut pk_scalars = [[0u8; 32]; N];
        let mut checksum_sum: u16 = 0;
        for i in 0..N {
            let b = hash[i];
            pk_scalars[i] = crate::chain(&self.scalars[i], 255 - b);
            checksum_sum += 255u16 - b as u16;
        }
        let pk_checksum = [
            crate::chain(&self.checksum[0], 255 - (checksum_sum >> 8) as u8),
            crate::chain(&self.checksum[1], 255 - checksum_sum as u8),
        ];
        WinternitzPubkey::new(pk_scalars, pk_checksum)
    }

    /// Hash a message into the `N`-byte Winternitz digest used by verification.
    /// Equivalent to truncating SHA-256 of the concatenated `message` slices
    /// to `N` bytes.
    #[inline(always)]
    pub fn hash(message: &[&[u8]]) -> [u8; N] {
        crate::hash::<N>(message)
    }
}

impl<const N: usize> core::fmt::Display for WinternitzSignature<N> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "0x")?;
        for s in self.scalars.iter().chain(self.checksum.iter()) {
            for b in s {
                write!(f, "{:02x}", b)?;
            }
        }
        Ok(())
    }
}

impl<const N: usize> core::fmt::Debug for WinternitzSignature<N> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        writeln!(f, "WinternitzSignature {{")?;
        for (i, s) in self.scalars.iter().enumerate() {
            write!(f, "  scalars[{}]  = 0x", i)?;
            for b in s {
                write!(f, "{:02x}", b)?;
            }
            writeln!(f)?;
        }
        for (i, s) in self.checksum.iter().enumerate() {
            write!(f, "  checksum[{}] = 0x", i)?;
            for b in s {
                write!(f, "{:02x}", b)?;
            }
            writeln!(f)?;
        }
        write!(f, "}}")
    }
}

impl<'a, const N: usize> TryFrom<&'a [u8]> for &'a WinternitzSignature<N> {
    type Error = WinternitzError;

    fn try_from(value: &'a [u8]) -> Result<Self, Self::Error> {
        const { crate::assert_n::<N>() };
        if value.len() != (N + 2) * 32 {
            return Err(WinternitzError::InvalidLength);
        }
        // SAFETY: length verified; alignment is 1; every bit pattern is valid.
        Ok(unsafe { &*value.as_ptr().cast::<WinternitzSignature<N>>() })
    }
}
