use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::fs;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
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
    state_path_in(&state_dir(), wallet_id_hex)
}

fn state_path_in(dir: &Path, wallet_id_hex: &str) -> PathBuf {
    dir.join(format!("{wallet_id_hex}.json"))
}

fn lock_path_in(dir: &Path, wallet_id_hex: &str) -> PathBuf {
    dir.join(format!("{wallet_id_hex}.lock"))
}

fn ensure_state_dir(dir: &Path) -> Result<(), String> {
    if !dir.exists() {
        fs::create_dir_all(dir).map_err(|e| format!("failed to create {}: {e}", dir.display()))?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))
            .map_err(|e| format!("failed to set dir permissions: {e}"))?;
    }
    Ok(())
}

/// Load wallet state from disk. Returns `None` if the file does not exist.
pub fn load(wallet_id_hex: &str) -> Result<Option<WalletState>, String> {
    load_in_dir(&state_dir(), wallet_id_hex)
}

fn load_in_dir(dir: &Path, wallet_id_hex: &str) -> Result<Option<WalletState>, String> {
    let path = state_path_in(dir, wallet_id_hex);
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
    save_in_dir(&state_dir(), state)
}

fn save_in_dir(dir: &Path, state: &WalletState) -> Result<(), String> {
    ensure_state_dir(dir)?;

    let json = serde_json::to_string_pretty(state)
        .map_err(|e| format!("failed to serialize state: {e}"))?;

    let path = state_path_in(dir, &state.wallet_id);
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
            let dir_file = fs::File::open(dir)
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

/// Held exclusive lock for one wallet's local state.
pub struct WalletLock {
    file: File,
}

impl WalletLock {
    fn new(file: File) -> Self {
        Self { file }
    }
}

impl Drop for WalletLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

/// Acquire an exclusive non-blocking lock for a wallet's local state.
pub fn acquire_lock(wallet_id_hex: &str) -> Result<WalletLock, String> {
    acquire_lock_in_dir(&state_dir(), wallet_id_hex)
}

fn acquire_lock_in_dir(dir: &Path, wallet_id_hex: &str) -> Result<WalletLock, String> {
    ensure_state_dir(dir)?;
    let path = lock_path_in(dir, wallet_id_hex);
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&path)
        .map_err(|e| format!("failed to open state lock {}: {e}", path.display()))?;
    file.try_lock_exclusive().map_err(|e| {
        format!(
            "wallet state is locked by another process ({}): {e}",
            path.display()
        )
    })?;
    Ok(WalletLock::new(file))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_load_roundtrip_uses_atomic_path() {
        let dir = temp_state_dir("roundtrip");
        let state = WalletState {
            wallet_id: "aa".repeat(32),
            pda: "pda".to_string(),
            parent: 7,
            child: 9,
        };

        save_in_dir(&dir, &state).unwrap();
        let loaded = load_in_dir(&dir, &state.wallet_id).unwrap().unwrap();

        assert_eq!(loaded.wallet_id, state.wallet_id);
        assert_eq!(loaded.pda, state.pda);
        assert_eq!(loaded.parent, 7);
        assert_eq!(loaded.child, 9);
        assert!(state_path_in(&dir, &state.wallet_id).exists());
    }

    #[test]
    fn wallet_lock_file_can_be_acquired() {
        let dir = temp_state_dir("lock");
        let wallet_id = "bb".repeat(32);
        let _lock = acquire_lock_in_dir(&dir, &wallet_id).unwrap();
        assert!(lock_path_in(&dir, &wallet_id).exists());
    }

    fn temp_state_dir(name: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "winterwallet-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        dir
    }
}
