//! Package signing infrastructure for Roast packages.
//!
//! Provides Ed25519-based digital signatures for package authentication,
//! ensuring packages haven't been tampered with and come from trusted publishers.

use ed25519_dalek::{
    Signature, Signer, Verifier, VerifyingKey,
    SECRET_KEY_LENGTH, PUBLIC_KEY_LENGTH,
};
use rand::rngs::OsRng;
use sha2::{Sha256, Digest};
use std::fs;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc, serde::ts_seconds};
use crate::{Error, Result};

// =============================================================================
// Package Signature
// =============================================================================

/// A cryptographic signature for a package.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageSignature {
    /// Ed25519 signature of package tarball (base64 encoded).
    pub signature: String,
    
    /// Public key fingerprint (SHA256 of public key, hex encoded).
    pub key_fingerprint: String,
    
    /// When the package was signed.
    #[serde(with = "chrono::serde::ts_seconds")]
    pub signed_at: DateTime<Utc>,
    
    /// Publisher email or identifier.
    pub publisher: Option<String>,
    
    /// Signature algorithm (always "ed25519" for now).
    pub algorithm: String,
}

impl PackageSignature {
    /// Create a new signature from raw bytes.
    pub fn new(
        signature: &[u8],
        key_fingerprint: String,
        publisher: Option<String>,
    ) -> Self {
        Self {
            signature: base64_encode(signature),
            key_fingerprint,
            signed_at: Utc::now(),
            publisher,
            algorithm: "ed25519".to_string(),
        }
    }

    /// Get the raw signature bytes.
    pub fn signature_bytes(&self) -> Result<Vec<u8>> {
        base64_decode(&self.signature)
            .map_err(|e| Error::Registry(format!("Invalid signature encoding: {}", e)))
    }
}

// =============================================================================
// Signing Key Management
// =============================================================================

/// A signing key pair for package publishing.
pub struct SigningKey {
    /// The Ed25519 signing key.
    inner: ed25519_dalek::SigningKey,
    
    /// Key fingerprint (SHA256 of public key).
    fingerprint: String,
    
    /// Optional key description/email.
    description: Option<String>,
}

impl SigningKey {
    /// Generate a new random signing key.
    pub fn generate() -> Self {
        let inner = ed25519_dalek::SigningKey::generate(&mut OsRng);
        let fingerprint = compute_fingerprint(inner.verifying_key().as_bytes());
        
        Self {
            inner,
            fingerprint,
            description: None,
        }
    }

    /// Generate a new key with a description.
    pub fn generate_with_description(description: &str) -> Self {
        let mut key = Self::generate();
        key.description = Some(description.to_string());
        key
    }

    /// Load a signing key from bytes.
    pub fn from_bytes(bytes: &[u8; SECRET_KEY_LENGTH]) -> Result<Self> {
        let inner = ed25519_dalek::SigningKey::from_bytes(bytes);
        let fingerprint = compute_fingerprint(inner.verifying_key().as_bytes());
        
        Ok(Self {
            inner,
            fingerprint,
            description: None,
        })
    }

    /// Load a signing key from a file.
    pub fn load(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)?;
        let key_data: StoredKey = serde_json::from_str(&content)
            .map_err(|e| Error::Registry(format!("Invalid key file: {}", e)))?;
        
        let secret_bytes = base64_decode(&key_data.secret_key)
            .map_err(|e| Error::Registry(format!("Invalid key encoding: {}", e)))?;
        
        if secret_bytes.len() != SECRET_KEY_LENGTH {
            return Err(Error::Registry("Invalid key length".to_string()));
        }
        
        let mut key_bytes = [0u8; SECRET_KEY_LENGTH];
        key_bytes.copy_from_slice(&secret_bytes);
        
        let mut key = Self::from_bytes(&key_bytes)?;
        key.description = key_data.description;
        
