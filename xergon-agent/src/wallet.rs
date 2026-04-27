//! Xergon wallet management
//!
//! Generates a random 15-word BIP-39 mnemonic, derives a secp256k1 key pair,
//! and stores an encrypted wallet at `~/.xergon/wallet.json`.
//!
//! Key derivation (k256 secp256k1):
//!   mnemonic -> BIP39 seed -> blake2b256 -> k256::SecretKey::from_slice()
//!   public_key = secret_key.public_key().to_encoded_point(true)  // 33-byte compressed
//!
//! The compressed public key (33 bytes, hex = 66 chars) matches ErgoScript
//! `GroupElement` format used in contract R4/R5 registers.

use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use anyhow::{Context, Result};
use blake2::{Blake2b, Digest as _};
use digest::generic_array::typenum::U32;
use hkdf::Hkdf;
use k256::elliptic_curve::sec1::ToEncodedPoint;
use k256::SecretKey;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::Sha256;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Wallet file stored at ~/.xergon/wallet.json
#[derive(Debug, Serialize, Deserialize)]
pub struct WalletFile {
    /// AES-256-GCM encrypted mnemonic (hex-encoded ciphertext)
    pub encrypted_mnemonic: String,
    /// Salt used for HKDF key derivation (hex, 32 bytes)
    pub salt: String,
    /// Nonce used for AES-GCM (hex, 12 bytes)
    pub nonce: String,
    /// Compressed secp256k1 public key (hex, 33 bytes = 66 chars).
    /// Format: 0x02 or 0x03 (y-coordinate parity) + 32-byte x-coordinate.
    pub public_key: String,
    /// Ergo P2PK address (base58 with "4" prefix for mainnet)
    pub address: String,
}

/// In-memory wallet (decrypted)
#[derive(Debug, Clone)]
pub struct Wallet {
    pub mnemonic: String,
    /// 32-byte secret key as hex (for signing via node wallet API)
    pub secret_key: String,
    /// 33-byte compressed secp256k1 public key as hex
    pub public_key: String,
}

// ---------------------------------------------------------------------------
// Paths
// ---------------------------------------------------------------------------

/// Returns the xergon config directory path (`~/.xergon`).
pub fn xergon_dir() -> Result<std::path::PathBuf> {
    let home = dirs::home_dir().context("Cannot determine home directory")?;
    Ok(home.join(".xergon"))
}

/// Returns the wallet file path (`~/.xergon/wallet.json`).
pub fn wallet_path() -> Result<std::path::PathBuf> {
    Ok(xergon_dir()?.join("wallet.json"))
}

