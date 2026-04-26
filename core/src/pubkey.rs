use core::mem::MaybeUninit;

use crate::WinternitzError;

/// 32-byte commitment to a [`WinternitzPubkey`] — the domain-separated Merkle
/// root over its scalars. This is the value verifiers store; signatures are
/// validated against it.
#[repr(transparent)]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct WinternitzRoot([u8; 32]);

/// Public Winternitz key: `N` message scalars followed by 2 checksum scalars,
/// each 32 bytes. Total size is `(N + 2) * 32` bytes.
///
/// Each pubkey scalar equals `SHA256` applied 255 times to the corresponding
/// privkey scalar. `N` must be even and in `16..=32` (compile-time enforced).
#[repr(C)]
pub struct WinternitzPubkey<const N: usize> {
    scalars: [[u8; 32]; N],
    checksum: [[u8; 32]; 2],
}

impl<'a, const N: usize> TryFrom<&'a [u8]> for &'a WinternitzPubkey<N> {
    type Error = WinternitzError;

    fn try_from(value: &'a [u8]) -> Result<Self, Self::Error> {
        const { crate::assert_n::<N>() };
        if value.len() != (N + 2) * 32 {
            return Err(WinternitzError::InvalidLength);
        }
        // SAFETY: length verified; alignment of WinternitzPubkey<N> is 1 (all fields are [u8; _]);
        // every bit pattern is a valid inhabitant.
        Ok(unsafe { &*value.as_ptr().cast::<WinternitzPubkey<N>>() })
    }
}

impl WinternitzRoot {
    /// Wrap a 32-byte value as a `WinternitzRoot`. No validation is performed
    /// — the caller is asserting that these bytes are a previously-produced
    /// root (e.g. loaded from on-chain account state).
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Return the 32 raw bytes of the root.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl From<[u8; 32]> for WinternitzRoot {
    fn from(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

impl core::fmt::Display for WinternitzRoot {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "0x")?;
        for b in &self.0 {
            write!(f, "{:02x}", b)?;
        }
        Ok(())
    }
}

impl<const N: usize> WinternitzPubkey<N> {
    pub(crate) fn new(scalars: [[u8; 32]; N], checksum: [[u8; 32]; 2]) -> Self {
        const { crate::assert_n::<N>() };
        Self { scalars, checksum }
    }

    /// Return the pubkey's `(N + 2) * 32` raw bytes (message scalars then
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

    /// Compute the Merkle root over all `N + 2` scalars. Leaves are
    /// `SHA256(0x00 || scalar)`; internal nodes are `SHA256(0x01 || L || R)`
    /// (domain-separated). The tree shape is "rightmost-duplication on odd
    /// levels" — equivalently, an online stack-collapse where leaves are
    /// streamed left-to-right, adjacent same-level entries combine on push,
    /// and any orphan top-of-stack at drain is lifted to its left
    /// neighbour's level by repeated self-duplication.
    pub fn merklize(&self) -> WinternitzRoot {
        const { crate::assert_n::<N>() };
        // Domain-separate leaves and internal nodes to prevent second-preimage
        // attacks where an attacker constructs an internal node value that
        // collides with a leaf hash.
        const LEAF_TAG: &[u8] = &[0x00];
        const NODE_TAG: &[u8] = &[0x01];

        // For N in [16, 32] (even), N + 2 is in [18, 34]. Maximum stack
        // depth across any prefix is `popcount` of that prefix length —
        // worst case is i = 31 with popcount 5, hit only when N + 2 >= 32.
        const MAX_DEPTH: usize = 5;
        let mut stack: [MaybeUninit<[u8; 32]>; MAX_DEPTH] =
            [const { MaybeUninit::uninit() }; MAX_DEPTH];
        let mut levels = [0u8; MAX_DEPTH];
        let mut len: usize = 0;

        for leaf in self.scalars.iter().chain(self.checksum.iter()) {
            let mut h: [u8; 32] = solana_sha256_hasher::hashv(&[LEAF_TAG, leaf]).to_bytes();
            let mut level: u8 = 0;
            while len > 0 && levels[len - 1] == level {
                // SAFETY: stack[len - 1] was initialised in a prior push.
                let top = unsafe { stack[len - 1].assume_init_read() };
                h = solana_sha256_hasher::hashv(&[NODE_TAG, &top, &h]).to_bytes();
                level += 1;
                len -= 1;
            }
            stack[len].write(h);
            levels[len] = level;
            len += 1;
        }

        // Drain: collapse the stack, lifting orphan right-edge subtrees by
        // self-duplication so the resulting tree matches the level-by-level
        // shape with rightmost-duplication.
        while len > 1 {
            // SAFETY: both top entries are initialised.
            let mut top = unsafe { stack[len - 1].assume_init_read() };
            let mut top_level = levels[len - 1];
            let next_level = levels[len - 2];
            while top_level < next_level {
                top = solana_sha256_hasher::hashv(&[NODE_TAG, &top, &top]).to_bytes();
                top_level += 1;
            }
            let next = unsafe { stack[len - 2].assume_init_read() };
            let combined = solana_sha256_hasher::hashv(&[NODE_TAG, &next, &top]).to_bytes();
            stack[len - 2].write(combined);
            levels[len - 2] = top_level + 1;
            len -= 1;
        }

        // SAFETY: at least N + 2 >= 18 leaves were processed, so len == 1.
        let root = unsafe { stack[0].assume_init_read() };
        WinternitzRoot(root)
    }
}

impl<const N: usize> core::fmt::Display for WinternitzPubkey<N> {
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

impl<const N: usize> core::fmt::Debug for WinternitzPubkey<N> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        writeln!(f, "WinternitzPubkey {{")?;
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

impl<const N: usize> From<WinternitzPubkey<N>> for WinternitzRoot {
    fn from(pk: WinternitzPubkey<N>) -> Self {
        const { crate::assert_n::<N>() };
        pk.merklize()
    }
}

impl<const N: usize> From<&WinternitzPubkey<N>> for WinternitzRoot {
    fn from(pk: &WinternitzPubkey<N>) -> Self {
        const { crate::assert_n::<N>() };
        pk.merklize()
    }
}
