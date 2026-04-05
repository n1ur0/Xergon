//! Xergon wallet management
//!
//! Generates a random 15-word BIP-39 mnemonic, derives a secret key, and stores
//! an encrypted wallet at `~/.xergon/wallet.json`.
//!
//! Phase 3 MVP: secret key = blake2b256(seed_bytes), public key = blake2b256(secret_key).
//! This is NOT a real secp256k1 curve point (that requires ergo-lib), but it is
//! sufficient for relay request authentication.

use anyhow::{Context, Result};
use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use hkdf::Hkdf;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::Sha256;

/// Wallet file stored at ~/.xergon/wallet.json
#[derive(Debug, Serialize, Deserialize)]
pub struct WalletFile {
    /// AES-256-GCM encrypted mnemonic (hex-encoded ciphertext)
    pub encrypted_mnemonic: String,
    /// Salt used for HKDF key derivation (hex)
    pub salt: String,
    /// Nonce used for AES-GCM (hex, 12 bytes)
    pub nonce: String,
    /// Public key (hex, 32 bytes) — blake2b256 of secret_key
    pub public_key: String,
    /// Ergo address placeholder — will be set by relay or ergo-lib
    pub address: String,
}

/// In-memory wallet (decrypted)
#[derive(Debug, Clone)]
pub struct Wallet {
    /// The 15-word BIP-39 mnemonic phrase
    pub mnemonic: String,
    /// Secret key (32 bytes, hex)
    pub secret_key: String,
    /// Public key (32 bytes, hex) — blake2b256(secret_key)
    pub public_key: String,
}

/// Returns the xergon config directory path (`~/.xergon`).
pub fn xergon_dir() -> Result<std::path::PathBuf> {
    let home = dirs::home_dir()
        .context("Cannot determine home directory")?;
    let dir = home.join(".xergon");
    Ok(dir)
}

/// Returns the wallet file path.
pub fn wallet_path() -> Result<std::path::PathBuf> {
    Ok(xergon_dir()?.join("wallet.json"))
}

/// Check if a wallet already exists.
pub fn wallet_exists() -> bool {
    wallet_path()
        .map(|p| p.exists())
        .unwrap_or(false)
}

/// Load wallet from disk, prompting for password to decrypt.
pub fn load_wallet(password: &str) -> Result<Wallet> {
    let path = wallet_path()?;
    let contents = std::fs::read_to_string(&path)
        .with_context(|| format!("Wallet not found at {}. Run `xergon setup` first.", path.display()))?;

    let wallet_file: WalletFile = serde_json::from_str(&contents)
        .context("Failed to parse wallet.json")?;

    decrypt_wallet(&wallet_file, password)
}

/// Load wallet using password from env var or stdin prompt.
pub fn load_wallet_interactive() -> Result<Wallet> {
    let password = get_password()?;
    load_wallet(&password)
}

/// Generate a new random 15-word BIP-39 mnemonic.
fn generate_mnemonic() -> Result<String> {
    let mut entropy = [0u8; 20]; // 128 bits for 12 words; 160 bits for 15 words
    OsRng.fill_bytes(&mut entropy);

    // bip39::Mnemonic::from_entropy with 160 bits = 15 words
    let mnemonic = bip39::Mnemonic::from_entropy(&entropy)
        .map_err(|e| anyhow::anyhow!("Failed to generate mnemonic: {}", e))?;

    Ok(mnemonic.to_string())
}

/// Derive secret key from mnemonic using blake2b256.
///
/// Takes the seed bytes from the mnemonic and hashes them to produce a 32-byte
/// secret key. This is a simplified derivation — real Ergo key derivation uses
/// BIP-32/44 on secp256k1 via ergo-lib.
fn derive_secret_key(mnemonic: &str) -> Result<[u8; 32]> {
    let mnemonic = bip39::Mnemonic::parse_normalized(mnemonic)
        .map_err(|e| anyhow::anyhow!("Invalid mnemonic: {}", e))?;

    let seed = mnemonic.to_seed_normalized(""); // empty passphrase for now
    // Blake2b256 of the seed to get a 32-byte secret key
    let hash = blake2b256(&seed);
    Ok(hash)
}

