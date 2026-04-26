// Size of our Winternitz scalars (+2 for checksum)
pub const WINTERNITZ_SCALARS: usize = 22;
pub const WINTERWALLET_INITIALIZE: &[u8] = b"WINTERWALLET_INITIALIZE";
pub const WINTERWALLET_ADVANCE: &[u8] = b"WINTERWALLET_ADVANCE";

/// Upper bound on trailing accounts an Advance instruction can commit to.
/// Sizes the stack-allocated signature-preimage buffer used during recovery.
pub const MAX_PASSTHROUGH_ACCOUNTS: usize = 128;

/// Upper bound on account metas per inner CPI'd instruction inside Advance.
pub const MAX_CPI_INSTRUCTION_ACCOUNTS: usize = 16;
