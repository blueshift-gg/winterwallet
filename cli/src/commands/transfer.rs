use winterwallet_client::{
    AdvancePlan, WinterWalletAccount, find_wallet_address, wallet_id_from_mnemonic,
};
use winterwallet_common::{SIGNATURE_LEN, WINTERNITZ_SCALARS};
use winterwallet_core::WinternitzKeypair;

use crate::helpers;
use crate::state::{self, WalletState};

/// Build an SPL Token Transfer instruction (discriminator 3, amount as u64 LE).
fn spl_transfer_ix(
    source: &solana_address::Address,
    destination: &solana_address::Address,
    authority: &solana_address::Address,
    amount: u64,
    token_program: &solana_address::Address,
) -> solana_instruction::Instruction {
    let mut data = Vec::with_capacity(9);
    data.push(3); // SPL Token Transfer
    data.extend_from_slice(&amount.to_le_bytes());

    solana_instruction::Instruction {
        program_id: *token_program,
        accounts: vec![
            solana_instruction::AccountMeta::new(*source, false),
            solana_instruction::AccountMeta::new(*destination, false),
            // Authority: is_signer=false because invoke_signed promotes the PDA.
            solana_instruction::AccountMeta::new_readonly(*authority, false),
        ],
        data,
    }
}

pub struct RunArgs<'a> {
    pub keypair_path: &'a str,
    pub to: &'a str,
    pub mint: &'a str,
    pub amount: u64,
    pub rpc_url: &'a str,
    pub token_program: &'a str,
    pub json_output: bool,
    pub dry_run: bool,
}

pub fn run(args: RunArgs<'_>) -> Result<(), String> {
    let RunArgs {
        keypair_path,
        to,
        mint,
        amount,
        rpc_url,
        token_program,
        json_output,
        dry_run,
    } = args;

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

    let mut keypair =
        WinternitzKeypair::from_mnemonic_at(&mnemonic, 0, local_state.parent, local_state.child)
            .map_err(|e| format!("invalid mnemonic: {e}"))?;

    let current_root = keypair
        .derive::<WINTERNITZ_SCALARS>()
        .to_pubkey()
        .merklize();
    if current_root != on_chain.root {
        return Err(
            "on-chain root does not match local state — run `winterwallet recover`".to_string(),
        );
    }

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

    let token_program: solana_address::Address = token_program
        .parse()
        .map_err(|e| format!("invalid token program: {e}"))?;
    let mint_addr: solana_address::Address =
        mint.parse().map_err(|e| format!("invalid mint: {e}"))?;
    let dest_owner: solana_address::Address =
        to.parse().map_err(|e| format!("invalid recipient: {e}"))?;

    let source_ata = derive_ata(&pda, &mint_addr, &token_program);
    let dest_ata = derive_ata(&dest_owner, &mint_addr, &token_program);

    let inner = spl_transfer_ix(&source_ata, &dest_ata, &pda, amount, &token_program);
    let plan = AdvancePlan::new(&pda, new_root.as_bytes(), &[inner]).map_err(|e| format!("{e}"))?;

    let zero_sig = [0u8; SIGNATURE_LEN];
    let preview_ix = plan.instruction(&zero_sig);
    let preview = helpers::transaction_preview(&payer, &[preview_ix])?;

    if dry_run {
        if json_output {
            println!(
                "{}",
                serde_json::json!({
                    "dry_run": true,
                    "action": "transfer",
                    "wallet_id": wallet_id_hex,
                    "pda": pda.to_string(),
                    "recipient_owner": dest_owner.to_string(),
                    "mint": mint_addr.to_string(),
                    "amount": amount,
                    "source_ata": source_ata.to_string(),
                    "destination_ata": dest_ata.to_string(),
                    "token_program": token_program.to_string(),
                    "next_position": {
                        "parent": local_state.parent,
                        "child": local_state.child + 1,
                    },
                    "estimated_transaction_size": preview.estimated_size,
                    "compute_unit_limit": preview.compute_unit_limit,
                })
            );
        } else {
            println!("Dry run: token transfer");
            println!("  Amount:    {amount}");
            println!("  Source:    {source_ata}");
            println!("  Dest:      {dest_ata}");
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
                "action": "transfer",
                "wallet_id": new_state.wallet_id,
                "pda": pda.to_string(),
                "recipient_owner": dest_owner.to_string(),
                "mint": mint_addr.to_string(),
                "amount": amount,
                "source_ata": source_ata.to_string(),
                "destination_ata": dest_ata.to_string(),
                "token_program": token_program.to_string(),
                "signature": signature,
                "position": {
                    "parent": new_state.parent,
                    "child": new_state.child,
                },
                "estimated_transaction_size": preview.estimated_size,
            })
        );
    } else {
        println!("Token transfer successful!");
        println!("  Amount:    {amount}");
        println!("  Source:    {source_ata}");
        println!("  Dest:      {dest_ata}");
        println!("  Tx:        {signature}");
        println!("  Position:  ({}, {})", new_state.parent, new_state.child);
    }

    Ok(())
}

fn derive_ata(
    owner: &solana_address::Address,
    mint: &solana_address::Address,
    token_program: &solana_address::Address,
) -> solana_address::Address {
    let ata_program: solana_address::Address = "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL"
        .parse()
        .expect("valid ATA program ID");

    let (addr, _bump) = solana_address::Address::find_program_address(
        &[owner.as_array(), token_program.as_array(), mint.as_array()],
        &ata_program,
    );
    addr
}
