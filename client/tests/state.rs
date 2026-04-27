use winterwallet_client::*;

#[test]
fn deserialize_wallet_account() {
    let mut data = [0u8; WALLET_ACCOUNT_LEN];
    // id = 0x01 repeated
    data[0..32].fill(0x01);
    // root = 0x02 repeated
    data[32..64].fill(0x02);
    // bump = 42
    data[64] = 42;

    let account = WinterWalletAccount::from_bytes(&data).unwrap();
    assert_eq!(account.id, [0x01; 32]);
    assert_eq!(*account.root.as_bytes(), [0x02; 32]);
    assert_eq!(account.bump, [42]);
}

#[test]
fn deserialize_rejects_wrong_size() {
    // Too short.
    let short = [0u8; 64];
    assert!(WinterWalletAccount::from_bytes(&short).is_err());

    // Too long.
    let long = [0u8; 66];
    assert!(WinterWalletAccount::from_bytes(&long).is_err());

    // Exactly right.
    let exact = [0u8; 65];
    assert!(WinterWalletAccount::from_bytes(&exact).is_ok());
}

#[test]
fn wallet_account_len_is_65() {
    assert_eq!(WALLET_ACCOUNT_LEN, 65);
    assert_eq!(
        core::mem::size_of::<WinterWalletAccount>(),
        WALLET_ACCOUNT_LEN
    );
}
