//! Regenerate the shared golden fixture JSONs from the current Rust client.
//!
//! Run with:
//! ```text
//! cargo test -p winterwallet-client --test regen -- --ignored --nocapture
//! ```
//!
//! All four fixtures (`initialize`, `advance-withdraw`, `advance-token-transfer`,
//! `advance-close`) are deterministic from the constants below — same values as
//! the original hand-crafted fixtures. Re-run this any time the program ID,
//! discriminators, or wire format changes.

use std::{fs, path::PathBuf};

use serde_json::json;
use solana_address::Address;
use winterwallet_client::*;

#[test]
#[ignore]
fn regen_fixtures() {
    let fixtures_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("fixtures");

    let payer: Address = "cGfHiC6Kgg3FpFZvgwGcswsCRtp4aBP2fzuXRQPizuN"
        .parse()
        .unwrap();
    let receiver: Address = "CktRuQ2mttgRGkXJtyksdKHjUdc2C4TgDzyB98oEzy8"
        .parse()
        .unwrap();
    let wallet_id = [0x01u8; 32];
    let current_root = [0x02u8; 32];
    let next_root = current_root;
    let new_root = [0x04u8; 32];
    let zero_sig = [0u8; SIGNATURE_LEN];

    let (wallet_pda, bump) = find_wallet_address(&wallet_id);

    // ── Initialize ───────────────────────────────────────────────────
    {
        let ix = initialize(&payer, &wallet_pda, &zero_sig, &next_root);
        let ixs = with_compute_budget(
            std::slice::from_ref(&ix),
            DEFAULT_ADVANCE_COMPUTE_UNIT_LIMIT,
            0,
        );
        let tx_size = estimate_legacy_transaction_size(&payer, &ixs).unwrap();

        let value = json!({
            "name": "initialize_v1",
            "wallet_id": hex(&wallet_id),
            "payer": payer.to_string(),
            "wallet_pda": wallet_pda.to_string(),
            "wallet_bump": bump,
            "next_root": hex(&next_root),
            "instruction_data_len": ix.data.len(),
            "instruction_data_sha256": hex(&solana_sha256_hasher::hash(&ix.data).to_bytes()),
            "instruction_accounts": metas_json(&ix.accounts),
            "legacy_transaction_size": tx_size,
        });
        write_pretty(&fixtures_dir.join("initialize.json"), &value);
    }

    // ── Advance(Withdraw) ────────────────────────────────────────────
    {
        let lamports: u64 = 500_000;
        let plan = AdvancePlan::withdraw(&wallet_pda, &receiver, lamports, &new_root).unwrap();
        let preimage = plan.preimage(&wallet_id, &current_root);
        let digest = solana_sha256_hasher::hashv(&preimage).to_bytes();
        let ix = plan.instruction(&zero_sig);
        let ixs = with_compute_budget(
            std::slice::from_ref(&ix),
            DEFAULT_ADVANCE_COMPUTE_UNIT_LIMIT,
            0,
        );
        let tx_size = estimate_legacy_transaction_size(&payer, &ixs).unwrap();

        let value = json!({
            "name": "advance_withdraw_v1",
            "wallet_id": hex(&wallet_id),
            "current_root": hex(&current_root),
            "new_root": hex(&new_root),
            "payer": payer.to_string(),
            "wallet_pda": wallet_pda.to_string(),
            "wallet_bump": bump,
            "receiver": receiver.to_string(),
            "lamports": lamports.to_string(),
            "payload": hex(plan.payload()),
            "passthrough_accounts": metas_json(plan.passthrough_accounts()),
            "advance_digest": hex(&digest),
            "advance_instruction_data_len": ix.data.len(),
            "advance_instruction_data_sha256": hex(&solana_sha256_hasher::hash(&ix.data).to_bytes()),
            "advance_instruction_accounts": metas_json(&ix.accounts),
            "legacy_transaction_size": tx_size,
        });
        write_pretty(&fixtures_dir.join("advance-withdraw.json"), &value);
    }

    // ── Advance(TokenTransfer) ───────────────────────────────────────
    {
        let token_program: Address = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
            .parse()
            .unwrap();
        let ata_program: Address = "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL"
            .parse()
            .unwrap();
        let mint: Address = "LbUiWL3xVV8hTFYBVdbTNrpDo41NKS6o3LHHuDzjfcY"
            .parse()
            .unwrap();
        let recipient_owner: Address = "QWmroo4YnnMqYW3cnxWkFdaTxGD3P7vMSzwMHGbUzwF"
            .parse()
            .unwrap();
        let source_ata: Address = "8EtiUEXLDm4XnvZaXKwzbXwZznQRApfW2NjzPAgebb8z"
            .parse()
            .unwrap();
        let destination_ata: Address = "FGBgsmcgiXTsDi92iCUuuBd25gsaF29Fa5cb8CsHhJtt"
            .parse()
            .unwrap();
        let amount: u64 = 123_456_789;

        let inner = token_transfer(
            &source_ata,
            &destination_ata,
            &wallet_pda,
            amount,
            &token_program,
        );
        let plan = AdvancePlan::new(&wallet_pda, &new_root, std::slice::from_ref(&inner)).unwrap();
        let preimage = plan.preimage(&wallet_id, &current_root);
        let digest = solana_sha256_hasher::hashv(&preimage).to_bytes();
        let ix = plan.instruction(&zero_sig);
        let ixs = with_compute_budget(
            std::slice::from_ref(&ix),
            DEFAULT_ADVANCE_COMPUTE_UNIT_LIMIT,
            0,
        );
        let tx_size = estimate_legacy_transaction_size(&payer, &ixs).unwrap();

        let value = json!({
            "name": "advance_token_transfer_v1",
            "wallet_id": hex(&wallet_id),
            "current_root": hex(&current_root),
            "new_root": hex(&new_root),
            "payer": payer.to_string(),
            "wallet_pda": wallet_pda.to_string(),
            "wallet_bump": bump,
            "token_program": token_program.to_string(),
            "ata_program": ata_program.to_string(),
            "mint": mint.to_string(),
            "recipient_owner": recipient_owner.to_string(),
            "source_ata": source_ata.to_string(),
            "destination_ata": destination_ata.to_string(),
            "amount": amount.to_string(),
            "inner_instruction_data": hex(&inner.data),
            "payload": hex(plan.payload()),
            "passthrough_accounts": metas_json(plan.passthrough_accounts()),
            "advance_digest": hex(&digest),
            "advance_instruction_data_len": ix.data.len(),
            "advance_instruction_data_sha256": hex(&solana_sha256_hasher::hash(&ix.data).to_bytes()),
            "advance_instruction_accounts": metas_json(&ix.accounts),
            "legacy_transaction_size": tx_size,
        });
        write_pretty(&fixtures_dir.join("advance-token-transfer.json"), &value);
    }

    // ── Advance(Close) ───────────────────────────────────────────────
    {
        let plan = AdvancePlan::close(&wallet_pda, &receiver, &new_root).unwrap();
        let preimage = plan.preimage(&wallet_id, &current_root);
        let digest = solana_sha256_hasher::hashv(&preimage).to_bytes();
        let ix = plan.instruction(&zero_sig);
        let ixs = with_compute_budget(
            std::slice::from_ref(&ix),
            DEFAULT_ADVANCE_COMPUTE_UNIT_LIMIT,
            0,
        );
        let tx_size = estimate_legacy_transaction_size(&payer, &ixs).unwrap();

        let value = json!({
            "name": "advance_close_v1",
            "wallet_id": hex(&wallet_id),
            "current_root": hex(&current_root),
            "new_root": hex(&new_root),
            "payer": payer.to_string(),
            "wallet_pda": wallet_pda.to_string(),
            "wallet_bump": bump,
            "receiver": receiver.to_string(),
            "payload": hex(plan.payload()),
            "passthrough_accounts": metas_json(plan.passthrough_accounts()),
            "advance_digest": hex(&digest),
            "advance_instruction_data_len": ix.data.len(),
            "advance_instruction_data_sha256": hex(&solana_sha256_hasher::hash(&ix.data).to_bytes()),
            "advance_instruction_accounts": metas_json(&ix.accounts),
            "legacy_transaction_size": tx_size,
        });
        write_pretty(&fixtures_dir.join("advance-close.json"), &value);
    }

    println!("regenerated fixtures in {}", fixtures_dir.display());
}

