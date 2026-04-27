use winterwallet_client::wallet_id_from_mnemonic;

use crate::helpers;
use crate::state;

pub fn show(json_output: bool) -> Result<(), String> {
    let mnemonic = helpers::read_mnemonic()?;
    let wallet_id = wallet_id_from_mnemonic(&mnemonic)
        .map_err(|e| format!("failed to derive wallet ID: {e}"))?;
    let wallet_id_hex = state::hex_encode(&wallet_id);
    let path = state::state_path(&wallet_id_hex);
    let local_state = state::load(&wallet_id_hex)?;

    if json_output {
        println!(
            "{}",
            serde_json::json!({
                "wallet_id": wallet_id_hex,
                "path": path.display().to_string(),
                "state": local_state.as_ref().map(|s| serde_json::json!({
                    "pda": s.pda,
                    "parent": s.parent,
                    "child": s.child,
                })),
            })
        );
    } else {
        println!("Wallet ID: {wallet_id_hex}");
        println!("Path:      {}", path.display());
        match local_state {
            Some(local) => {
                println!("PDA:       {}", local.pda);
                println!("Position:  ({}, {})", local.parent, local.child);
            }
            None => println!("State:     not found"),
        }
    }

    Ok(())
}

pub fn path(json_output: bool) -> Result<(), String> {
    let dir = state::state_dir();
    if json_output {
        println!(
            "{}",
            serde_json::json!({
                "state_dir": dir.display().to_string(),
            })
        );
    } else {
        println!("{}", dir.display());
    }
    Ok(())
}
