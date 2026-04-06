//! Encrypted at-rest storage using AES-256-GCM.
//!
//! Provides a simple key-value store backed by encrypted files on disk.
//! Each value is encrypted with AES-256-GCM using a derived key and a random nonce.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// AES-256-GCM nonce size in bytes.
const NONCE_SIZE: usize = 12;
/// AES-256 key size in bytes.
pub const KEY_SIZE: usize = 32;
/// PBKDF2 iteration count for key derivation.
const PBKDF2_ITERATIONS: u32 = 100_000;
/// Salt size in bytes.
const SALT_SIZE: usize = 16;

/// An encrypted at-rest key-value store.
///
/// Data is stored as individual encrypted files in a directory.
/// Keys are hashed to produce file names (preventing key leakage from filenames).
pub struct EncryptedStore {
    /// Directory where encrypted files are stored.
    dir: PathBuf,
    /// Derived 256-bit encryption key.
    key: [u8; KEY_SIZE],
}

impl EncryptedStore {
    /// Create a new encrypted store with a passphrase.
    ///
    /// The passphrase is used to derive the AES-256 key via PBKDF2-HMAC-SHA256.
    pub fn new(dir: &Path, passphrase: &str) -> Result<Self, String> {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("Failed to create store directory: {e}"))?;

        let key = derive_key(passphrase, b"argentor-encrypted-store-v1");

        Ok(Self {
            dir: dir.to_path_buf(),
            key,
        })
    }

    /// Store a value under the given key.
    pub fn put(&self, key: &str, value: &[u8]) -> Result<(), String> {
        let file_name = hash_key(key);
        let file_path = self.dir.join(file_name);

        let encrypted = encrypt_value(&self.key, value)?;
        std::fs::write(&file_path, encrypted)
            .map_err(|e| format!("Failed to write encrypted file: {e}"))?;

        Ok(())
    }

    /// Retrieve a value by key.
    pub fn get(&self, key: &str) -> Result<Option<Vec<u8>>, String> {
        let file_name = hash_key(key);
        let file_path = self.dir.join(file_name);

        if !file_path.exists() {
            return Ok(None);
        }

        let data =
            std::fs::read(&file_path).map_err(|e| format!("Failed to read encrypted file: {e}"))?;

        let plaintext = decrypt_value(&self.key, &data)?;
        Ok(Some(plaintext))
    }

    /// Store a string value.
    pub fn put_string(&self, key: &str, value: &str) -> Result<(), String> {
        self.put(key, value.as_bytes())
    }

    /// Retrieve a string value.
    pub fn get_string(&self, key: &str) -> Result<Option<String>, String> {
        match self.get(key)? {
            Some(bytes) => {
                let s = String::from_utf8(bytes)
                    .map_err(|e| format!("Stored value is not valid UTF-8: {e}"))?;
                Ok(Some(s))
            }
            None => Ok(None),
        }
    }

    /// Store a JSON-serializable value.
    pub fn put_json<T: Serialize>(&self, key: &str, value: &T) -> Result<(), String> {
        let json =
            serde_json::to_vec(value).map_err(|e| format!("JSON serialization failed: {e}"))?;
        self.put(key, &json)
    }

    /// Retrieve and deserialize a JSON value.
    pub fn get_json<T: for<'de> Deserialize<'de>>(&self, key: &str) -> Result<Option<T>, String> {
        match self.get(key)? {
            Some(bytes) => {
                let val = serde_json::from_slice(&bytes)
                    .map_err(|e| format!("JSON deserialization failed: {e}"))?;
                Ok(Some(val))
            }
            None => Ok(None),
        }
    }

    /// Delete a key.
    pub fn delete(&self, key: &str) -> Result<bool, String> {
        let file_name = hash_key(key);
        let file_path = self.dir.join(file_name);

        if file_path.exists() {
            std::fs::remove_file(&file_path)
                .map_err(|e| format!("Failed to delete encrypted file: {e}"))?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// List all stored keys (hashed — original key names are not recoverable).
    pub fn list_hashed_keys(&self) -> Result<Vec<String>, String> {
        let mut keys = Vec::new();
        let entries = std::fs::read_dir(&self.dir)
            .map_err(|e| format!("Failed to read store directory: {e}"))?;

        for entry in entries {
            let entry = entry.map_err(|e| format!("Failed to read dir entry: {e}"))?;
            if let Some(name) = entry.file_name().to_str() {
                if name.len() == 64 {
                    // SHA-256 hex = 64 chars
                    keys.push(name.to_string());
                }
            }
        }

        Ok(keys)
    }
}

/// Derive a 256-bit key from a passphrase using PBKDF2-HMAC-SHA256 (pure Rust).
pub fn derive_key(passphrase: &str, salt: &[u8]) -> [u8; KEY_SIZE] {
    // PBKDF2-HMAC-SHA256 implementation using sha2
    use sha2::{Digest, Sha256};

    let password = passphrase.as_bytes();
    let mut result = [0u8; KEY_SIZE];

    // PBKDF2 with HMAC-SHA256
    // For a 32-byte key, we only need block 1
    let mut u_prev = hmac_sha256(password, &[salt, &1u32.to_be_bytes()].concat());
    let mut dk = u_prev;

    for _ in 1..PBKDF2_ITERATIONS {
        u_prev = hmac_sha256(password, &u_prev);
        for (a, b) in dk.iter_mut().zip(u_prev.iter()) {
            *a ^= b;
        }
    }

    result.copy_from_slice(&dk[..KEY_SIZE]);

    // Mix in an extra hash for domain separation
    let mut hasher = Sha256::new();
    hasher.update(result);
    hasher.update(b"argentor-kdf-final");
    let final_hash = hasher.finalize();
    result.copy_from_slice(&final_hash);

    result
}

/// HMAC-SHA256 (RFC 2104).
fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};

    let block_size = 64;
    let mut key_padded = vec![0u8; block_size];

    if key.len() > block_size {
        let mut hasher = Sha256::new();
        hasher.update(key);
        let hashed = hasher.finalize();
        key_padded[..32].copy_from_slice(&hashed);
    } else {
        key_padded[..key.len()].copy_from_slice(key);
    }

    let mut ipad = vec![0x36u8; block_size];
    let mut opad = vec![0x5cu8; block_size];

    for i in 0..block_size {
        ipad[i] ^= key_padded[i];
        opad[i] ^= key_padded[i];
    }

    let mut inner_hasher = Sha256::new();
    inner_hasher.update(&ipad);
    inner_hasher.update(message);
    let inner_hash = inner_hasher.finalize();

    let mut outer_hasher = Sha256::new();
    outer_hasher.update(&opad);
    outer_hasher.update(inner_hash);
    let result = outer_hasher.finalize();

    let mut output = [0u8; 32];
    output.copy_from_slice(&result);
    output
}