        Ok(key)
    }

    /// Save the signing key to a file (with restricted permissions).
    pub fn save(&self, path: &Path) -> Result<()> {
        let key_data = StoredKey {
            secret_key: base64_encode(self.inner.as_bytes()),
            public_key: base64_encode(self.inner.verifying_key().as_bytes()),
            fingerprint: self.fingerprint.clone(),
            description: self.description.clone(),
            created_at: Utc::now(),
        };
        
        let json = serde_json::to_string_pretty(&key_data)
            .map_err(|e| Error::Registry(format!("Failed to serialize key: {}", e)))?;
        
        // Create parent directory if needed
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        
        fs::write(path, &json)?;
        
        // Set restrictive permissions on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(path)?.permissions();
            perms.set_mode(0o600); // Read/write for owner only
            fs::set_permissions(path, perms)?;
        }
        
        Ok(())
    }

    /// Get the key fingerprint.
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }

    /// Get the public key bytes.
    pub fn public_key_bytes(&self) -> [u8; PUBLIC_KEY_LENGTH] {
        self.inner.verifying_key().to_bytes()
    }

    /// Get the public key as base64.
    pub fn public_key_base64(&self) -> String {
        base64_encode(&self.public_key_bytes())
    }

    /// Sign a package tarball.
    pub fn sign(&self, tarball: &[u8]) -> PackageSignature {
        let signature = self.inner.sign(tarball);
        
        PackageSignature::new(
            signature.to_bytes().as_slice(),
            self.fingerprint.clone(),
            self.description.clone(),
        )
    }

    /// Sign a file.
    pub fn sign_file(&self, path: &Path) -> Result<PackageSignature> {
        let content = fs::read(path)?;
        Ok(self.sign(&content))
    }

    /// Get the default key path.
    pub fn default_key_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from(".config"))
            .join("kitchen")
            .join("signing_key.json")
    }

    /// Load the default signing key or generate one.
    pub fn load_or_generate() -> Result<Self> {
        let path = Self::default_key_path();
        
        if path.exists() {
            Self::load(&path)
        } else {
            let key = Self::generate();
            key.save(&path)?;
            eprintln!("✨ Generated new signing key: {}", key.fingerprint());
            eprintln!("   Public key saved to: {}", path.display());
            Ok(key)
        }
    }
}

/// Stored key format.
#[derive(Debug, Serialize, Deserialize)]
struct StoredKey {
    secret_key: String,
    public_key: String,
    fingerprint: String,
    description: Option<String>,
    #[serde(with = "chrono::serde::ts_seconds")]
    created_at: DateTime<Utc>,
}

// =============================================================================
// Signature Verification
// =============================================================================

/// Verify a package signature.
pub fn verify_signature(
    tarball: &[u8],
    signature: &PackageSignature,
    public_key: &[u8; PUBLIC_KEY_LENGTH],
) -> Result<bool> {
    // Verify fingerprint matches
    let expected_fingerprint = compute_fingerprint(public_key);
    if signature.key_fingerprint != expected_fingerprint {
        return Err(Error::Registry(format!(
            "Key fingerprint mismatch: expected {}, got {}",
            expected_fingerprint, signature.key_fingerprint
        )));
    }
    
    // Decode and verify signature
    let sig_bytes = signature.signature_bytes()?;
    if sig_bytes.len() != 64 {
        return Err(Error::Registry("Invalid signature length".to_string()));
    }
    
    let mut sig_array = [0u8; 64];
    sig_array.copy_from_slice(&sig_bytes);
    
    let sig = Signature::from_bytes(&sig_array);
    let verifying_key = VerifyingKey::from_bytes(public_key)
        .map_err(|e| Error::Registry(format!("Invalid public key: {}", e)))?;
    
    match verifying_key.verify(tarball, &sig) {
        Ok(()) => Ok(true),
        Err(_) => Ok(false),
    }
}

/// Verify a package file.
pub fn verify_file(
    path: &Path,
    signature: &PackageSignature,
    public_key: &[u8; PUBLIC_KEY_LENGTH],
) -> Result<bool> {
    let content = fs::read(path)?;
    verify_signature(&content, signature, public_key)
}

// =============================================================================
// Trusted Publisher
// =============================================================================