/// Check if a wallet already exists on disk.
pub fn wallet_exists() -> bool {
    wallet_path().map(|p| p.exists()).unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Wallet generation
// ---------------------------------------------------------------------------

/// Generate a new random 15-word BIP-39 mnemonic.
fn generate_mnemonic() -> Result<String> {
    let mut entropy = [0u8; 20];
    OsRng.fill_bytes(&mut entropy);
    let mnemonic = bip39::Mnemonic::from_entropy(&entropy)
        .map_err(|e| anyhow::anyhow!("Failed to generate mnemonic: {}", e))?;
    Ok(mnemonic.to_string())
}

/// Derive 32-byte secret key from mnemonic: BIP39 seed -> blake2b256 -> mod n.
///
/// Takes the BIP39 normalized mnemonic, derives the 64-byte seed, hashes it
/// with blake2b256 to get 32 random bytes, then passes to k256 which reduces
/// modulo the secp256k1 curve order (producing a valid scalar in [1, n-1]).
fn derive_secret_key(mnemonic: &str) -> Result<[u8; 32]> {
    let mnemonic = bip39::Mnemonic::parse_normalized(mnemonic)
        .map_err(|e| anyhow::anyhow!("Invalid mnemonic: {}", e))?;
    let seed = mnemonic.to_seed_normalized("");
    Ok(blake2b256(&seed))
}

/// Derive a compressed secp256k1 public key (33 bytes) from a 32-byte secret key.
/// Returns hex-encoded string (66 chars). Prefix is 0x02 or 0x03.
fn derive_public_key(secret_key: &[u8; 32]) -> Result<String> {
    let sk = SecretKey::from_slice(secret_key).map_err(|e| {
        anyhow::anyhow!(
            "Secret key is not a valid secp256k1 scalar: {}. \
             This should not happen with blake2b256-derived keys.",
            e
        )
    })?;
    let pk = sk.public_key();
    let encoded = pk.to_encoded_point(true);
    Ok(hex::encode(encoded.as_bytes()))
}

/// Derive an Ergo mainnet P2PK address from a compressed secp256k1 public key.
/// Payload: [0x01 mainnet prefix | 33-byte PK | 4-byte blake2b256 checksum]
/// Result: base58-encoded with "4" prefix, e.g. "4AhCX...".
fn derive_ergo_address(compressed_pk_hex: &str) -> Result<String> {
    let pk_bytes = hex::decode(compressed_pk_hex).context("Invalid public key hex")?;
    anyhow::ensure!(pk_bytes.len() == 33, "Public key must be 33 bytes");

    let mut payload = Vec::with_capacity(38);
    payload.push(0x01); // mainnet P2PK prefix
    payload.extend_from_slice(&pk_bytes);

    let checksum = blake2b256(&payload);
    payload.extend_from_slice(&checksum[..4]);

    Ok(format!("4{}", bs58::encode(&payload).into_string()))
}

/// Generate a new wallet and save it to disk encrypted.
pub fn generate_wallet(password: &str) -> Result<Wallet> {
    if wallet_exists() {
        anyhow::bail!(
            "Wallet already exists at {}. Remove it first.",
            wallet_path()?.display()
        );
    }

    let mnemonic = generate_mnemonic()?;
    let secret_key = derive_secret_key(&mnemonic)?;
    let public_key = derive_public_key(&secret_key)?;
    let _address = derive_ergo_address(&public_key)?;

    let wallet = Wallet {
        mnemonic: mnemonic.clone(),
        secret_key: hex::encode(secret_key),
        public_key: public_key.clone(),
    };

    let wf = encrypt_wallet(&wallet, password)?;
    save_wallet(&wf)?;
    Ok(wallet)
}

/// Interactive wallet generation (password from env or stdin).
pub fn generate_wallet_interactive() -> Result<Wallet> {
    println!("  Creating a new Xergon wallet...");
    println!();

    let password = get_new_password()?;

    let confirm = rpassword::prompt_password("  Confirm password: ")
        .context("Failed to read password confirmation")?;
    anyhow::ensure!(password == confirm, "Passwords do not match.");

    let mnemonic = generate_mnemonic()?;
    let secret_key = derive_secret_key(&mnemonic)?;
    let public_key = derive_public_key(&secret_key)?;
    let address = derive_ergo_address(&public_key)?;

    let wallet = Wallet {
        mnemonic: mnemonic.clone(),
        secret_key: hex::encode(secret_key),
        public_key: public_key.clone(),
    };

    let wf = encrypt_wallet(&wallet, &password)?;
    save_wallet(&wf)?;

    println!();
    println!("  Wallet created successfully!");
    println!("  Store your mnemonic phrase in a safe place:");
    println!("  ");
    for word in wallet.mnemonic.split_whitespace() {
        print!("  {}", word);
    }
    println!();
    println!("  ");
    println!("  WARNING: If you lose your mnemonic, you cannot recover your wallet.");
    println!("  Public key (compressed secp256k1, 33 bytes): {}", public_key);
    println!("  Address: {}", address);

    Ok(wallet)
}

// ---------------------------------------------------------------------------
// Wallet loading
// ---------------------------------------------------------------------------

/// Load a wallet from disk, decrypting with the given password.
pub fn load_wallet(password: &str) -> Result<Wallet> {
    let path = wallet_path()?;
    let contents = std::fs::read_to_string(&path)
        .with_context(|| format!("Wallet not found at {}. Run `xergon setup`.", path.display()))?;
    let wf: WalletFile =
        serde_json::from_str(&contents).context("Failed to parse wallet.json")?;

    if wf.public_key.len() == 64 {
        anyhow::bail!(
            "Your wallet uses an outdated key format (blake2b256 public key hash). \
             Delete ~/.xergon/wallet.json and run `xergon setup` to regenerate."
        );
    }

    decrypt_wallet(&wf, password)
}

/// Load wallet using XERGON_WALLET_PASSWORD env var or stdin prompt.
pub fn load_wallet_interactive() -> Result<Wallet> {
    load_wallet(&get_password()?)
}

// ---------------------------------------------------------------------------
// Encryption / decryption
// ---------------------------------------------------------------------------

/// Encrypt wallet data with AES-256-GCM using HKDF-SHA256 key derived from password.
fn encrypt_wallet(wallet: &Wallet, password: &str) -> Result<WalletFile> {
    let mut salt = [0u8; 32];
    OsRng.fill_bytes(&mut salt);

    let key = derive_encryption_key(password, &salt)?;

    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|e| anyhow::anyhow!("Failed to create cipher: {}", e))?;

    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, wallet.mnemonic.as_bytes())
        .map_err(|e| anyhow::anyhow!("Encryption failed: {}", e))?;

    let address = derive_ergo_address(&wallet.public_key)?;

    Ok(WalletFile {
        encrypted_mnemonic: hex::encode(&ciphertext),
        salt: hex::encode(salt),
        nonce: hex::encode(nonce_bytes),
        public_key: wallet.public_key.clone(),
        address,
    })
}