/// Derive public key from secret key using blake2b256.
///
/// NOTE: This is NOT a real secp256k1 public key. It's blake2b256(secret_key).
/// Sufficient for relay authentication in Phase 3 MVP.
fn derive_public_key(secret_key: &[u8; 32]) -> [u8; 32] {
    blake2b256(secret_key)
}

/// Generate a new wallet: create mnemonic, derive keys, encrypt, and save.
pub fn generate_wallet(password: &str) -> Result<Wallet> {
    if wallet_exists() {
        anyhow::bail!(
            "Wallet already exists at {}. Remove it first if you want to create a new one.",
            wallet_path()?.display()
        );
    }

    let mnemonic = generate_mnemonic()?;
    let secret_key = derive_secret_key(&mnemonic)?;
    let public_key = derive_public_key(&secret_key);

    let wallet = Wallet {
        mnemonic: mnemonic.clone(),
        secret_key: hex::encode(secret_key),
        public_key: hex::encode(public_key),
    };

    // Encrypt and save
    let wallet_file = encrypt_wallet(&wallet, password)?;
    save_wallet(&wallet_file)?;

    Ok(wallet)
}

/// Generate a new wallet using password from env var or stdin prompt.
pub fn generate_wallet_interactive() -> Result<Wallet> {
    println!("  Creating a new Xergon wallet...");
    println!();

    let password = get_new_password()?;

    // Confirm the password
    let confirm = rpassword::prompt_password("  Confirm password: ")
        .context("Failed to read password confirmation")?;
    if password != confirm {
        anyhow::bail!("Passwords do not match. Please try again.");
    }

    let wallet = generate_wallet(&password)?;

    println!();
    println!("  Wallet created successfully!");
    println!("  Store your mnemonic phrase in a safe place:");
    println!("  ─────────────────────────────────────────");
    for word in wallet.mnemonic.split_whitespace() {
        print!("  {}", word);
    }
    println!();
    println!("  ─────────────────────────────────────────");
    println!();
    println!("  WARNING: If you lose your mnemonic, you cannot recover your wallet.");
    println!("  Public key: {}", wallet.public_key);

    Ok(wallet)
}

/// Encrypt wallet data with AES-256-GCM using a key derived from the password.
fn encrypt_wallet(wallet: &Wallet, password: &str) -> Result<WalletFile> {
    let mut salt = [0u8; 32];
    OsRng.fill_bytes(&mut salt);

    let encryption_key = derive_encryption_key(password, &salt)?;

    let cipher = Aes256Gcm::new_from_slice(&encryption_key)
        .map_err(|e| anyhow::anyhow!("Failed to create cipher: {}", e))?;

    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let plaintext = wallet.mnemonic.as_bytes();
    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| anyhow::anyhow!("Encryption failed: {}", e))?;

    Ok(WalletFile {
        encrypted_mnemonic: hex::encode(&ciphertext),
        salt: hex::encode(salt),
        nonce: hex::encode(nonce_bytes),
        public_key: wallet.public_key.clone(),
        address: String::new(), // Will be populated by relay
    })
}

/// Decrypt wallet data.
fn decrypt_wallet(wallet_file: &WalletFile, password: &str) -> Result<Wallet> {
    let salt = hex::decode(&wallet_file.salt)
        .context("Invalid salt in wallet file")?;
    let nonce_bytes = hex::decode(&wallet_file.nonce)
        .context("Invalid nonce in wallet file")?;
    let ciphertext = hex::decode(&wallet_file.encrypted_mnemonic)
        .context("Invalid encrypted data in wallet file")?;

    let encryption_key = derive_encryption_key(password, &salt)?;

    let cipher = Aes256Gcm::new_from_slice(&encryption_key)
        .map_err(|e| anyhow::anyhow!("Failed to create cipher: {}", e))?;

    let nonce = Nonce::from_slice(&nonce_bytes);
    let plaintext = cipher
        .decrypt(nonce, ciphertext.as_ref())
        .map_err(|_| anyhow::anyhow!("Decryption failed. Wrong password?"))?;

    let mnemonic = String::from_utf8(plaintext)
        .context("Decrypted data is not valid UTF-8")?;

    let secret_key = derive_secret_key(&mnemonic)?;

    Ok(Wallet {
        mnemonic,
        secret_key: hex::encode(secret_key),
        public_key: wallet_file.public_key.clone(),
    })
}