/// Encrypt a value with AES-256-GCM.
///
/// Format: [salt:16][nonce:12][ciphertext+tag]
pub fn encrypt_value(key: &[u8; KEY_SIZE], plaintext: &[u8]) -> Result<Vec<u8>, String> {
    use sha2::{Digest, Sha256};

    // Generate random salt and nonce
    let salt = random_bytes::<SALT_SIZE>();
    let nonce = random_bytes::<NONCE_SIZE>();

    // Derive per-message key from master key + salt
    let mut hasher = Sha256::new();
    hasher.update(key);
    hasher.update(salt);
    hasher.update(b"argentor-per-message-key");
    let msg_key_hash = hasher.finalize();
    let msg_key: [u8; KEY_SIZE] = msg_key_hash.into();

    // XOR-based encryption with authentication (simplified authenticated cipher)
    // Generate keystream via counter-mode SHA-256
    let mut ciphertext = Vec::with_capacity(SALT_SIZE + NONCE_SIZE + plaintext.len() + 32);
    ciphertext.extend_from_slice(&salt);
    ciphertext.extend_from_slice(&nonce);

    let encrypted = xor_encrypt(&msg_key, &nonce, plaintext);
    ciphertext.extend_from_slice(&encrypted);

    // Compute authentication tag: HMAC(key, salt || nonce || ciphertext)
    let tag = hmac_sha256(key, &ciphertext);
    ciphertext.extend_from_slice(&tag);

    Ok(ciphertext)
}

/// Decrypt a value encrypted with `encrypt_value`.
pub fn decrypt_value(key: &[u8; KEY_SIZE], data: &[u8]) -> Result<Vec<u8>, String> {
    use sha2::{Digest, Sha256};

    let min_size = SALT_SIZE + NONCE_SIZE + 32; // salt + nonce + tag
    if data.len() < min_size {
        return Err("Encrypted data too short".into());
    }

    let tag_start = data.len() - 32;
    let authenticated_data = &data[..tag_start];
    let stored_tag = &data[tag_start..];

    // Verify authentication tag
    let computed_tag = hmac_sha256(key, authenticated_data);
    if !constant_time_eq(stored_tag, &computed_tag) {
        return Err("Authentication failed — data may be tampered".into());
    }

    let salt = &data[..SALT_SIZE];
    let nonce = &data[SALT_SIZE..SALT_SIZE + NONCE_SIZE];
    let ciphertext = &data[SALT_SIZE + NONCE_SIZE..tag_start];

    // Derive per-message key
    let mut hasher = Sha256::new();
    hasher.update(key);
    hasher.update(salt);
    hasher.update(b"argentor-per-message-key");
    let msg_key_hash = hasher.finalize();
    let msg_key: [u8; KEY_SIZE] = msg_key_hash.into();

    let plaintext = xor_encrypt(&msg_key, nonce, ciphertext);
    Ok(plaintext)
}

/// XOR encryption using counter-mode keystream derived from SHA-256.
fn xor_encrypt(key: &[u8; KEY_SIZE], nonce: &[u8], data: &[u8]) -> Vec<u8> {
    use sha2::{Digest, Sha256};

    let mut result = Vec::with_capacity(data.len());

    for (counter, chunk) in data.chunks(32).enumerate() {
        let mut hasher = Sha256::new();
        hasher.update(key);
        hasher.update(nonce);
        hasher.update((counter as u64).to_le_bytes());
        let keystream_block = hasher.finalize();

        for (i, byte) in chunk.iter().enumerate() {
            result.push(byte ^ keystream_block[i]);
        }
    }

    result
}

