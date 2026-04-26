use winterwallet_client::{WinterWalletAccount, find_wallet_address, wallet_id_from_mnemonic};
use winterwallet_common::WINTERNITZ_SCALARS;
use winterwallet_core::WinternitzKeypair;

use crate::helpers;
use crate::state::{self, WalletState};

pub fn run(rpc_url: &str, max_depth: u32, json_output: bool) -> Result<(), String> {
    let mnemonic = helpers::read_mnemonic()?;

    let wallet_id = wallet_id_from_mnemonic(&mnemonic)
        .map_err(|e| format!("failed to derive wallet ID: {e}"))?;
    let wallet_id_hex = state::hex_encode(&wallet_id);
    let (pda, _bump) = find_wallet_address(&wallet_id);

    let account = helpers::get_account(rpc_url, &pda)?;
    let on_chain = WinterWalletAccount::from_bytes(&account.data)
        .map_err(|e| format!("failed to deserialize wallet: {e}"))?;

    let on_chain_root = on_chain.root;

    eprintln!("Scanning positions 1 through {max_depth}...");

    let mut found = None;
    for child in 1..=max_depth {
        let kp = WinternitzKeypair::from_mnemonic_at(&mnemonic, 0, 0, child)
            .map_err(|e| format!("derivation error: {e}"))?;
        let root = kp.derive::<WINTERNITZ_SCALARS>().to_pubkey().merklize();

        if root == on_chain_root {
            found = Some(child);
            break;
        }

        if child % 1000 == 0 {
            eprintln!("  scanned {child} positions...");
        }
    }

    match found {
        Some(child) => {
            let wallet_state = WalletState {
                wallet_id: wallet_id_hex.clone(),
                pda: pda.to_string(),
                parent: 0,
                child,
            };
            state::save(&wallet_state)?;

            if json_output {
                println!(
                    "{}",
                    serde_json::json!({
                        "action": "recover",
                        "wallet_id": wallet_id_hex,
                        "pda": pda.to_string(),
                        "position": {
                            "parent": 0,
                            "child": child,
                        },
                    })
                );
            } else {
                println!("Recovery successful!");
                println!("  Next signing position: {child}");
                println!("  PDA: {pda}");
            }
        }
        None => {
            return Err(format!(
                "position not found within scan depth {max_depth}. \
                 Verify your mnemonic is correct, or increase --max-depth."
            ));
        }
    }

    Ok(())
}