/// A trusted package publisher.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustedPublisher {
    /// Publisher name or organization.
    pub name: String,
    
    /// Publisher email.
    pub email: Option<String>,
    
    /// Ed25519 public key (base64 encoded).
    pub public_key: String,
    
    /// Key fingerprint.
    pub fingerprint: String,
    
    /// When the key was registered.
    #[serde(with = "chrono::serde::ts_seconds")]
    pub registered_at: DateTime<Utc>,
    
    /// Key expiration (optional).
    #[serde(default, with = "chrono::serde::ts_seconds_option")]
    pub expires_at: Option<DateTime<Utc>>,
    
    /// Is this an officially verified publisher?
    pub verified: bool,
    
    /// Is this an official Roast project publisher?
    pub official: bool,
}

impl TrustedPublisher {
    /// Create a new trusted publisher from a public key.
    pub fn new(name: &str, public_key: &[u8; PUBLIC_KEY_LENGTH]) -> Self {
        Self {
            name: name.to_string(),
            email: None,
            public_key: base64_encode(public_key),
            fingerprint: compute_fingerprint(public_key),
            registered_at: Utc::now(),
            expires_at: None,
            verified: false,
            official: false,
        }
    }

    /// Get the public key bytes.
    pub fn public_key_bytes(&self) -> Result<[u8; PUBLIC_KEY_LENGTH]> {
        let bytes = base64_decode(&self.public_key)
            .map_err(|e| Error::Registry(format!("Invalid public key encoding: {}", e)))?;
        
        if bytes.len() != PUBLIC_KEY_LENGTH {
            return Err(Error::Registry("Invalid public key length".to_string()));
        }
        
        let mut key = [0u8; PUBLIC_KEY_LENGTH];
        key.copy_from_slice(&bytes);
        Ok(key)
    }

    /// Check if the key is expired.
    pub fn is_expired(&self) -> bool {
        if let Some(expires) = self.expires_at {
            Utc::now() > expires
        } else {
            false
        }
    }

    /// Verify a signature from this publisher.
    pub fn verify(&self, tarball: &[u8], signature: &PackageSignature) -> Result<bool> {
        if self.is_expired() {
            return Err(Error::Registry("Publisher key has expired".to_string()));
        }
        
        let public_key = self.public_key_bytes()?;
        verify_signature(tarball, signature, &public_key)
    }
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Compute the fingerprint (SHA256) of a public key.
fn compute_fingerprint(public_key: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(public_key);
    hex::encode(hasher.finalize())
}

/// Base64 encode bytes.
pub fn base64_encode(data: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(data)
}

/// Base64 decode a string.
pub fn base64_decode(s: &str) -> std::result::Result<Vec<u8>, base64::DecodeError> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode(s)
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_and_sign() {
        let key = SigningKey::generate();
        let tarball = b"test package contents";
        
        let signature = key.sign(tarball);
        
        assert_eq!(signature.key_fingerprint, key.fingerprint());
        assert_eq!(signature.algorithm, "ed25519");
    }

    #[test]
    fn test_verify_signature() {
        let key = SigningKey::generate();
        let tarball = b"test package contents";
        
        let signature = key.sign(tarball);
        let public_key = key.public_key_bytes();
        
        let result = verify_signature(tarball, &signature, &public_key);
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[test]
    fn test_verify_tampered() {
        let key = SigningKey::generate();
        let tarball = b"test package contents";
        
        let signature = key.sign(tarball);
        let public_key = key.public_key_bytes();
        
        // Tampered content
        let tampered = b"tampered contents";
        let result = verify_signature(tampered, &signature, &public_key);
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[test]
    fn test_fingerprint_consistency() {
        let key = SigningKey::generate();
        let public_key = key.public_key_bytes();
        
        let fingerprint1 = compute_fingerprint(&public_key);
        let fingerprint2 = compute_fingerprint(&public_key);
        
        assert_eq!(fingerprint1, fingerprint2);
        assert_eq!(fingerprint1, key.fingerprint());
    }

    #[test]
    fn test_trusted_publisher() {
        let key = SigningKey::generate();
        let public_key = key.public_key_bytes();
        
        let publisher = TrustedPublisher::new("test-publisher", &public_key);
        
        assert_eq!(publisher.fingerprint, key.fingerprint());
        assert!(!publisher.is_expired());
        
        let tarball = b"test package";
        let signature = key.sign(tarball);
        
        let verified = publisher.verify(tarball, &signature);
        assert!(verified.is_ok());
        assert!(verified.unwrap());
    }
}
