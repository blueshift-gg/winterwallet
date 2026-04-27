use winterwallet_common::{WINTERWALLET_ADVANCE, WINTERWALLET_INITIALIZE};

/// Build the preimage parts for an Initialize signature.
///
/// The program signs over just the domain tag. The wallet ID is *recovered*
/// from the signature (not committed in the preimage) — the signer's
/// identity IS the wallet ID.
pub fn initialize_preimage() -> [&'static [u8]; 1] {
    [WINTERWALLET_INITIALIZE]
}

/// Build the preimage parts for an Advance signature.
///
/// Must match `program/src/instructions/advance.rs:verify_signature` exactly.
///
/// The `account_addresses` MUST be in the same order as the passthrough
/// accounts in the Advance instruction's account list. Use
/// [`crate::encode_advance`] to produce both the payload and account list
/// atomically, then extract addresses from those account metas.
pub fn advance_preimage<'a>(
    id: &'a [u8; 32],
    current_root: &'a [u8; 32],
    new_root: &'a [u8; 32],
    account_addresses: &'a [[u8; 32]],
    payload: &'a [u8],
) -> Vec<&'a [u8]> {
    let mut parts = Vec::with_capacity(5 + account_addresses.len());
    parts.push(WINTERWALLET_ADVANCE as &[u8]);
    parts.push(id.as_slice());
    parts.push(current_root.as_slice());
    parts.push(new_root.as_slice());
    for addr in account_addresses {
        parts.push(addr.as_slice());
    }
    parts.push(payload);
    parts
}
