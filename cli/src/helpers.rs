use ed25519_dalek::{Signer, SigningKey};
use serde_json::{Value, json};
use solana_address::Address;
use solana_instruction::Instruction;
use winterwallet_client::{
    AdvanceSender, DEFAULT_ADVANCE_COMPUTE_UNIT_LIMIT, PersistedAdvance,
    validate_legacy_transaction_size, validate_payer_only_signers, with_compute_budget,
};
use zeroize::{Zeroize, Zeroizing};

/// Read an ed25519 keypair from a JSON file (Solana CLI format: [u8; 64]).
pub fn read_keypair(path: &str) -> Result<SigningKey, String> {
    let mut data = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read keypair from {path}: {e}"))?;
    let mut bytes: Vec<u8> = match serde_json::from_str(&data) {
        Ok(bytes) => bytes,
        Err(e) => {
            data.zeroize();
            return Err(format!("invalid keypair JSON: {e}"));
        }
    };
    data.zeroize();
    if bytes.len() != 64 {
        let len = bytes.len();
        bytes.zeroize();
        return Err(format!("keypair must be 64 bytes, got {len}"));
    }
    let mut secret: [u8; 32] = bytes[..32]
        .try_into()
        .map_err(|_| "keypair secret must be 32 bytes".to_string())?;
    let key = SigningKey::from_bytes(&secret);
    secret.zeroize();
    bytes.zeroize();
    Ok(key)
}

pub fn read_mnemonic() -> Result<Zeroizing<String>, String> {
    let raw = Zeroizing::new(
        rpassword::prompt_password("Enter mnemonic: ")
            .map_err(|e| format!("failed to read mnemonic: {e}"))?,
    );
    Ok(Zeroizing::new(raw.trim().to_string()))
}

/// Payer pubkey from signing key.
pub fn pubkey(key: &SigningKey) -> Address {
    Address::from(key.verifying_key().to_bytes())
}

pub fn validate_commitment(commitment: &str) -> Result<(), String> {
    match commitment {
        "processed" | "confirmed" | "finalized" => Ok(()),
        _ => Err("commitment must be one of: processed, confirmed, finalized".to_string()),
    }
}

// ── RPC helpers (sync, ureq) ─────────────────────────────────────────

pub fn rpc_post(rpc_url: &str, method: &str, params: Value) -> Result<Value, String> {
    let body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params,
    });

    let resp: Value = ureq::post(rpc_url)
        .send_json(&body)
        .map_err(|e| format!("RPC request failed: {e}"))?
        .body_mut()
        .read_json()
        .map_err(|e| format!("RPC response parse error: {e}"))?;

    if let Some(err) = resp.get("error") {
        return Err(format!("RPC error: {err}"));
    }

    resp.get("result")
        .cloned()
        .ok_or_else(|| "RPC response missing 'result'".to_string())
}

pub fn get_latest_blockhash(rpc_url: &str, commitment: &str) -> Result<[u8; 32], String> {
    validate_commitment(commitment)?;
    let result = rpc_post(
        rpc_url,
        "getLatestBlockhash",
        json!([{"commitment": commitment}]),
    )?;
    let hash_str = result["value"]["blockhash"]
        .as_str()
        .ok_or("missing blockhash")?;
    bs58_decode_32(hash_str)
}

pub fn get_account(
    rpc_url: &str,
    commitment: &str,
    address: &Address,
) -> Result<AccountResult, String> {
    validate_commitment(commitment)?;
    let result = rpc_post(
        rpc_url,
        "getAccountInfo",
        json!([address.to_string(), {"encoding": "base64", "commitment": commitment}]),
    )?;

    if result["value"].is_null() {
        return Err(format!("account not found: {address}"));
    }

    let data_b64 = result["value"]["data"][0]
        .as_str()
        .ok_or("missing account data")?;
    let data = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, data_b64)
        .map_err(|e| format!("base64 decode error: {e}"))?;
    let lamports = result["value"]["lamports"]
        .as_u64()
        .ok_or("missing lamports")?;

    Ok(AccountResult { data, lamports })
}

pub struct AccountResult {
    pub data: Vec<u8>,
    pub lamports: u64,
}

// ── Transaction pipeline ─────────────────────────────────────────────

pub struct TransactionPreview {
    pub estimated_size: usize,
    pub compute_unit_limit: u32,
    pub priority_fee_micro_lamports: u64,
}

