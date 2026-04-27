use winterwallet_client::{find_wallet_address, wallet_id_from_mnemonic};
use winterwallet_core::WinternitzKeypair;
use zeroize::{Zeroize, Zeroizing};

use crate::state;

pub fn run(json_output: bool) -> Result<(), String> {
    let mut entropy = [0u8; 32];
    getrandom::fill(&mut entropy).map_err(|e| format!("failed to get entropy: {e}"))?;

    let words = WinternitzKeypair::generate_mnemonic(entropy);
    entropy.zeroize();

    let mnemonic = Zeroizing::new(words.join(" "));

    let wallet_id = wallet_id_from_mnemonic(&mnemonic)
        .map_err(|e| format!("failed to derive wallet ID: {e}"))?;
    let (pda, _bump) = find_wallet_address(&wallet_id);

    let wallet_id_hex = state::hex_encode(&wallet_id);

    if json_output {
        println!(
            "{}",
            serde_json::json!({
                "mnemonic": &*mnemonic,
                "wallet_id": wallet_id_hex,
                "pda": pda.to_string(),
            })
        );
    } else {
        println!("Mnemonic (write this down, it will NOT be shown again):\n");
        println!("  {}\n", &*mnemonic);
        println!("Wallet ID:  {wallet_id_hex}");
        println!("PDA:        {pda}");
    }

    Ok(())
}
