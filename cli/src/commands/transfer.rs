use winterwallet_client::{
    SigningPosition, WinterWallet, WinterWalletAccount, find_wallet_address,
    wallet_id_from_mnemonic,
};
use winterwallet_common::{SIGNATURE_LEN, WINTERNITZ_SCALARS};
use winterwallet_core::WinternitzKeypair;

use crate::helpers;
use crate::state::{self, StateStore};

pub struct RunArgs<'a> {
    pub keypair_path: &'a str,
    pub to: &'a str,
    pub mint: &'a str,
    pub amount: u64,
    pub rpc_url: &'a str,
    pub commitment: &'a str,
    pub token_program: &'a str,
    pub json_output: bool,
    pub dry_run: bool,
    pub priority_fee_micro_lamports: u64,
}

pub fn run(args: RunArgs<'_>) -> Result<(), String> {
    let RunArgs {
        keypair_path,
        to,
        mint,
        amount,
        rpc_url,
        commitment,
        token_program,
        json_output,
        dry_run,
        priority_fee_micro_lamports,
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
    let _wallet_lock = state::acquire_lock(&wallet_id_hex)?;

    let local_state = state::load(&wallet_id_hex)?
        .ok_or("no local state found — run `winterwallet init` first")?;
    let position = SigningPosition::new(0, local_state.parent, local_state.child);
    let next_position = position
        .next()
        .map_err(|e| format!("failed to derive next position: {e}"))?;

    let account = helpers::get_account(rpc_url, commitment, &pda)?;
    let on_chain = WinterWalletAccount::from_bytes(&account.data)
        .map_err(|e| format!("failed to deserialize wallet: {e}"))?;
    if on_chain.id != wallet_id {
        return Err("on-chain wallet ID does not match mnemonic-derived PDA".to_string());
    }

    let mut keypair =
        WinternitzKeypair::from_mnemonic_at(&mnemonic, 0, local_state.parent, local_state.child)
            .map_err(|e| format!("invalid mnemonic: {e}"))?;

    let new_root = WinternitzKeypair::from_mnemonic_at(
        &mnemonic,
        next_position.wallet(),
        next_position.parent(),
        next_position.child(),
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

    let wallet = WinterWallet::from_account(&on_chain, position);
    let unsigned = wallet
        .transfer_plan(
            &source_ata,
            &dest_ata,
            &token_program,
            amount,
            new_root.as_bytes(),
        )
        .map_err(|e| format!("{e}"))?;

    let zero_sig = [0u8; SIGNATURE_LEN];
    let preview_ix = unsigned.plan().instruction(&zero_sig);
    let preview = helpers::transaction_preview(&payer, &[preview_ix], priority_fee_micro_lamports)?;

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
                        "parent": next_position.parent(),
                        "child": next_position.child(),
                    },
                    "estimated_transaction_size": preview.estimated_size,
                    "compute_unit_limit": preview.compute_unit_limit,
                    "priority_fee_micro_lamports": preview.priority_fee_micro_lamports,
                    "requires_signature_before_simulation": true,
                })
            );
        } else {
            println!("Dry run: token transfer");
            println!("  Amount:    {amount}");
            println!("  Source:    {source_ata}");
            println!("  Dest:      {dest_ata}");
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
            println!(
                "  Note:      live simulation requires signing and burns position ({}, {})",
                local_state.parent, local_state.child
            );
        }
        return Ok(());
    }

    if !json_output {
        eprintln!(
            "Static checks passed. Signing burns position ({}, {}); local state will be advanced before network submission.",
            local_state.parent, local_state.child
        );
    }

    let signed = unsigned
        .sign(&mut keypair)
        .map_err(|e| format!("{e}. Run `winterwallet recover` if local state is stale."))?;
    let mut store = StateStore;
    let persisted = signed.persist(&mut store)?;
    let next_position = persisted.signed().next_position();
    let mut sender = helpers::RpcAdvanceSender {
        rpc_url,
        commitment,
        payer: &payer,
        priority_fee_micro_lamports,
    };
    let signature = persisted
        .send(&mut sender)
        .map_err(|e| format!("{e}\n  Position burned. Run `winterwallet recover` if needed."))?;

    if json_output {
        println!(
            "{}",
            serde_json::json!({
                "action": "transfer",
                "wallet_id": wallet_id_hex,
                "pda": pda.to_string(),
                "recipient_owner": dest_owner.to_string(),
                "mint": mint_addr.to_string(),
                "amount": amount,
                "source_ata": source_ata.to_string(),
                "destination_ata": dest_ata.to_string(),
                "token_program": token_program.to_string(),
                "signature": signature,
                "position": {
                    "parent": next_position.parent(),
                    "child": next_position.child(),
                },
                "estimated_transaction_size": preview.estimated_size,
                "position_persisted_before_send": true,
            })
        );
    } else {
        println!("Token transfer successful!");
        println!("  Amount:    {amount}");
        println!("  Source:    {source_ata}");
        println!("  Dest:      {dest_ata}");
        println!("  Tx:        {signature}");
        println!(
            "  Position:  ({}, {})",
            next_position.parent(),
            next_position.child()
        );
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
