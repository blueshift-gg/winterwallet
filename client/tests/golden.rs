use serde_json::Value;
use winterwallet_client::*;

#[test]
fn shared_initialize_vector_matches_rust_client() {
    let fixture: Value =
        serde_json::from_str(include_str!("../../fixtures/initialize.json")).unwrap();

    let wallet_id = hex_32(fixture["wallet_id"].as_str().unwrap());
    let next_root = hex_32(fixture["next_root"].as_str().unwrap());
    let payer: solana_address::Address = fixture["payer"].as_str().unwrap().parse().unwrap();
    let (wallet_pda, bump) = find_wallet_address(&wallet_id);
    assert_eq!(
        wallet_pda.to_string(),
        fixture["wallet_pda"].as_str().unwrap()
    );
    assert_eq!(bump, fixture["wallet_bump"].as_u64().unwrap() as u8);

    let zero_sig = [0u8; SIGNATURE_LEN];
    let ix = initialize(&payer, &wallet_pda, &zero_sig, &next_root);

    assert_eq!(
        ix.data.len(),
        fixture["instruction_data_len"].as_u64().unwrap() as usize
    );
    assert_eq!(
        hex(&solana_sha256_hasher::hash(&ix.data).to_bytes()),
        fixture["instruction_data_sha256"].as_str().unwrap()
    );
    assert_account_metas(
        &ix.accounts,
        fixture["instruction_accounts"].as_array().unwrap(),
    );

    let ixs = with_compute_budget(&[ix], DEFAULT_ADVANCE_COMPUTE_UNIT_LIMIT, 0);
    let tx_size = estimate_legacy_transaction_size(&payer, &ixs).unwrap();
    assert_eq!(
        tx_size,
        fixture["legacy_transaction_size"].as_u64().unwrap() as usize
    );
}

#[test]
fn shared_advance_withdraw_vector_matches_rust_client() {
    let fixture: Value =
        serde_json::from_str(include_str!("../../fixtures/advance-withdraw.json")).unwrap();

    let wallet_id = hex_32(fixture["wallet_id"].as_str().unwrap());
    let current_root = hex_32(fixture["current_root"].as_str().unwrap());
    let new_root = hex_32(fixture["new_root"].as_str().unwrap());
    let payer: solana_address::Address = fixture["payer"].as_str().unwrap().parse().unwrap();
    let receiver: solana_address::Address = fixture["receiver"].as_str().unwrap().parse().unwrap();
    let lamports: u64 = fixture["lamports"].as_str().unwrap().parse().unwrap();

    let (wallet_pda, bump) = find_wallet_address(&wallet_id);
    assert_eq!(
        wallet_pda.to_string(),
        fixture["wallet_pda"].as_str().unwrap()
    );
    assert_eq!(bump, fixture["wallet_bump"].as_u64().unwrap() as u8);

    let plan = AdvancePlan::withdraw(&wallet_pda, &receiver, lamports, &new_root).unwrap();
    assert_eq!(hex(plan.payload()), fixture["payload"].as_str().unwrap());

    assert_account_metas(
        plan.passthrough_accounts(),
        fixture["passthrough_accounts"].as_array().unwrap(),
    );

    let preimage = plan.preimage(&wallet_id, &current_root);
    let digest = solana_sha256_hasher::hashv(&preimage).to_bytes();
    assert_eq!(hex(&digest), fixture["advance_digest"].as_str().unwrap());

    let zero_sig = [0u8; SIGNATURE_LEN];
    let ix = plan.instruction(&zero_sig);
    assert_eq!(
        ix.data.len(),
        fixture["advance_instruction_data_len"].as_u64().unwrap() as usize
    );
    assert_eq!(
        hex(&solana_sha256_hasher::hash(&ix.data).to_bytes()),
        fixture["advance_instruction_data_sha256"].as_str().unwrap()
    );
    assert_account_metas(
        &ix.accounts,
        fixture["advance_instruction_accounts"].as_array().unwrap(),
    );

    let ixs = with_compute_budget(&[ix], DEFAULT_ADVANCE_COMPUTE_UNIT_LIMIT, 0);
    let tx_size = estimate_legacy_transaction_size(&payer, &ixs).unwrap();
    assert_eq!(
        tx_size,
        fixture["legacy_transaction_size"].as_u64().unwrap() as usize
    );
}