/// Decrypt a wallet file using the given password.
fn decrypt_wallet(wf: &WalletFile, password: &str) -> Result<Wallet> {
    let salt = hex::decode(&wf.salt).context("Invalid salt hex")?;
    let nonce_bytes = hex::decode(&wf.nonce).context("Invalid nonce hex")?;
    let ciphertext = hex::decode(&wf.encrypted_mnemonic).context("Invalid ciphertext hex")?;

    let key = derive_encryption_key(password, &salt)?;

    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|e| anyhow::anyhow!("Failed to create cipher: {}", e))?;

    let plaintext = cipher
        .decrypt(Nonce::from_slice(&nonce_bytes), ciphertext.as_ref())
        .map_err(|_| anyhow::anyhow!("Decryption failed. Wrong password?"))?;

    let mnemonic = String::from_utf8(plaintext).context("Decrypted data is not valid UTF-8")?;
    let secret_key = derive_secret_key(&mnemonic)?;
    let public_key = derive_public_key(&secret_key)?;

    Ok(Wallet {
        mnemonic,
        secret_key: hex::encode(secret_key),
        public_key,
    })
}

/// Save wallet file to `~/.xergon/wallet.json` with mode 0o600.
fn save_wallet(wf: &WalletFile) -> Result<()> {
    let dir = xergon_dir()?;
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("Failed to create {}", dir.display()))?;
    let path = wallet_path()?;
    let contents = serde_json::to_string_pretty(wf)
        .context("Failed to serialize wallet")?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::write(&path, &contents)?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }
    #[cfg(not(unix))]
    {
        std::fs::write(&path, &contents)?;
    }
    Ok(())
}

/// Derive a 32-byte AES-256 key from password + salt using HKDF-SHA256.
fn derive_encryption_key(password: &str, salt: &[u8]) -> Result<[u8; 32]> {
    let hk = Hkdf::<Sha256>::new(Some(salt), password.as_bytes());
    let mut key = [0u8; 32];
    hk.expand(b"xergon-wallet-v1", &mut key)
        .map_err(|e| anyhow::anyhow!("HKDF expansion failed: {}", e))?;
    Ok(key)
}

/// Get password from XERGON_WALLET_PASSWORD env or stdin prompt.
fn get_password() -> Result<String> {
    if let Ok(pwd) = std::env::var("XERGON_WALLET_PASSWORD") {
        anyhow::ensure!(!pwd.is_empty(), "XERGON_WALLET_PASSWORD is set but empty");
        return Ok(pwd);
    }
    rpassword::prompt_password("  Enter wallet password: ")
        .context("Failed to read password")
}