/// Save wallet file to disk.
fn save_wallet(wallet_file: &WalletFile) -> Result<()> {
    let dir = xergon_dir()?;
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("Failed to create directory {}", dir.display()))?;

    let path = wallet_path()?;
    let contents = serde_json::to_string_pretty(wallet_file)
        .context("Failed to serialize wallet")?;

    // Write with restricted permissions (0600)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::write(&path, &contents)?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .context("Failed to set wallet file permissions")?;
    }
    #[cfg(not(unix))]
    {
        std::fs::write(&path, &contents)?;
    }

    Ok(())
}

/// Derive a 32-byte encryption key from password + salt using HKDF-SHA256.
fn derive_encryption_key(password: &str, salt: &[u8]) -> Result<[u8; 32]> {
    let hk = Hkdf::<Sha256>::new(Some(salt), password.as_bytes());
    let mut key = [0u8; 32];
    hk.expand(b"xergon-wallet-v1", &mut key)
        .map_err(|e| anyhow::anyhow!("HKDF expansion failed: {}", e))?;
    Ok(key)
}

/// Blake2b256 hash (32 bytes).
pub fn blake2b256(data: &[u8]) -> [u8; 32] {
    use blake2::{Blake2b, Digest as _};
    use digest::generic_array::typenum::U32;
    type Blake2b256 = Blake2b<U32>;
    let mut hasher = Blake2b256::new();
    hasher.update(data);
    let result = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&result);
    out
}

/// Get password from XERGON_WALLET_PASSWORD env var or stdin prompt.
fn get_password() -> Result<String> {
    if let Ok(pwd) = std::env::var("XERGON_WALLET_PASSWORD") {
        if pwd.is_empty() {
            anyhow::bail!("XERGON_WALLET_PASSWORD is set but empty");
        }
        return Ok(pwd);
    }

    rpassword::prompt_password("  Enter wallet password: ")
        .context("Failed to read password")
}

/// Get new password (with confirmation done externally).
fn get_new_password() -> Result<String> {
    if let Ok(pwd) = std::env::var("XERGON_WALLET_PASSWORD") {
        if pwd.is_empty() {
            anyhow::bail!("XERGON_WALLET_PASSWORD is set but empty");
        }
        return Ok(pwd);
    }

    rpassword::prompt_password("  Enter new wallet password (min 8 chars): ")
        .context("Failed to read password")
        .and_then(|pwd| {
            if pwd.len() < 8 {
                anyhow::bail!("Password must be at least 8 characters");
            }
            Ok(pwd)
        })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_and_encrypt_wallet_roundtrip() {
        let mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let secret_key = derive_secret_key(mnemonic).unwrap();
        let public_key = derive_public_key(&secret_key);

        let wallet = Wallet {
            mnemonic: mnemonic.to_string(),
            secret_key: hex::encode(secret_key),
            public_key: hex::encode(public_key),
        };

        let password = "test-password-123";
        let encrypted = encrypt_wallet(&wallet, password).unwrap();
        let decrypted = decrypt_wallet(&encrypted, password).unwrap();

        assert_eq!(wallet.mnemonic, decrypted.mnemonic);
        assert_eq!(wallet.public_key, decrypted.public_key);
        assert_eq!(wallet.secret_key, decrypted.secret_key);
    }

    #[test]
    fn test_wrong_password_fails() {
        let wallet = Wallet {
            mnemonic: "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about".to_string(),
            secret_key: "deadbeef".to_string(),
            public_key: "cafebabe".to_string(),
        };

        let encrypted = encrypt_wallet(&wallet, "correct-password").unwrap();
        let result = decrypt_wallet(&encrypted, "wrong-password");
        assert!(result.is_err());
    }

    #[test]
    fn test_blake2b256_deterministic() {
        let hash1 = blake2b256(b"hello");
        let hash2 = blake2b256(b"hello");
        assert_eq!(hash1, hash2);
        assert_eq!(hash1.len(), 32);
    }

    #[test]
    fn test_blake2b256_different_inputs() {
        let hash1 = blake2b256(b"hello");
        let hash2 = blake2b256(b"world");
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_generate_mnemonic() {
        let mnemonic = generate_mnemonic().unwrap();
        let words: Vec<&str> = mnemonic.split_whitespace().collect();
        assert_eq!(words.len(), 15, "Expected 15-word mnemonic");
    }
}
