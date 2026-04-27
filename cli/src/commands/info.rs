use winterwallet_client::{WinterWalletAccount, find_wallet_address, wallet_id_from_mnemonic};

use crate::helpers;
use crate::state;

pub fn run(rpc_url: &str, commitment: &str, json_output: bool) -> Result<(), String> {
    let mnemonic = helpers::read_mnemonic()?;

    let wallet_id = wallet_id_from_mnemonic(&mnemonic)
        .map_err(|e| format!("failed to derive wallet ID: {e}"))?;

    let wallet_id_hex = state::hex_encode(&wallet_id);
    let (pda, bump) = find_wallet_address(&wallet_id);

    let local_state = state::load(&wallet_id_hex)?;
    let account = match helpers::get_account(rpc_url, commitment, &pda) {
        Ok(account) => Some(account),
        Err(e) if e.starts_with("account not found") => None,
        Err(e) => return Err(e),
    };
    let wallet = account
        .as_ref()
        .map(|a| WinterWalletAccount::from_bytes(&a.data))
        .transpose()
        .map_err(|e| format!("corrupted wallet account: {e}"))?;

    if json_output {
        println!(
            "{}",
            serde_json::json!({
                "wallet_id": wallet_id_hex,
                "pda": pda.to_string(),
                "bump": bump,
                "position": local_state.as_ref().map(|s| serde_json::json!({
                    "parent": s.parent,
                    "child": s.child,
                })),
                "on_chain": account.as_ref().map(|a| serde_json::json!({
                    "balance_lamports": a.lamports,
                    "root": wallet.as_ref().map(|w| hex_encode(w.root.as_bytes())),
                })),
            })
        );
    } else {
        println!("Wallet ID:  {wallet_id_hex}");
        println!("PDA:        {pda}");
        println!("Bump:       {bump}");
        match local_state {
            Some(local) => println!("Position:   {}", local.child),
            None => println!("Position:   (no local state)"),
        }
        match account {
            Some(account) => {
                println!(
                    "Balance:    {} lamports ({:.9} SOL)",
                    account.lamports,
                    account.lamports as f64 / 1e9
                );
                if let Some(wallet) = wallet {
                    println!("Root:       {}", hex_encode(wallet.root.as_bytes()));
                }
            }
            None => println!("On-chain:   not initialized"),
        }
    }

    Ok(())
}

fn hex_encode(bytes: &[u8; 32]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
