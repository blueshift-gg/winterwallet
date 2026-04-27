use winterwallet_client::{SigningPosition, find_wallet_address, wallet_id_from_mnemonic};
use winterwallet_common::{SIGNATURE_LEN, WINTERNITZ_SCALARS};
use winterwallet_core::WinternitzKeypair;

use crate::helpers;
use crate::state::{self, WalletState};

pub struct RunArgs<'a> {
    pub keypair_path: &'a str,
    pub rpc_url: &'a str,
    pub commitment: &'a str,
    pub json_output: bool,
    pub dry_run: bool,
    pub priority_fee_micro_lamports: u64,
}

pub fn run(args: RunArgs<'_>) -> Result<(), String> {
    let RunArgs {
        keypair_path,
        rpc_url,
        commitment,
        json_output,
        dry_run,
        priority_fee_micro_lamports,
    } = args;

    let payer = helpers::read_keypair(keypair_path)?;
    let mnemonic = helpers::read_mnemonic()?;

    let wallet_id = wallet_id_from_mnemonic(&mnemonic)
        .map_err(|e| format!("failed to derive wallet ID: {e}"))?;
    let wallet_id_hex = state::hex_encode(&wallet_id);
    let _wallet_lock = state::acquire_lock(&wallet_id_hex)?;

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
    let next_position = SigningPosition::new(0, 0, 0)
        .next()
        .map_err(|e| format!("failed to derive next position: {e}"))?;

    let next_root = WinternitzKeypair::from_mnemonic_at(
        &mnemonic,
        next_position.wallet(),
        next_position.parent(),
        next_position.child(),
    )
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
    let preview = helpers::transaction_preview(&payer, &[preview_ix], priority_fee_micro_lamports)?;

    if dry_run {
        if json_output {
            println!(
                "{}",
                serde_json::json!({
                    "dry_run": true,
                    "action": "init",
                    "wallet_id": wallet_id_hex,
                    "pda": pda.to_string(),
                    "next_position": { "parent": next_position.parent(), "child": next_position.child() },
                    "estimated_transaction_size": preview.estimated_size,
                    "compute_unit_limit": preview.compute_unit_limit,
                    "priority_fee_micro_lamports": preview.priority_fee_micro_lamports,
                    "requires_signature_before_simulation": true,
                })
            );
        } else {
            println!("Dry run: initialize wallet");
            println!("  PDA:       {pda}");
            println!(
                "  Position:  ({}, {})",
                next_position.parent(),
                next_position.child()
            );
            println!("  Tx size:   {} bytes", preview.estimated_size);
            println!("  CU limit:  {}", preview.compute_unit_limit);
            println!(
                "  Priority:  {} micro-lamports/CU",
                preview.priority_fee_micro_lamports
            );
            println!("  Note:      live simulation requires signing and burns position (0, 0)");
        }
        return Ok(());
    }

    if !json_output {
        eprintln!(
            "Static checks passed. Signing burns position (0, 0); local state will be advanced before network submission."
        );
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

    let signature = helpers::simulate_sign_send(
        rpc_url,
        commitment,
        &payer,
        &[ix],
        priority_fee_micro_lamports,
    )?;

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
                "position_persisted_before_send": true,
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
