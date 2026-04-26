use winterwallet_client::*;

#[test]
fn initialize_preimage_matches_program() {
    // The program signs over just [WINTERWALLET_INITIALIZE].
    // See program/src/instructions/initialize.rs:102
    let parts = initialize_preimage();
    assert_eq!(parts.len(), 1);
    assert_eq!(parts[0], b"WINTERWALLET_INITIALIZE");
}

#[test]
fn advance_preimage_structure() {
    // See program/src/instructions/advance.rs:113-138
    let id = [1u8; 32];
    let current_root = [2u8; 32];
    let new_root = [3u8; 32];
    let account1 = [4u8; 32];
    let account2 = [5u8; 32];
    let payload = [0x01, 0x00, 0x00, 0x00];

    let addrs = [account1, account2];
    let parts = advance_preimage(&id, &current_root, &new_root, &addrs, &payload);

    // Structure: [tag, id, current_root, new_root, acct1, acct2, payload]
    assert_eq!(parts.len(), 7);
    assert_eq!(parts[0], b"WINTERWALLET_ADVANCE");
    assert_eq!(parts[1], &id[..]);
    assert_eq!(parts[2], &current_root[..]);
    assert_eq!(parts[3], &new_root[..]);
    assert_eq!(parts[4], &account1[..]);
    assert_eq!(parts[5], &account2[..]);
    assert_eq!(parts[6], &payload[..]);
}

#[test]
fn advance_preimage_no_accounts() {
    let id = [1u8; 32];
    let current_root = [2u8; 32];
    let new_root = [3u8; 32];
    let payload = [0x00]; // 0 inner instructions

    let parts = advance_preimage(&id, &current_root, &new_root, &[], &payload);

    // [tag, id, current_root, new_root, payload] = 5 parts
    assert_eq!(parts.len(), 5);
}
