use crate::{WinternitzError, WinternitzPubkey, WinternitzSignature};
use zeroize::{Zeroize, ZeroizeOnDrop};

/// Private Winternitz key: `N` message scalars and 2 checksum scalars, each 32
/// bytes. Zeroized on drop.
///
/// **Security:** Winternitz is a *one-time* signature scheme. Signing two
/// different messages with the same privkey scalars allows an attacker to
/// forge signatures on a third message. Prefer
/// [`crate::WinternitzKeypair::sign_and_increment`] which guarantees the
/// keypair advances after every signature. Calling [`Self::sign`] directly
/// places the burden of one-time-use enforcement on the caller.
#[repr(C)]
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct WinternitzPrivkey<const N: usize> {
    scalars: [[u8; 32]; N],
    checksum: [[u8; 32]; 2],
}

impl<const N: usize> WinternitzPrivkey<N> {
    pub(crate) fn new(scalars: [[u8; 32]; N], checksum: [[u8; 32]; 2]) -> Self {
        const { crate::assert_n::<N>() };
        Self { scalars, checksum }
    }

    /// Return the privkey's `(N + 2) * 32` raw bytes (message scalars then
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

    /// Sign a message. Consumes `self` so the same privkey can't be reused
    /// directly. Note that re-deriving from a `WinternitzKeypair` at the same
    /// position produces an identical privkey — for replay-safe signing prefer
    /// [`crate::WinternitzKeypair::sign_and_increment`].
    ///
    /// The message is supplied as a slice of byte slices (matching
    /// `solana_sha256_hasher::hashv`), so callers can mix domain-separation
    /// tags or context bytes with the payload.
    pub fn sign(self, message: &[&[u8]]) -> WinternitzSignature<N> {
        const { crate::assert_n::<N>() };
        let h = Self::hash(message);
        self.sign_prehashed(&h)
    }

    /// Sign a pre-hashed message. Consumes `self`; same caveat as [`Self::sign`].
    #[inline(always)]
    pub fn sign_prehashed(self, hash: &[u8; N]) -> WinternitzSignature<N> {
        const { crate::assert_n::<N>() };
        let mut sig_scalars = [[0u8; 32]; N];
        let mut checksum_sum: u16 = 0;
        for i in 0..N {
            let b = hash[i];
            sig_scalars[i] = crate::chain(&self.scalars[i], b);
            checksum_sum += 255u16 - b as u16;
        }
        let sig_checksum = [
            crate::chain(&self.checksum[0], (checksum_sum >> 8) as u8),
            crate::chain(&self.checksum[1], checksum_sum as u8),
        ];
        WinternitzSignature::new(sig_scalars, sig_checksum)
    }

    /// Hash a message into the `N`-byte Winternitz digest used by signing.
    /// Equivalent to truncating SHA-256 of the concatenated `message` slices
    /// to `N` bytes.
    #[inline(always)]
    pub fn hash(message: &[&[u8]]) -> [u8; N] {
        crate::hash::<N>(message)
    }

    /// Derive the corresponding [`WinternitzPubkey`] by chaining each scalar
    /// 255 times under SHA-256.
    pub fn to_pubkey(&self) -> WinternitzPubkey<N> {
        const { crate::assert_n::<N>() };
        let mut scalars = [[0u8; 32]; N];
        for (out, sk) in scalars.iter_mut().zip(self.scalars.iter()) {
            *out = crate::chain(sk, 255);
        }
        let checksum = [
            crate::chain(&self.checksum[0], 255),
            crate::chain(&self.checksum[1], 255),
        ];
        WinternitzPubkey::new(scalars, checksum)
    }
}

impl<const N: usize> core::fmt::Display for WinternitzPrivkey<N> {
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

impl<const N: usize> From<WinternitzPrivkey<N>> for WinternitzPubkey<N> {
    fn from(sk: WinternitzPrivkey<N>) -> Self {
        const { crate::assert_n::<N>() };
        sk.to_pubkey()
    }
}

impl<const N: usize> From<&WinternitzPrivkey<N>> for WinternitzPubkey<N> {
    fn from(sk: &WinternitzPrivkey<N>) -> Self {
        const { crate::assert_n::<N>() };
        sk.to_pubkey()
    }
}

impl<'a, const N: usize> TryFrom<&'a [u8]> for &'a WinternitzPrivkey<N> {
    type Error = WinternitzError;

    fn try_from(value: &'a [u8]) -> Result<Self, Self::Error> {
        const { crate::assert_n::<N>() };
        if value.len() != (N + 2) * 32 {
            return Err(WinternitzError::InvalidLength);
        }
        // SAFETY: length verified; alignment is 1; every bit pattern is valid.
        Ok(unsafe { &*value.as_ptr().cast::<WinternitzPrivkey<N>>() })
    }
}