pub fn transaction_preview(
    payer: &SigningKey,
    instructions: &[Instruction],
    priority_fee_micro_lamports: u64,
) -> Result<TransactionPreview, String> {
    let payer_addr = pubkey(payer);
    let final_ixs = with_compute_budget(
        instructions,
        DEFAULT_ADVANCE_COMPUTE_UNIT_LIMIT,
        priority_fee_micro_lamports,
    );
    validate_payer_only_signers(&payer_addr, &final_ixs).map_err(|e| e.to_string())?;
    let estimated_size =
        validate_legacy_transaction_size(&payer_addr, &final_ixs).map_err(|e| e.to_string())?;
    Ok(TransactionPreview {
        estimated_size,
        compute_unit_limit: DEFAULT_ADVANCE_COMPUTE_UNIT_LIMIT,
        priority_fee_micro_lamports,
    })
}

/// Add compute budget, validate size/signers, simulate, sign, send, and confirm.
pub fn simulate_sign_send(
    rpc_url: &str,
    commitment: &str,
    payer: &SigningKey,
    instructions: &[Instruction],
    priority_fee_micro_lamports: u64,
) -> Result<String, String> {
    let payer_addr = pubkey(payer);
    let final_ixs = with_compute_budget(
        instructions,
        DEFAULT_ADVANCE_COMPUTE_UNIT_LIMIT,
        priority_fee_micro_lamports,
    );
    validate_payer_only_signers(&payer_addr, &final_ixs).map_err(|e| e.to_string())?;
    validate_legacy_transaction_size(&payer_addr, &final_ixs).map_err(|e| e.to_string())?;

    // Simulate the exact transaction shape with zeroed transaction signatures.
    let blockhash = get_latest_blockhash(rpc_url, commitment)?;
    let message = build_message(&payer_addr, &final_ixs, &blockhash);
    let unsigned_tx = build_unsigned_wire_tx(&message, 1);
    simulate_transaction(rpc_url, commitment, &unsigned_tx)?;

    // Rebuild with a fresh blockhash before signing.
    let blockhash = get_latest_blockhash(rpc_url, commitment)?;
    let message = build_message(&payer_addr, &final_ixs, &blockhash);
    let signature = payer.sign(&message);

    let signed_tx = build_signed_wire_tx(&message, &signature.to_bytes());
    let tx_sig = send_transaction(rpc_url, &signed_tx)?;
    confirm_transaction(rpc_url, &tx_sig)?;

    Ok(tx_sig)
}

fn simulate_transaction(rpc_url: &str, commitment: &str, wire_tx: &[u8]) -> Result<(), String> {
    let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, wire_tx);
    let result = rpc_post(
        rpc_url,
        "simulateTransaction",
        json!([b64, {"encoding": "base64", "sigVerify": false, "commitment": commitment}]),
    )?;

    let err = &result["value"]["err"];
    if !err.is_null() {
        let logs = result["value"]["logs"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str())
                    .collect::<Vec<_>>()
                    .join("\n  ")
            })
            .unwrap_or_default();
        return Err(format!("simulation failed: {err}\n  {logs}"));
    }

    Ok(())
}

pub struct RpcAdvanceSender<'a> {
    pub rpc_url: &'a str,
    pub commitment: &'a str,
    pub payer: &'a SigningKey,
    pub priority_fee_micro_lamports: u64,
}

impl AdvanceSender for RpcAdvanceSender<'_> {
    type Error = String;

    fn send_persisted_advance(
        &mut self,
        advance: &PersistedAdvance,
    ) -> Result<String, Self::Error> {
        simulate_sign_send(
            self.rpc_url,
            self.commitment,
            self.payer,
            &[advance.advance_instruction()],
            self.priority_fee_micro_lamports,
        )
    }
}

fn send_transaction(rpc_url: &str, wire_tx: &[u8]) -> Result<String, String> {
    let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, wire_tx);
    let result = rpc_post(
        rpc_url,
        "sendTransaction",
        json!([b64, {"encoding": "base64", "skipPreflight": true}]),
    )?;
    result
        .as_str()
        .map(|s| s.to_string())
        .ok_or("unexpected sendTransaction response".to_string())
}

fn confirm_transaction(rpc_url: &str, signature: &str) -> Result<(), String> {
    for _ in 0..60 {
        std::thread::sleep(std::time::Duration::from_millis(500));
        let result = rpc_post(rpc_url, "getSignatureStatuses", json!([[signature]]))?;

        if let Some(status) = result["value"][0].as_object() {
            if status.get("err").is_some() && !status["err"].is_null() {
                return Err(format!("transaction failed: {:?}", status["err"]));
            }
            if let Some(conf) = status.get("confirmationStatus") {
                let level = conf.as_str().unwrap_or("");
                if level == "confirmed" || level == "finalized" {
                    return Ok(());
                }
            }
        }
    }
    Err("transaction confirmation timeout (30s)".to_string())
}

