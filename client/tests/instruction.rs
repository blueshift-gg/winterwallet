use winterwallet_client::*;

#[test]
fn initialize_instruction_layout() {
    let payer = solana_address::Address::from([1u8; 32]);
    let wallet_pda = solana_address::Address::from([2u8; 32]);
    let sig = [0xABu8; SIGNATURE_LEN];
    let root = [0xCDu8; 32];

    let ix = initialize(&payer, &wallet_pda, &sig, &root);

    // Program ID should be the winterwallet program.
    assert_eq!(ix.program_id, ID);

    // Three accounts: payer (signer, writable), wallet (writable), system_program (readonly).
    assert_eq!(ix.accounts.len(), 3);
    assert!(ix.accounts[0].is_signer);
    assert!(!ix.accounts[1].is_signer);
    assert!(!ix.accounts[2].is_signer);

    // Data layout: [discriminator(1)] [signature(768)] [root(32)] = 801 bytes.
    assert_eq!(ix.data.len(), 1 + SIGNATURE_LEN + 32);
    assert_eq!(ix.data[0], discriminator::INITIALIZE);
    assert_eq!(&ix.data[1..1 + SIGNATURE_LEN], &sig[..]);
    assert_eq!(&ix.data[1 + SIGNATURE_LEN..], &root[..]);
}

#[test]
fn withdraw_instruction_layout() {
    let wallet_pda = solana_address::Address::from([2u8; 32]);
    let receiver = solana_address::Address::from([3u8; 32]);
    let lamports: u64 = 1_000_000_000;

    let ix = withdraw(&wallet_pda, &receiver, lamports);

    assert_eq!(ix.program_id, ID);
    assert_eq!(ix.accounts.len(), 2);
    assert!(!ix.accounts[0].is_signer); // PDA never signer from caller — invoke_signed on-chain
    assert!(ix.accounts[0].is_writable);
    assert!(!ix.accounts[1].is_signer);

    // Data layout: [discriminator(1)] [lamports(8)] = 9 bytes.
    assert_eq!(ix.data.len(), 1 + 8);
    assert_eq!(ix.data[0], discriminator::WITHDRAW);
    assert_eq!(
        u64::from_le_bytes(ix.data[1..9].try_into().unwrap()),
        lamports
    );
}

#[test]
fn advance_instruction_layout() {
    let wallet_pda = solana_address::Address::from([2u8; 32]);
    let sig = [0xABu8; SIGNATURE_LEN];
    let root = [0xCDu8; 32];
    let payload = [0x01, 0x00, 0x00, 0x00]; // minimal: 1 instruction, 0 accounts, 0 data

    let accounts = vec![solana_instruction::AccountMeta::new_readonly(
        solana_address::Address::from([5u8; 32]),
        false,
    )];

    let ix = advance(&wallet_pda, &accounts, &sig, &root, &payload);

    assert_eq!(ix.program_id, ID);
    // 1 (wallet_pda) + 1 (passthrough) = 2 accounts.
    assert_eq!(ix.accounts.len(), 2);

    // Data: [disc(1)] [sig(768)] [root(32)] [payload(4)] = 805 bytes.
    assert_eq!(ix.data.len(), 1 + SIGNATURE_LEN + 32 + payload.len());
    assert_eq!(ix.data[0], discriminator::ADVANCE);
}

#[test]
fn encode_advance_withdraw_roundtrip() {
    let wallet_pda = solana_address::Address::from([2u8; 32]);
    let receiver = solana_address::Address::from([3u8; 32]);
    let lamports: u64 = 500_000;

    // Build the inner withdraw instruction.
    let inner = withdraw(&wallet_pda, &receiver, lamports);
    let payload = encode_advance(&[inner]).unwrap();

    // Payload structure: [num_instructions(1)] [num_accounts(1)] [data_len(2)] [data(9)]
    assert_eq!(payload.data[0], 1); // 1 inner instruction
    assert_eq!(payload.data[1], 2); // 2 accounts (wallet + receiver)
    let data_len = u16::from_le_bytes([payload.data[2], payload.data[3]]);
    assert_eq!(data_len, 9); // 1 discriminator + 8 lamports

    // Accounts: [winterwallet_program (readonly)] [wallet_pda (writable)] [receiver (writable)]
    // Signer flags are scrubbed — PDA signing happens via invoke_signed on-chain.
    assert_eq!(payload.accounts.len(), 3); // program + 2 instruction accounts
    assert_eq!(payload.accounts[0].pubkey, ID); // program account
    assert!(!payload.accounts[0].is_signer);
    assert_eq!(*payload.accounts[1].pubkey.as_array(), [2u8; 32]); // wallet_pda
    assert!(!payload.accounts[1].is_signer); // scrubbed — PDA can't sign outer tx
    assert!(payload.accounts[1].is_writable);
    assert_eq!(*payload.accounts[2].pubkey.as_array(), [3u8; 32]); // receiver
    assert!(!payload.accounts[2].is_signer);
}

#[test]
fn encode_advance_validates_limits() {
    // Too many inner instructions.
    let dummy = winterwallet_client::withdraw(
        &solana_address::Address::from([1u8; 32]),
        &solana_address::Address::from([2u8; 32]),
        100,
    );
    let many: Vec<_> = (0..256).map(|_| dummy.clone()).collect();
    assert!(encode_advance(&many).is_err());

    // Too many accounts per instruction.
    let mut big_ix = dummy.clone();
    for i in 0..17 {
        big_ix.accounts.push(solana_instruction::AccountMeta::new(
            solana_address::Address::from([i as u8; 32]),
            false,
        ));
    }
    assert!(encode_advance(&[big_ix]).is_err());
}

#[test]
fn advance_plan_keeps_payload_accounts_and_instruction_in_sync() {
    let wallet_pda = solana_address::Address::from([2u8; 32]);
    let receiver = solana_address::Address::from([3u8; 32]);
    let new_root = [4u8; 32];
    let sig = [0u8; SIGNATURE_LEN];

    let plan =
        winterwallet_client::AdvancePlan::withdraw(&wallet_pda, &receiver, 500_000, &new_root)
            .unwrap();
    let ix = plan.instruction(&sig);

    assert_eq!(plan.payload()[0], 1);
    assert_eq!(
        plan.account_addresses().len(),
        plan.passthrough_accounts().len()
    );
    assert_eq!(ix.accounts[0].pubkey, wallet_pda);
    assert!(!ix.accounts[0].is_signer);
}

#[test]
fn advance_plan_estimates_transaction_size_with_compute_budget() {
    let payer = solana_address::Address::from([9u8; 32]);
    let wallet_pda = solana_address::Address::from([2u8; 32]);
    let receiver = solana_address::Address::from([3u8; 32]);
    let new_root = [4u8; 32];
    let sig = [0u8; SIGNATURE_LEN];

    let plan =
        winterwallet_client::AdvancePlan::withdraw(&wallet_pda, &receiver, 500_000, &new_root)
            .unwrap();
    let size = plan.validate_transaction_size(&payer, &sig).unwrap();

    assert!(size > 0);
    assert!(size <= winterwallet_client::LEGACY_TRANSACTION_SIZE_LIMIT);
}
