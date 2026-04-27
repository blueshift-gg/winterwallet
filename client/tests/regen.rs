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