// ── Message / Transaction building ───────────────────────────────────

/// Build a Solana legacy message (v0 not needed for our tx sizes).
fn build_message(
    payer: &Address,
    instructions: &[solana_instruction::Instruction],
    blockhash: &[u8; 32],
) -> Vec<u8> {
    // Collect and deduplicate accounts.
    let mut accounts: Vec<AccountEntry> = Vec::new();

    // Payer is always first, signer + writable.
    upsert(&mut accounts, payer, true, true);

    for ix in instructions {
        // Program ID: not signer, not writable.
        upsert(&mut accounts, &ix.program_id, false, false);
        for meta in &ix.accounts {
            upsert(
                &mut accounts,
                &meta.pubkey,
                meta.is_signer,
                meta.is_writable,
            );
        }
    }

    // Sort: signer+writable, signer+readonly, nonsigner+writable, nonsigner+readonly.
    accounts.sort_by(|a, b| {
        let rank = |e: &AccountEntry| -> u8 {
            match (e.is_signer, e.is_writable) {
                (true, true) => 0,
                (true, false) => 1,
                (false, true) => 2,
                (false, false) => 3,
            }
        };
        rank(a).cmp(&rank(b))
    });

    // Count header values.
    let num_required_sigs = accounts.iter().filter(|a| a.is_signer).count() as u8;
    let num_readonly_signed = accounts
        .iter()
        .filter(|a| a.is_signer && !a.is_writable)
        .count() as u8;
    let num_readonly_unsigned = accounts
        .iter()
        .filter(|a| !a.is_signer && !a.is_writable)
        .count() as u8;

    // Build the message.
    let mut msg = Vec::new();

    // Header: 3 bytes.
    msg.push(num_required_sigs);
    msg.push(num_readonly_signed);
    msg.push(num_readonly_unsigned);

    // Account addresses.
    encode_compact_u16(&mut msg, accounts.len() as u16);
    for acc in &accounts {
        msg.extend_from_slice(acc.pubkey.as_array());
    }

    // Recent blockhash.
    msg.extend_from_slice(blockhash);

    // Instructions.
    encode_compact_u16(&mut msg, instructions.len() as u16);
    for ix in instructions {
        let prog_idx = accounts
            .iter()
            .position(|a| a.pubkey == ix.program_id)
            .unwrap() as u8;
        msg.push(prog_idx);

        encode_compact_u16(&mut msg, ix.accounts.len() as u16);
        for meta in &ix.accounts {
            let idx = accounts
                .iter()
                .position(|a| a.pubkey == meta.pubkey)
                .unwrap() as u8;
            msg.push(idx);
        }

        encode_compact_u16(&mut msg, ix.data.len() as u16);
        msg.extend_from_slice(&ix.data);
    }

    msg
}

fn build_unsigned_wire_tx(message: &[u8], num_sigs: usize) -> Vec<u8> {
    let mut tx = Vec::new();
    encode_compact_u16(&mut tx, num_sigs as u16);
    // Zeroed signatures for simulation.
    for _ in 0..num_sigs {
        tx.extend_from_slice(&[0u8; 64]);
    }
    tx.extend_from_slice(message);
    tx
}

fn build_signed_wire_tx(message: &[u8], signature: &[u8; 64]) -> Vec<u8> {
    let mut tx = Vec::new();
    encode_compact_u16(&mut tx, 1u16);
    tx.extend_from_slice(signature);
    tx.extend_from_slice(message);
    tx
}

struct AccountEntry {
    pubkey: Address,
    is_signer: bool,
    is_writable: bool,
}

fn upsert(accounts: &mut Vec<AccountEntry>, pubkey: &Address, is_signer: bool, is_writable: bool) {
    if let Some(existing) = accounts.iter_mut().find(|a| a.pubkey == *pubkey) {
        existing.is_signer |= is_signer;
        existing.is_writable |= is_writable;
    } else {
        accounts.push(AccountEntry {
            pubkey: *pubkey,
            is_signer,
            is_writable,
        });
    }
}

fn encode_compact_u16(buf: &mut Vec<u8>, val: u16) {
    let mut v = val;
    loop {
        let mut byte = (v & 0x7F) as u8;
        v >>= 7;
        if v > 0 {
            byte |= 0x80;
        }
        buf.push(byte);
        if v == 0 {
            break;
        }
    }
}

fn bs58_decode_32(s: &str) -> Result<[u8; 32], String> {
    let bytes = bs58::decode(s)
        .into_vec()
        .map_err(|e| format!("bs58 decode error: {e}"))?;
    bytes
        .try_into()
        .map_err(|_| "expected 32 bytes from bs58".to_string())
}
