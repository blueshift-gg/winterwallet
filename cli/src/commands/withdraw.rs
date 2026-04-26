use winterwallet_client::{
    AdvancePlan, WinterWalletAccount, find_wallet_address, wallet_id_from_mnemonic,
};
use winterwallet_common::{SIGNATURE_LEN, WINTERNITZ_SCALARS};
use winterwallet_core::WinternitzKeypair;

use crate::helpers;
use crate::state::{self, WalletState};

pub fn run(
    keypair_path: &str,
    to: &str,
    amount: u64,
    rpc_url: &str,
    json_output: bool,
    dry_run: bool,
) -> Result<(), String> {
    if amount == 0 {
        return Err("amount must be greater than zero".to_string());
    }

    let payer = helpers::read_keypair(keypair_path)?;
    let mnemonic = helpers::read_mnemonic()?;

    let wallet_id = wallet_id_from_mnemonic(&mnemonic)
        .map_err(|e| format!("failed to derive wallet ID: {e}"))?;
    let wallet_id_hex = state::hex_encode(&wallet_id);
    let (pda, _bump) = find_wallet_address(&wallet_id);

    let local_state = state::load(&wallet_id_hex)?
        .ok_or("no local state found — run `winterwallet init` first")?;

    let account = helpers::get_account(rpc_url, &pda)?;
    let on_chain = WinterWalletAccount::from_bytes(&account.data)
        .map_err(|e| format!("failed to deserialize wallet: {e}"))?;

    // Resume keypair at stored position.
    let mut keypair =
        WinternitzKeypair::from_mnemonic_at(&mnemonic, 0, local_state.parent, local_state.child)
            .map_err(|e| format!("invalid mnemonic: {e}"))?;

    // Verify root matches on-chain.
    let current_root = keypair
        .derive::<WINTERNITZ_SCALARS>()
        .to_pubkey()
        .merklize();
    if current_root != on_chain.root {
        return Err(
            "on-chain root does not match local state — run `winterwallet recover`".to_string(),
        );
    }

    // Root for the position AFTER signing.
    let new_root = WinternitzKeypair::from_mnemonic_at(
        &mnemonic,
        0,
        local_state.parent,
        local_state.child + 1,
    )
    .map_err(|e| format!("failed to derive next position: {e}"))?
    .derive::<WINTERNITZ_SCALARS>()
    .to_pubkey()
    .merklize();

    let receiver: solana_address::Address = to
        .parse()
        .map_err(|e| format!("invalid receiver address: {e}"))?;

    let plan = AdvancePlan::withdraw(&pda, &receiver, amount, new_root.as_bytes())
        .map_err(|e| format!("advance plan error: {e}"))?;

    let zero_sig = [0u8; SIGNATURE_LEN];
    let preview_ix = plan.instruction(&zero_sig);
    let preview = helpers::transaction_preview(&payer, &[preview_ix])?;

    if dry_run {
        if json_output {
            println!(
                "{}",
                serde_json::json!({
                    "dry_run": true,
                    "action": "withdraw",
                    "wallet_id": wallet_id_hex,
                    "pda": pda.to_string(),
                    "receiver": receiver.to_string(),
                    "amount": amount,
                    "next_position": {
                        "parent": local_state.parent,
                        "child": local_state.child + 1,
                    },
                    "estimated_transaction_size": preview.estimated_size,
                    "compute_unit_limit": preview.compute_unit_limit,
                })
            );
        } else {
            println!("Dry run: withdraw");
            println!("  Amount:    {amount} lamports");
            println!("  Receiver:  {receiver}");
            println!(
                "  Position:  ({}, {})",
                local_state.parent,
                local_state.child + 1
            );
            println!("  Tx size:   {} bytes", preview.estimated_size);
            println!("  CU limit:  {}", preview.compute_unit_limit);
        }
        return Ok(());
    }

    let preimage = plan.preimage(&wallet_id, current_root.as_bytes());

    let sig = keypair.sign_and_increment::<WINTERNITZ_SCALARS>(&preimage);
    let sig_bytes: &[u8; SIGNATURE_LEN] = sig
        .as_bytes()
        .try_into()
        .map_err(|_| "signature size mismatch")?;

    let advance_ix = plan.instruction(sig_bytes);

    // Persist BEFORE sending.
    let new_state = WalletState {
        wallet_id: wallet_id_hex,
        pda: pda.to_string(),
        parent: keypair.parent(),
        child: keypair.child(),
    };
    state::save(&new_state)?;

    let signature = helpers::simulate_sign_send(rpc_url, &payer, &[advance_ix])
        .map_err(|e| format!("{e}\n  Position burned. Run `winterwallet recover` if needed."))?;

    if json_output {
        println!(
            "{}",
            serde_json::json!({
                "action": "withdraw",
                "wallet_id": new_state.wallet_id,
                "pda": pda.to_string(),
                "receiver": receiver.to_string(),
                "amount": amount,
                "signature": signature,
                "position": {
                    "parent": new_state.parent,
                    "child": new_state.child,
                },
                "estimated_transaction_size": preview.estimated_size,
            })
        );
    } else {
        println!("Withdrawal successful!");
        println!("  Amount:    {amount} lamports");
        println!("  Receiver:  {to}");
        println!("  Tx:        {signature}");
        println!("  Position:  ({}, {})", new_state.parent, new_state.child);
    }

    Ok(())
}
