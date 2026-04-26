/// Errors returned by the WinterWallet client.
#[derive(Debug)]
pub enum Error {
    /// Account data is missing, too short, or has an unexpected layout.
    InvalidAccountData,

    /// A Winternitz cryptographic operation failed.
    Winternitz(winterwallet_core::WinternitzError),

    /// Recovery scan did not find a matching position within the given depth.
    RecoveryFailed(u32),

    /// The on-chain root does not match the expected local root.
    RootMismatch,

    /// The CPI payload exceeds on-chain limits.
    PayloadTooLarge(&'static str),

    /// The estimated transaction size exceeds the Solana limit.
    TransactionTooLarge { estimated: usize, limit: usize },

    /// The requested transaction shape is outside the supported legacy builder.
    UnsupportedTransaction(&'static str),
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidAccountData => write!(f, "invalid account data"),
            Self::Winternitz(e) => write!(f, "winternitz error: {e}"),
            Self::RecoveryFailed(depth) => {
                write!(f, "position not found within scan depth {depth}")
            }
            Self::RootMismatch => write!(f, "on-chain root does not match local state"),
            Self::PayloadTooLarge(reason) => write!(f, "payload too large: {reason}"),
            Self::TransactionTooLarge { estimated, limit } => {
                write!(
                    f,
                    "transaction too large: {estimated} bytes (limit {limit})"
                )
            }
            Self::UnsupportedTransaction(reason) => write!(f, "unsupported transaction: {reason}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Winternitz(e) => Some(e),
            _ => None,
        }
    }
}

impl From<winterwallet_core::WinternitzError> for Error {
    fn from(e: winterwallet_core::WinternitzError) -> Self {
        Self::Winternitz(e)
    }
}
