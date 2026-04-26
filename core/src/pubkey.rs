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
    /// (domain-separated). Odd-length levels duplicate the trailing node.
    #[inline(never)]
    pub fn merklize(&self) -> WinternitzRoot {
        const { crate::assert_n::<N>() };
        // Domain-separate leaves and internal nodes to prevent second-preimage
        // attacks where an attacker constructs an internal node value that
        // collides with a leaf hash.
        const LEAF_TAG: &[u8] = &[0x00];
        const NODE_TAG: &[u8] = &[0x01];

        // N <= 32 (asserted), so N + 2 <= 34. Stack buffer collapses in place.
        let mut nodes: [[u8; 32]; 34] = [[0u8; 32]; 34];
        for (slot, leaf) in nodes
            .iter_mut()
            .zip(self.scalars.iter().chain(self.checksum.iter()))
        {
            *slot = solana_sha256_hasher::hashv(&[LEAF_TAG, leaf]).to_bytes();
        }
        let mut len = N + 2;

        while len > 1 {
            let mut write = 0;
            let mut read = 0;
            while read < len {
                let left = nodes[read];
                let right = if read + 1 < len {
                    nodes[read + 1]
                } else {
                    left
                };
                nodes[write] =
                    solana_sha256_hasher::hashv(&[NODE_TAG, &left[..], &right[..]]).to_bytes();
                write += 1;
                read += 2;
            }
            len = write;
        }

        WinternitzRoot(nodes[0])
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
