#![no_std]
#![deny(missing_docs)]

//! Winternitz one-time signatures with BIP-39 keypair derivation.
//!
//! `no_std` (no `alloc`). The `signer` and `mnemonic` paths
//! ([`WinternitzKeypair`], [`WinternitzPrivkey`]) compile only off-Solana;
//! verification ([`WinternitzSignature`], [`WinternitzPubkey`],
//! [`WinternitzRoot`]) builds everywhere.
//!
//! ## Security
//!
//! Winternitz is a **one-time signature scheme**. Signing two different
//! messages with the same privkey scalars allows an attacker to forge a third
//! signature. Use [`WinternitzKeypair::sign_and_increment`] to enforce
//! position advancement after every signature.
//!
//! Derivation uses a custom magic string `"Winternitz seed"` and is
//! **not** BIP-32 compatible — keys derived here will not match any standard
//! Bitcoin/Solana wallet.
//!
//! ## Parameters
//!
//! All public types take a const generic `N` (number of message scalars).
//! `N` must be even and in `16..=32`; the constraint is enforced at compile
//! time. `N = 32` gives 256-bit message-hash security.

mod error;
mod pubkey;
mod signature;

#[cfg(not(any(target_arch = "bpf", target_os = "solana")))]
mod privkey;

#[cfg(not(any(target_arch = "bpf", target_os = "solana")))]
mod keypair;

pub use error::WinternitzError;
pub use pubkey::{WinternitzPubkey, WinternitzRoot};
pub use signature::WinternitzSignature;

#[cfg(not(any(target_arch = "bpf", target_os = "solana")))]
pub use privkey::WinternitzPrivkey;

#[cfg(not(any(target_arch = "bpf", target_os = "solana")))]
pub use keypair::WinternitzKeypair;

pub(crate) const fn assert_n<const N: usize>() {
    assert!(
        N >= 16 && N <= 32 && N.is_multiple_of(2),
        "N must be even and in 16..=32",
    );
}

#[inline(always)]
pub(crate) fn chain(seed: &[u8; 32], iters: u8) -> [u8; 32] {
    let mut current = *seed;
    for _ in 0..iters {
        current = solana_sha256_hasher::hash(&current).to_bytes();
    }
    current
}

#[inline(always)]
pub(crate) fn hash<const N: usize>(message: &[&[u8]]) -> [u8; N] {
    const { assert_n::<N>() };
    let digest = solana_sha256_hasher::hashv(message).to_bytes();
    let mut out = [0u8; N];
    out.copy_from_slice(&digest[..N]);
    out
}
