use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Persisted local wallet state. Tracks the current key position so the
/// CLI knows which Winternitz position to sign with next.
///
/// The mnemonic is NEVER stored on disk.
#[derive(Serialize, Deserialize)]
pub struct WalletState {
    /// Hex-encoded wallet ID (32 bytes → 64 hex chars).
    pub wallet_id: String,
    /// Base58-encoded wallet PDA address.
    pub pda: String,
    /// Current parent index (always 0 in practice).
    pub parent: u32,
    /// Current child index — the next position to sign with.
    pub child: u32,
}

fn state_dir() -> PathBuf {
    dirs_or_default()
}

fn dirs_or_default() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".winterwallet")
}

fn state_path(wallet_id_hex: &str) -> PathBuf {
    state_dir().join(format!("{wallet_id_hex}.json"))
}

/// Load wallet state from disk. Returns `None` if the file does not exist.
pub fn load(wallet_id_hex: &str) -> Result<Option<WalletState>, String> {
    let path = state_path(wallet_id_hex);
    if !path.exists() {
        return Ok(None);
    }
    let contents = fs::read_to_string(&path)
        .map_err(|e| format!("failed to read state file {}: {e}", path.display()))?;
    let state: WalletState =
        serde_json::from_str(&contents).map_err(|e| format!("failed to parse state file: {e}"))?;
    Ok(Some(state))
}

/// Save wallet state to disk. Creates the directory if needed.
/// Sets directory permissions to 0700 and file permissions to 0600.
pub fn save(state: &WalletState) -> Result<(), String> {
    let dir = state_dir();
    if !dir.exists() {
        fs::create_dir_all(&dir).map_err(|e| format!("failed to create {}: {e}", dir.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))
                .map_err(|e| format!("failed to set dir permissions: {e}"))?;
        }
    }

    let json = serde_json::to_string_pretty(state)
        .map_err(|e| format!("failed to serialize state: {e}"))?;

    let path = state_path(&state.wallet_id);
    fs::write(&path, &json)
        .map_err(|e| format!("failed to write state file {}: {e}", path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))
            .map_err(|e| format!("failed to set file permissions: {e}"))?;
    }

    Ok(())
}

/// Hex-encode a 32-byte wallet ID.
pub fn hex_encode(bytes: &[u8; 32]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