/// Get new password (min 8 chars) from XERGON_WALLET_PASSWORD env or stdin.
fn get_new_password() -> Result<String> {
    if let Ok(pwd) = std::env::var("XERGON_WALLET_PASSWORD") {
        anyhow::ensure!(!pwd.is_empty(), "XERGON_WALLET_PASSWORD is empty");
        anyhow::ensure!(pwd.len() >= 8, "Password must be at least 8 characters");
        return Ok(pwd);
    }
    let pwd = rpassword::prompt_password("  Enter new wallet password (min 8 chars): ")
        .context("Failed to read password")?;
    anyhow::ensure!(pwd.len() >= 8, "Password must be at least 8 characters");
    Ok(pwd)
}

/// Blake2b256 hash (32 bytes).
pub fn blake2b256(data: &[u8]) -> [u8; 32] {
    let mut hasher = Blake2b::<U32>::new();
    hasher.update(data);
    let result = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&result);
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_public_key_valid_secp256k1() {
        let secret = [0x01u8; 32];
        let pk = derive_public_key(&secret).unwrap();
        assert_eq!(pk.len(), 66, "Compressed PK must be 66 hex chars");
        let prefix = u8::from_str_radix(&pk[..2], 16).unwrap();
        assert!(
            prefix == 0x02 || prefix == 0x03,
            "PK must start with 0x02 or 0x03, got 0x{:02x}",
            prefix
        );
    }

    #[test]
    fn test_public_key_is_deterministic() {
        let s1 = blake2b256(b"seed");
        let s2 = blake2b256(b"seed");
        let pk1 = derive_public_key(&s1).unwrap();
        let pk2 = derive_public_key(&s2).unwrap();
        assert_eq!(pk1, pk2);
    }

    #[test]
    fn test_different_secrets_different_keys() {
        let s1 = blake2b256(b"seed A");
        let s2 = blake2b256(b"seed B");
        let pk1 = derive_public_key(&s1).unwrap();
        let pk2 = derive_public_key(&s2).unwrap();
        assert_ne!(pk1, pk2);
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let sk = derive_secret_key(mnemonic).unwrap();
        let pk = derive_public_key(&sk).unwrap();

        let wallet = Wallet {
            mnemonic: mnemonic.to_string(),
            secret_key: hex::encode(sk),
            public_key: pk.clone(),
        };

        let encrypted = encrypt_wallet(&wallet, "test-password-xyz").unwrap();
        let decrypted = decrypt_wallet(&encrypted, "test-password-xyz").unwrap();

        assert_eq!(wallet.mnemonic, decrypted.mnemonic);
        assert_eq!(wallet.public_key, decrypted.public_key);
        assert_eq!(wallet.secret_key, decrypted.secret_key);
    }

    #[test]
    fn test_wrong_password_fails() {
        let wallet = Wallet {
            mnemonic: "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about"
                .to_string(),
            secret_key: "deadbeef".to_string(),
            public_key: "02".to_string() + &"aa".repeat(32),
        };
        let encrypted = encrypt_wallet(&wallet, "correct-password").unwrap();
        let result = decrypt_wallet(&encrypted, "wrong-password");
        assert!(result.is_err());
    }

    #[test]
    fn test_generate_mnemonic_15_words() {
        let mn = generate_mnemonic().unwrap();
        let words: Vec<_> = mn.split_whitespace().collect();
        assert_eq!(words.len(), 15);
    }

    #[test]
    fn test_blake2b256_deterministic() {
        let h1 = blake2b256(b"hello");
        let h2 = blake2b256(b"hello");
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 32);
    }

    #[test]
    fn test_blake2b256_different_inputs() {
        let h1 = blake2b256(b"hello");
        let h2 = blake2b256(b"world");
        assert_ne!(h1, h2);
    }
}
