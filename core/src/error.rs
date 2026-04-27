/// Errors returned by this crate.
#[derive(Debug)]
pub enum WinternitzError {
    /// The mnemonic failed BIP-39 validation: wrong word count, unknown word,
    /// or invalid checksum.
    InvalidMnemonic,
    /// A byte slice supplied to a `TryFrom` impl, or a passphrase, exceeded
    /// the expected length.
    InvalidLength,
    /// A signature failed to verify.
    SignatureError,
}

impl core::fmt::Display for WinternitzError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidMnemonic => write!(f, "invalid BIP-39 mnemonic"),
            Self::InvalidLength => write!(f, "byte length does not match expected size"),
            Self::SignatureError => write!(f, "signature verification failed"),
        }
    }
}

impl core::error::Error for WinternitzError {}