/// Generate a signing session fixture with real Winternitz signatures.
///
/// Run with:
/// ```text
/// cargo test -p winterwallet-client --test regen -- --ignored regen_signing_session --nocapture
/// ```
#[test]
#[ignore]
fn regen_signing_session() {
    use winterwallet_client::{initialize_preimage, token_transfer, wallet_id_from_mnemonic};
    use winterwallet_common::WINTERNITZ_SCALARS;
    use winterwallet_core::WinternitzKeypair;

    let fixtures_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("fixtures");

    // WARNING: This is a well-known BIP-39 test vector.
    // NEVER fund the derived wallet on mainnet.
    let mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    let wallet_id = wallet_id_from_mnemonic(mnemonic).unwrap();
    let (wallet_pda, bump) = find_wallet_address(&wallet_id);

    let payer: Address = "cGfHiC6Kgg3FpFZvgwGcswsCRtp4aBP2fzuXRQPizuN"
        .parse()
        .unwrap();
    let receiver: Address = "CktRuQ2mttgRGkXJtyksdKHjUdc2C4TgDzyB98oEzy8"
        .parse()
        .unwrap();
    let lamports: u64 = 1_000_000_000; // 1 SOL — must exceed rent-exempt minimum
    let token_amount: u64 = 1_000_000; // 1 token (6 decimals)
    let token_program: Address = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
        .parse()
        .unwrap();
    let mint: Address = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v" // USDC
        .parse()
        .unwrap();
    let ata_program: Address = "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL"
        .parse()
        .unwrap();
    // ATAs: PDA([owner, token_program, mint], ATA_PROGRAM)
    let (source_ata, _) = Address::derive_program_address(
        &[wallet_pda.as_array(), token_program.as_array(), mint.as_array()],
        &ata_program,
    )
    .unwrap();
    let (destination_ata, _) = Address::derive_program_address(
        &[receiver.as_array(), token_program.as_array(), mint.as_array()],
        &ata_program,
    )
    .unwrap();

    let mut keypair = WinternitzKeypair::from_mnemonic(mnemonic, 0).unwrap();

    // ── Session 0: Initialize ────────────────────────────────────────
    let init_preimage = initialize_preimage();
    let init_digest = solana_sha256_hasher::hashv(&init_preimage).to_bytes();
    let init_sig = keypair.sign_and_increment::<WINTERNITZ_SCALARS>(&init_preimage);
    let init_next_root = keypair
        .derive::<WINTERNITZ_SCALARS>()
        .to_pubkey()
        .merklize();

    let session_0 = json!({
        "position": 0,
        "current_root": hex(&wallet_id),
        "new_root": hex(init_next_root.as_bytes()),
        "signature": hex(init_sig.as_bytes()),
        "digest": hex(&init_digest),
    });

    // ── Session 1: Advance(Withdraw) ─────────────────────────────────
    // Keypair is at position 1 (will sign here). new_root must be the
    // root at position 2 — peek ahead without consuming.
    let current_root_1 = *init_next_root.as_bytes();
    let withdraw_new_root = WinternitzKeypair::from_mnemonic_at(mnemonic, 0, 0, 2)
        .unwrap()
        .derive::<WINTERNITZ_SCALARS>()
        .to_pubkey()
        .merklize();

    let plan_1 = AdvancePlan::withdraw(
        &wallet_pda,
        &receiver,
        lamports,
        withdraw_new_root.as_bytes(),
    )
    .unwrap();

    let preimage_1 = plan_1.preimage(&wallet_id, &current_root_1);
    let digest_1 = solana_sha256_hasher::hashv(&preimage_1).to_bytes();
    let sig_1 = keypair.sign_and_increment::<WINTERNITZ_SCALARS>(&preimage_1);

    let session_1 = json!({
        "position": 1,
        "current_root": hex(&current_root_1),
        "new_root": hex(withdraw_new_root.as_bytes()),
        "signature": hex(sig_1.as_bytes()),
        "digest": hex(&digest_1),
        "lamports": lamports.to_string(),
        "payload": hex(plan_1.payload()),
        "passthrough_accounts": metas_json(plan_1.passthrough_accounts()),
    });

    // ── Session 2: Advance(TokenTransfer) ──────────────────────────────
    // Keypair is at position 2 (will sign here). new_root = position 3.
    let current_root_2 = *withdraw_new_root.as_bytes();
    let token_new_root = WinternitzKeypair::from_mnemonic_at(mnemonic, 0, 0, 3)
        .unwrap()
        .derive::<WINTERNITZ_SCALARS>()
        .to_pubkey()
        .merklize();

    let token_inner = token_transfer(
        &source_ata,
        &destination_ata,
        &wallet_pda,
        token_amount,
        &token_program,
    );
    let plan_2 = AdvancePlan::new(
        &wallet_pda,
        token_new_root.as_bytes(),
        std::slice::from_ref(&token_inner),
    )
    .unwrap();

    let preimage_2 = plan_2.preimage(&wallet_id, &current_root_2);
    let digest_2 = solana_sha256_hasher::hashv(&preimage_2).to_bytes();
    let sig_2 = keypair.sign_and_increment::<WINTERNITZ_SCALARS>(&preimage_2);

    let session_2 = json!({
        "position": 2,
        "current_root": hex(&current_root_2),
        "new_root": hex(token_new_root.as_bytes()),
        "signature": hex(sig_2.as_bytes()),
        "digest": hex(&digest_2),
        "token_amount": token_amount.to_string(),
        "mint": mint.to_string(),
        "source_ata": source_ata.to_string(),
        "destination_ata": destination_ata.to_string(),
        "token_program": token_program.to_string(),
        "payload": hex(plan_2.payload()),
        "passthrough_accounts": metas_json(plan_2.passthrough_accounts()),
    });

    // ── Session 3: Advance(Close) ───────────────────────────────────────
    // Keypair is at position 3 (will sign here). new_root = position 4.
    let current_root_3 = *token_new_root.as_bytes();
    let close_new_root = WinternitzKeypair::from_mnemonic_at(mnemonic, 0, 0, 4)
        .unwrap()
        .derive::<WINTERNITZ_SCALARS>()
        .to_pubkey()
        .merklize();

    let plan_3 =
        AdvancePlan::close(&wallet_pda, &receiver, close_new_root.as_bytes()).unwrap();

    let preimage_3 = plan_3.preimage(&wallet_id, &current_root_3);
    let digest_3 = solana_sha256_hasher::hashv(&preimage_3).to_bytes();
    let sig_3 = keypair.sign_and_increment::<WINTERNITZ_SCALARS>(&preimage_3);

    let session_3 = json!({
        "position": 3,
        "current_root": hex(&current_root_3),
        "new_root": hex(close_new_root.as_bytes()),
        "signature": hex(sig_3.as_bytes()),
        "digest": hex(&digest_3),
        "payload": hex(plan_3.payload()),
        "passthrough_accounts": metas_json(plan_3.passthrough_accounts()),
    });

    let value = json!({
        "name": "signing_session_v1",
        "wallet_id": hex(&wallet_id),
        "wallet_pda": wallet_pda.to_string(),
        "wallet_bump": bump,
        "payer": payer.to_string(),
        "receiver": receiver.to_string(),
        "lamports": lamports.to_string(),
        "token_amount": token_amount.to_string(),
        "mint": mint.to_string(),
        "source_ata": source_ata.to_string(),
        "destination_ata": destination_ata.to_string(),
        "token_program": token_program.to_string(),
        "sessions": [session_0, session_1, session_2, session_3],
    });

    write_pretty(&fixtures_dir.join("signing-session.json"), &value);
    println!(
        "regenerated signing-session fixture in {}",
        fixtures_dir.display()
    );
}

fn metas_json(metas: &[solana_instruction::AccountMeta]) -> serde_json::Value {
    serde_json::Value::Array(
        metas
            .iter()
            .map(|m| {
                json!({
                    "pubkey": m.pubkey.to_string(),
                    "is_signer": m.is_signer,
                    "is_writable": m.is_writable,
                })
            })
            .collect(),
    )
}

fn write_pretty(path: &std::path::Path, value: &serde_json::Value) {
    let mut out = serde_json::to_string_pretty(value).unwrap();
    out.push('\n');
    fs::write(path, out).unwrap();
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
