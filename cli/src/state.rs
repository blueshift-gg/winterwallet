use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use winterwallet_client::{AdvancePersistence, SignedAdvance};

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

pub fn state_dir() -> PathBuf {
    dirs_or_default()
}

fn dirs_or_default() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".winterwallet")
}

pub fn state_path(wallet_id_hex: &str) -> PathBuf {
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
/// Sets directory permissions to 0700 and file permissions to 0600. Writes are
/// temp-file + fsync + rename so an interrupted process does not corrupt the
/// current position file.
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
    let tmp_path = path.with_file_name(format!(
        ".{}.tmp.{}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("state.json"),
        std::process::id()
    ));

    let write_result = (|| -> Result<(), String> {
        let mut file = fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&tmp_path)
            .map_err(|e| format!("failed to open temp state file {}: {e}", tmp_path.display()))?;

        file.write_all(json.as_bytes()).map_err(|e| {
            format!(
                "failed to write temp state file {}: {e}",
                tmp_path.display()
            )
        })?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(fs::Permissions::from_mode(0o600))
                .map_err(|e| format!("failed to set temp file permissions: {e}"))?;
        }

        file.sync_all()
            .map_err(|e| format!("failed to sync temp state file {}: {e}", tmp_path.display()))?;
        drop(file);

        fs::rename(&tmp_path, &path).map_err(|e| {
            format!(
                "failed to replace state file {} with {}: {e}",
                path.display(),
                tmp_path.display()
            )
        })?;

        #[cfg(unix)]
        {
            let dir_file = fs::File::open(&dir)
                .map_err(|e| format!("failed to open state dir {}: {e}", dir.display()))?;
            dir_file
                .sync_all()
                .map_err(|e| format!("failed to sync state dir {}: {e}", dir.display()))?;
        }

        Ok(())
    })();

    if write_result.is_err() {
        let _ = fs::remove_file(&tmp_path);
    }
    write_result?;

    Ok(())
}

/// Hex-encode a 32-byte wallet ID.
pub fn hex_encode(bytes: &[u8; 32]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Adapter used by the client SDK state machine.
pub struct StateStore;

impl AdvancePersistence for StateStore {
    type Error = String;

    fn persist_signed_advance(&mut self, advance: &SignedAdvance) -> Result<(), Self::Error> {
        let next = advance.next_position();
        save(&WalletState {
            wallet_id: hex_encode(advance.wallet_id()),
            pda: advance.wallet_pda().to_string(),
            parent: next.parent(),
            child: next.child(),
        })
    }
}