#[test]
fn shared_advance_close_vector_matches_rust_client() {
    let fixture: Value =
        serde_json::from_str(include_str!("../../fixtures/advance-close.json")).unwrap();

    let wallet_id = hex_32(fixture["wallet_id"].as_str().unwrap());
    let current_root = hex_32(fixture["current_root"].as_str().unwrap());
    let new_root = hex_32(fixture["new_root"].as_str().unwrap());
    let payer: solana_address::Address = fixture["payer"].as_str().unwrap().parse().unwrap();
    let receiver: solana_address::Address = fixture["receiver"].as_str().unwrap().parse().unwrap();

    let (wallet_pda, bump) = find_wallet_address(&wallet_id);
    assert_eq!(
        wallet_pda.to_string(),
        fixture["wallet_pda"].as_str().unwrap()
    );
    assert_eq!(bump, fixture["wallet_bump"].as_u64().unwrap() as u8);

    let plan = AdvancePlan::close(&wallet_pda, &receiver, &new_root).unwrap();
    assert_eq!(hex(plan.payload()), fixture["payload"].as_str().unwrap());

    assert_account_metas(
        plan.passthrough_accounts(),
        fixture["passthrough_accounts"].as_array().unwrap(),
    );

    let preimage = plan.preimage(&wallet_id, &current_root);
    let digest = solana_sha256_hasher::hashv(&preimage).to_bytes();
    assert_eq!(hex(&digest), fixture["advance_digest"].as_str().unwrap());

    let zero_sig = [0u8; SIGNATURE_LEN];
    let ix = plan.instruction(&zero_sig);
    assert_eq!(
        ix.data.len(),
        fixture["advance_instruction_data_len"].as_u64().unwrap() as usize
    );
    assert_eq!(
        hex(&solana_sha256_hasher::hash(&ix.data).to_bytes()),
        fixture["advance_instruction_data_sha256"].as_str().unwrap()
    );
    assert_account_metas(
        &ix.accounts,
        fixture["advance_instruction_accounts"].as_array().unwrap(),
    );

    let ixs = with_compute_budget(&[ix], DEFAULT_ADVANCE_COMPUTE_UNIT_LIMIT, 0);
    let tx_size = estimate_legacy_transaction_size(&payer, &ixs).unwrap();
    assert_eq!(
        tx_size,
        fixture["legacy_transaction_size"].as_u64().unwrap() as usize
    );
}

#[test]
fn shared_advance_token_transfer_vector_matches_rust_client() {
    let fixture: Value =
        serde_json::from_str(include_str!("../../fixtures/advance-token-transfer.json")).unwrap();

    let wallet_id = hex_32(fixture["wallet_id"].as_str().unwrap());
    let current_root = hex_32(fixture["current_root"].as_str().unwrap());
    let new_root = hex_32(fixture["new_root"].as_str().unwrap());
    let payer: solana_address::Address = fixture["payer"].as_str().unwrap().parse().unwrap();
    let source: solana_address::Address = fixture["source_ata"].as_str().unwrap().parse().unwrap();
    let destination: solana_address::Address = fixture["destination_ata"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let token_program: solana_address::Address =
        fixture["token_program"].as_str().unwrap().parse().unwrap();
    let amount: u64 = fixture["amount"].as_str().unwrap().parse().unwrap();

    let (wallet_pda, bump) = find_wallet_address(&wallet_id);
    assert_eq!(
        wallet_pda.to_string(),
        fixture["wallet_pda"].as_str().unwrap()
    );
    assert_eq!(bump, fixture["wallet_bump"].as_u64().unwrap() as u8);

    let inner = token_transfer(&source, &destination, &wallet_pda, amount, &token_program);
    assert_eq!(
        hex(&inner.data),
        fixture["inner_instruction_data"].as_str().unwrap()
    );
    let plan = AdvancePlan::new(&wallet_pda, &new_root, &[inner]).unwrap();
    assert_eq!(hex(plan.payload()), fixture["payload"].as_str().unwrap());
    assert_account_metas(
        plan.passthrough_accounts(),
        fixture["passthrough_accounts"].as_array().unwrap(),
    );

    let preimage = plan.preimage(&wallet_id, &current_root);
    let digest = solana_sha256_hasher::hashv(&preimage).to_bytes();
    assert_eq!(hex(&digest), fixture["advance_digest"].as_str().unwrap());

    let zero_sig = [0u8; SIGNATURE_LEN];
    let ix = plan.instruction(&zero_sig);
    assert_eq!(
        ix.data.len(),
        fixture["advance_instruction_data_len"].as_u64().unwrap() as usize
    );
    assert_eq!(
        hex(&solana_sha256_hasher::hash(&ix.data).to_bytes()),
        fixture["advance_instruction_data_sha256"].as_str().unwrap()
    );
    assert_account_metas(
        &ix.accounts,
        fixture["advance_instruction_accounts"].as_array().unwrap(),
    );

    let ixs = with_compute_budget(&[ix], DEFAULT_ADVANCE_COMPUTE_UNIT_LIMIT, 0);
    let tx_size = estimate_legacy_transaction_size(&payer, &ixs).unwrap();
    assert_eq!(
        tx_size,
        fixture["legacy_transaction_size"].as_u64().unwrap() as usize
    );
}

fn assert_account_metas(actual: &[solana_instruction::AccountMeta], expected: &[Value]) {
    assert_eq!(actual.len(), expected.len());
    for (actual, expected) in actual.iter().zip(expected) {
        assert_eq!(
            actual.pubkey.to_string(),
            expected["pubkey"].as_str().unwrap()
        );
        assert_eq!(actual.is_signer, expected["is_signer"].as_bool().unwrap());
        assert_eq!(
            actual.is_writable,
            expected["is_writable"].as_bool().unwrap()
        );
    }
}

fn hex_32(value: &str) -> [u8; 32] {
    let bytes = hex_decode(value);
    bytes.try_into().unwrap()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn hex_decode(value: &str) -> Vec<u8> {
    assert_eq!(value.len() % 2, 0);
    (0..value.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&value[i..i + 2], 16).unwrap())
        .collect()
}