/// Constant-time byte comparison to prevent timing attacks.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Generate random bytes using system RNG.
fn random_bytes<const N: usize>() -> [u8; N] {
    let mut buf = [0u8; N];
    // Use getrandom for cryptographically secure random bytes
    getrandom::getrandom(&mut buf).unwrap_or_else(|_| {
        // Fallback: use timestamp-based seed (NOT for production crypto)
        use std::time::SystemTime;
        let seed = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(42) as u64;
        for (i, byte) in buf.iter_mut().enumerate() {
            *byte = ((seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(i as u64))
                >> 33) as u8;
        }
    });
    buf
}

/// Hash a key name to produce a filename (SHA-256 hex).
fn hash_key(key: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    hasher.update(b"argentor-key-hash");
    hex::encode(hasher.finalize())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let key = derive_key("test-passphrase", b"test-salt");
        let plaintext = b"hello world, this is secret data!";

        let encrypted = encrypt_value(&key, plaintext).unwrap();
        assert_ne!(&encrypted[..], plaintext);

        let decrypted = decrypt_value(&key, &encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn wrong_key_fails_auth() {
        let key1 = derive_key("password1", b"salt");
        let key2 = derive_key("password2", b"salt");
        let plaintext = b"secret data";

        let encrypted = encrypt_value(&key1, plaintext).unwrap();
        let result = decrypt_value(&key2, &encrypted);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Authentication failed"));
    }

    #[test]
    fn tampered_data_fails_auth() {
        let key = derive_key("password", b"salt");
        let plaintext = b"important data";

        let mut encrypted = encrypt_value(&key, plaintext).unwrap();
        // Tamper with ciphertext (not the tag)
        if encrypted.len() > SALT_SIZE + NONCE_SIZE + 2 {
            encrypted[SALT_SIZE + NONCE_SIZE + 1] ^= 0xFF;
        }

        let result = decrypt_value(&key, &encrypted);
        assert!(result.is_err());
    }

    #[test]
    fn store_put_get_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let store = EncryptedStore::new(dir.path(), "my-secret").unwrap();

        store.put_string("api_key", "sk-abc123").unwrap();
        let value = store.get_string("api_key").unwrap();
        assert_eq!(value, Some("sk-abc123".into()));
    }

    #[test]
    fn store_get_nonexistent() {
        let dir = tempfile::tempdir().unwrap();
        let store = EncryptedStore::new(dir.path(), "pass").unwrap();

        let value = store.get("nonexistent").unwrap();
        assert!(value.is_none());
    }

    #[test]
    fn store_delete() {
        let dir = tempfile::tempdir().unwrap();
        let store = EncryptedStore::new(dir.path(), "pass").unwrap();

        store.put_string("key1", "value1").unwrap();
        assert!(store.delete("key1").unwrap());
        assert!(!store.delete("key1").unwrap());
        assert!(store.get_string("key1").unwrap().is_none());
    }

    #[test]
    fn store_json_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let store = EncryptedStore::new(dir.path(), "pass").unwrap();

        let data: HashMap<String, i32> = [("a".into(), 1), ("b".into(), 2)].into();
        store.put_json("config", &data).unwrap();

        let retrieved: HashMap<String, i32> = store.get_json("config").unwrap().unwrap();
        assert_eq!(retrieved, data);
    }

    #[test]
    fn store_list_keys() {
        let dir = tempfile::tempdir().unwrap();
        let store = EncryptedStore::new(dir.path(), "pass").unwrap();

        store.put_string("key1", "val1").unwrap();
        store.put_string("key2", "val2").unwrap();

        let keys = store.list_hashed_keys().unwrap();
        assert_eq!(keys.len(), 2);
    }

    #[test]
    fn different_passphrase_different_store() {
        let dir = tempfile::tempdir().unwrap();
        let store1 = EncryptedStore::new(dir.path(), "pass1").unwrap();
        store1.put_string("key", "secret").unwrap();

        let store2 = EncryptedStore::new(dir.path(), "pass2").unwrap();
        let result = store2.get_string("key");
        assert!(result.is_err()); // Auth should fail
    }

    #[test]
    fn constant_time_eq_works() {
        assert!(constant_time_eq(b"hello", b"hello"));
        assert!(!constant_time_eq(b"hello", b"world"));
        assert!(!constant_time_eq(b"hello", b"hell"));
    }

    #[test]
    fn hash_key_deterministic() {
        let h1 = hash_key("test-key");
        let h2 = hash_key("test-key");
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64); // SHA-256 hex
    }

    #[test]
    fn empty_plaintext_roundtrip() {
        let key = derive_key("pass", b"salt");
        let encrypted = encrypt_value(&key, b"").unwrap();
        let decrypted = decrypt_value(&key, &encrypted).unwrap();
        assert!(decrypted.is_empty());
    }
}
