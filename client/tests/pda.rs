use winterwallet_client::*;

#[test]
fn find_wallet_address_is_deterministic() {
    let wallet_id = [0xAA; 32];
    let (addr1, bump1) = find_wallet_address(&wallet_id);
    let (addr2, bump2) = find_wallet_address(&wallet_id);
    assert_eq!(addr1, addr2);
    assert_eq!(bump1, bump2);
}

#[test]
fn different_ids_give_different_addresses() {
    let (addr_a, _) = find_wallet_address(&[0x01; 32]);
    let (addr_b, _) = find_wallet_address(&[0x02; 32]);
    assert_ne!(addr_a, addr_b);
}

#[test]
fn wallet_id_from_known_mnemonic() {
    // Use the canonical test mnemonic from core/tests.
    let mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
    let id = wallet_id_from_mnemonic(mnemonic).unwrap();

    // The ID should be 32 bytes and deterministic.
    assert_eq!(id.len(), 32);

    // Calling again produces the same ID.
    let id2 = wallet_id_from_mnemonic(mnemonic).unwrap();
    assert_eq!(id, id2);
}

#[test]
fn wallet_id_rejects_invalid_mnemonic() {
    assert!(wallet_id_from_mnemonic("not a valid mnemonic").is_err());
}
