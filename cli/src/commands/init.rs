use winterwallet_client::{find_wallet_address, wallet_id_from_mnemonic};
use winterwallet_common::{SIGNATURE_LEN, WINTERNITZ_SCALARS};
use winterwallet_core::WinternitzKeypair;

use crate::helpers;
use crate::state::{self, WalletState};

pub fn run(
    keypair_path: &str,
    rpc_url: &str,
    json_output: bool,
    dry_run: bool,
) -> Result<(), String> {
    let payer = helpers::read_keypair(keypair_path)?;
    let mnemonic = helpers::read_mnemonic()?;

    let wallet_id = wallet_id_from_mnemonic(&mnemonic)
        .map_err(|e| format!("failed to derive wallet ID: {e}"))?;
    let wallet_id_hex = state::hex_encode(&wallet_id);

    if let Some(existing) = state::load(&wallet_id_hex)? {
        return Err(format!(
            "wallet {} already initialized at position ({}, {})",
            existing.pda, existing.parent, existing.child,
        ));
    }

    let (pda, _bump) = find_wallet_address(&wallet_id);

    // Position (0,0,0) signs the Initialize preimage.
    let mut keypair = WinternitzKeypair::from_mnemonic(&mnemonic, 0)
        .map_err(|e| format!("invalid mnemonic: {e}"))?;

    let next_root = WinternitzKeypair::from_mnemonic_at(&mnemonic, 0, 0, 1)
        .map_err(|e| format!("failed to derive next position: {e}"))?
        .derive::<WINTERNITZ_SCALARS>()
        .to_pubkey()
        .merklize();

    let zero_sig = [0u8; SIGNATURE_LEN];
    let preview_ix = winterwallet_client::initialize(
        &helpers::pubkey(&payer),
        &pda,
        &zero_sig,
        next_root.as_bytes(),
    );
    let preview = helpers::transaction_preview(&payer, &[preview_ix])?;

    if dry_run {
        if json_output {
            println!(
                "{}",
                serde_json::json!({
                    "dry_run": true,
                    "action": "init",
                    "wallet_id": wallet_id_hex,
                    "pda": pda.to_string(),
                    "next_position": { "parent": 0, "child": 1 },
                    "estimated_transaction_size": preview.estimated_size,
                    "compute_unit_limit": preview.compute_unit_limit,
                })
            );
        } else {
            println!("Dry run: initialize wallet");
            println!("  PDA:       {pda}");
            println!("  Position:  (0, 1)");
            println!("  Tx size:   {} bytes", preview.estimated_size);
            println!("  CU limit:  {}", preview.compute_unit_limit);
        }
        return Ok(());
    }

    let preimage = winterwallet_client::initialize_preimage();
    let sig = keypair.sign_and_increment::<WINTERNITZ_SCALARS>(&preimage);

    let sig_bytes: &[u8; winterwallet_common::SIGNATURE_LEN] = sig
        .as_bytes()
        .try_into()
        .map_err(|_| "signature size mismatch".to_string())?;

    let ix = winterwallet_client::initialize(
        &helpers::pubkey(&payer),
        &pda,
        sig_bytes,
        next_root.as_bytes(),
    );

    // Persist position BEFORE sending (burned once signed).
    let wallet_state = WalletState {
        wallet_id: wallet_id_hex.clone(),
        pda: pda.to_string(),
        parent: keypair.parent(),
        child: keypair.child(),
    };
    state::save(&wallet_state)?;

    let signature = helpers::simulate_sign_send(rpc_url, &payer, &[ix])?;

    if json_output {
        println!(
            "{}",
            serde_json::json!({
                "action": "init",
                "wallet_id": wallet_id_hex,
                "pda": pda.to_string(),
                "signature": signature,
                "position": {
                    "parent": wallet_state.parent,
                    "child": wallet_state.child,
                },
            })
        );
    } else {
        println!("Wallet initialized!");
        println!("  PDA:       {pda}");
        println!("  Tx:        {signature}");
        println!(
            "  Position:  ({}, {})",
            wallet_state.parent, wallet_state.child
        );
    }

    Ok(())
}
