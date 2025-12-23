//! Trust store for package publishers.
//!
//! Manages trusted publishers, revoked keys, and verification policies.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::fs;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use crate::signing::{PackageSignature, TrustedPublisher};
use crate::deps::PackageMetadata;
use crate::{Error, Result};

// =============================================================================
// Verification Status
// =============================================================================

/// Result of signature verification.
#[derive(Debug, Clone)]
pub enum VerificationStatus {
    /// Signature verified successfully.
    Verified {
        publisher: Option<TrustedPublisher>,
        signed_at: DateTime<Utc>,
    },
    
    /// Package has no signature.
    Unsigned,
    
    /// Signature is invalid.
    Invalid(String),
    
    /// Publisher is not trusted.
    UntrustedPublisher {
        fingerprint: String,
    },
    
    /// Publisher key is revoked.
    RevokedKey {
        fingerprint: String,
        revoked_at: DateTime<Utc>,
    },
}

impl VerificationStatus {
    /// Check if the package should be trusted.
    pub fn is_trusted(&self) -> bool {
        matches!(self, VerificationStatus::Verified { .. })
    }
    
    /// Get a human-readable status message.
    pub fn message(&self) -> String {
        match self {
            VerificationStatus::Verified { publisher, .. } => {
                if let Some(pub_info) = publisher {
                    if pub_info.official {
                        format!("✅ Verified (Official: {})", pub_info.name)
                    } else if pub_info.verified {
                        format!("✅ Verified ({})", pub_info.name)
                    } else {
                        format!("✓ Signed by {}", pub_info.name)
                    }
                } else {
                    "✓ Signature valid".to_string()
                }
            }
            VerificationStatus::Unsigned => "⚠️  Unsigned package".to_string(),
            VerificationStatus::Invalid(reason) => format!("❌ Invalid signature: {}", reason),
            VerificationStatus::UntrustedPublisher { fingerprint } => {
                format!("⚠️  Unknown publisher (key: {}...)", &fingerprint[..12.min(fingerprint.len())])
            }
            VerificationStatus::RevokedKey { fingerprint, .. } => {
                format!("❌ Revoked key: {}...", &fingerprint[..12.min(fingerprint.len())])
            }
        }
    }
}

// =============================================================================
// Trust Store
// =============================================================================

/// Trust store for managing publisher trust.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustStore {
    /// Official Roast packages (always trusted).
    #[serde(default)]
    official_packages: HashSet<String>,
    
    /// Official publisher fingerprints.
    #[serde(default)]
    official_fingerprints: HashSet<String>,
    
    /// User-trusted publishers by fingerprint.
    #[serde(default)]
    trusted_publishers: HashMap<String, TrustedPublisher>,
    
    /// Revoked key fingerprints.
    #[serde(default)]
    revoked_keys: HashMap<String, KeyRevocation>,
    
    /// Trust policies.
    #[serde(default)]
    policies: TrustPolicies,
    
    /// Last updated (stored as timestamp).
    #[serde(default = "default_now", with = "chrono::serde::ts_seconds")]
    updated_at: DateTime<Utc>,
}

fn default_now() -> DateTime<Utc> {
    Utc::now()
}

/// Key revocation information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyRevocation {
    /// Key fingerprint.
    pub fingerprint: String,
    
    /// Revocation reason.
    pub reason: String,
    
    /// When the key was revoked.
    #[serde(with = "chrono::serde::ts_seconds")]
    pub revoked_at: DateTime<Utc>,
    
    /// Who revoked the key.
    pub revoked_by: Option<String>,
}

/// Trust policies configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustPolicies {
    /// Require signatures for all packages.
    #[serde(default)]
    pub require_signatures: bool,
    
    /// Warn on unsigned packages.
    #[serde(default = "default_true")]
    pub warn_unsigned: bool,
    
    /// Warn on untrusted publishers.
    #[serde(default = "default_true")]
    pub warn_untrusted: bool,
    
    /// Auto-trust packages signed by known publishers.
    #[serde(default)]
    pub auto_trust_known: bool,
    
    /// Prompt before installing from new publishers.
    #[serde(default = "default_true")]
    pub prompt_new_publishers: bool,
    
    /// Allow packages from revoked keys.
    #[serde(default)]
    pub allow_revoked: bool,
    
    /// Reserved package name patterns (cannot be published by non-officials).
    #[serde(default = "default_reserved_patterns")]
    pub reserved_patterns: Vec<String>,
}

fn default_true() -> bool {
    true
}

fn default_reserved_patterns() -> Vec<String> {
    vec![
        "roast-*".to_string(),
        "std-*".to_string(),
        "core-*".to_string(),
        "stdlib-*".to_string(),
    ]
}

impl Default for TrustPolicies {
    fn default() -> Self {
        Self {
            require_signatures: false,
            warn_unsigned: true,
            warn_untrusted: true,
            auto_trust_known: false,
            prompt_new_publishers: true,
            allow_revoked: false,
            reserved_patterns: default_reserved_patterns(),
        }
    }
}

/// Trust decision for a package.
#[derive(Debug, Clone)]
pub enum TrustDecision {
    /// Allow installation.
    Allow(TrustLevel),
    
    /// Prompt user before installation.
    Prompt(String),
    
    /// Deny installation.
    Deny(String),
}

/// Trust level for a publisher/package.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TrustLevel {
    /// No trust - warn on install.
    Untrusted,
    
    /// User explicitly trusted.
    Trusted,
    
    /// Verified publisher (email/identity verified).
    Verified,
    
    /// Official Roast project.
    Official,
}

impl std::fmt::Display for TrustLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TrustLevel::Untrusted => write!(f, "⚪ Untrusted"),
            TrustLevel::Trusted => write!(f, "🔵 Trusted"),
            TrustLevel::Verified => write!(f, "✅ Verified"),
            TrustLevel::Official => write!(f, "🏆 Official"),
        }
    }
}

impl Default for TrustStore {
    fn default() -> Self {
        Self::new()
    }
}

impl TrustStore {
    /// Create a new empty trust store.
    pub fn new() -> Self {
        Self {
            official_packages: Self::default_official_packages(),
            official_fingerprints: HashSet::new(),
            trusted_publishers: HashMap::new(),
            revoked_keys: HashMap::new(),
            policies: TrustPolicies::default(),
            updated_at: Utc::now(),
        }
    }

    /// Get default official package names.
    fn default_official_packages() -> HashSet<String> {
        let mut packages = HashSet::new();
        // Core packages that should always be official
        for name in &[
            "roast-std",
            "roast-io",
            "roast-async",
            "roast-http",
            "roast-json",
            "roast-testing",
            "roast-crypto",
        ] {
            packages.insert(name.to_string());
        }
        packages
    }

    /// Load trust store from file.
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::new());
        }
        
        let content = fs::read_to_string(path)?;
        let store: TrustStore = toml::from_str(&content)
            .map_err(|e| Error::InvalidConfig(format!("Invalid trust store: {}", e)))?;
        
        Ok(store)
    }

    /// Save trust store to file.
    pub fn save(&self, path: &Path) -> Result<()> {
        // Create parent directory if needed
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        
        let content = toml::to_string_pretty(self)
            .map_err(|e| Error::InvalidConfig(format!("Failed to serialize trust store: {}", e)))?;
        
        let content = format!(
            "# Kitchen Trust Store\n\
             # This file manages trusted package publishers.\n\
             # Edit with caution.\n\n{}",
            content
        );
        
        fs::write(path, content)?;
        Ok(())
    }

    /// Get default trust store path.
    pub fn default_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from(".config"))
            .join("kitchen")
            .join("trust.toml")
    }

    /// Load the default trust store.
    pub fn load_default() -> Result<Self> {
        Self::load(&Self::default_path())
    }

    /// Save to the default path.
    pub fn save_default(&self) -> Result<()> {
        self.save(&Self::default_path())
    }

    // =========================================================================
    // Publisher Management
    // =========================================================================

    /// Add a trusted publisher.
    pub fn add_publisher(&mut self, publisher: TrustedPublisher) {
        self.trusted_publishers.insert(
            publisher.fingerprint.clone(),
            publisher,
        );
        self.updated_at = Utc::now();
    }

    /// Remove a trusted publisher.
    pub fn remove_publisher(&mut self, fingerprint: &str) -> Option<TrustedPublisher> {
        self.updated_at = Utc::now();
        self.trusted_publishers.remove(fingerprint)
    }

    /// Get a publisher by fingerprint.
    pub fn get_publisher(&self, fingerprint: &str) -> Option<&TrustedPublisher> {
        self.trusted_publishers.get(fingerprint)
    }

    /// List all trusted publishers.
    pub fn list_publishers(&self) -> Vec<&TrustedPublisher> {
        self.trusted_publishers.values().collect()
    }

    /// Check if a publisher is trusted.
    pub fn is_trusted(&self, fingerprint: &str) -> bool {
        self.official_fingerprints.contains(fingerprint) ||
        self.trusted_publishers.contains_key(fingerprint)
    }

    // =========================================================================
    // Key Revocation
    // =========================================================================

    /// Revoke a key.
    pub fn revoke_key(&mut self, fingerprint: &str, reason: &str) {
        self.revoked_keys.insert(
            fingerprint.to_string(),
            KeyRevocation {
                fingerprint: fingerprint.to_string(),
                reason: reason.to_string(),
                revoked_at: Utc::now(),
                revoked_by: None,
            },
        );
        
        // Remove from trusted publishers
        self.trusted_publishers.remove(fingerprint);
        self.updated_at = Utc::now();
    }

    /// Check if a key is revoked.
    pub fn is_revoked(&self, fingerprint: &str) -> bool {
        self.revoked_keys.contains_key(fingerprint)
    }

    /// Get revocation info for a key.
    pub fn get_revocation(&self, fingerprint: &str) -> Option<&KeyRevocation> {
        self.revoked_keys.get(fingerprint)
    }

    // =========================================================================
    // Trust Verification
    // =========================================================================

    /// Check if a package name is reserved.
    pub fn is_reserved_name(&self, name: &str) -> bool {
        for pattern in &self.policies.reserved_patterns {
            if pattern.ends_with('*') {
                let prefix = pattern.trim_end_matches('*');
                if name.starts_with(prefix) {
                    return true;
                }
            } else if pattern == name {
                return true;
            }
        }
        false
    }

    /// Check if a package is official.
    pub fn is_official_package(&self, name: &str) -> bool {
        self.official_packages.contains(name)
    }

    /// Get trust level for a fingerprint.
    pub fn get_trust_level(&self, fingerprint: &str) -> TrustLevel {
        if self.official_fingerprints.contains(fingerprint) {
            TrustLevel::Official
        } else if let Some(publisher) = self.trusted_publishers.get(fingerprint) {
            if publisher.official {
                TrustLevel::Official
            } else if publisher.verified {
                TrustLevel::Verified
            } else {
                TrustLevel::Trusted
            }
        } else {
            TrustLevel::Untrusted
        }
    }

    /// Verify a package signature and determine trust status.
    pub fn verify_package(
        &self,
        tarball: &[u8],
        signature: Option<&PackageSignature>,
    ) -> VerificationStatus {
        // Check if signature exists
        let signature = match signature {
            Some(sig) => sig,
            None => return VerificationStatus::Unsigned,
        };
        
        let fingerprint = &signature.key_fingerprint;
        
        // Check if key is revoked
        if let Some(revocation) = self.revoked_keys.get(fingerprint) {
            return VerificationStatus::RevokedKey {
                fingerprint: fingerprint.clone(),
                revoked_at: revocation.revoked_at,
            };
        }
        
        // Get publisher info
        let publisher = self.trusted_publishers.get(fingerprint).cloned();
        
        // Verify signature
        if let Some(ref pub_info) = publisher {
            match pub_info.verify(tarball, signature) {
                Ok(true) => VerificationStatus::Verified {
                    publisher,
                    signed_at: signature.signed_at,
                },
                Ok(false) => VerificationStatus::Invalid("Signature does not match".to_string()),
                Err(e) => VerificationStatus::Invalid(e.to_string()),
            }
        } else {
            VerificationStatus::UntrustedPublisher {
                fingerprint: fingerprint.clone(),
            }
        }
    }

    /// Make a trust decision for a package.
    pub fn check_package(
        &self,
        name: &str,
        signature: Option<&PackageSignature>,
    ) -> TrustDecision {
        // Check if it's an official package
        if self.official_packages.contains(name) {
            return TrustDecision::Allow(TrustLevel::Official);
        }
        
        // Check if package name is reserved
        if self.is_reserved_name(name) {
            // Reserved packages must come from official publishers
            if let Some(sig) = signature {
                if !self.official_fingerprints.contains(&sig.key_fingerprint) {
                    return TrustDecision::Deny(format!(
                        "Package '{}' uses a reserved name but is not from an official publisher",
                        name
                    ));
                }
            } else if self.policies.require_signatures {
                return TrustDecision::Deny(format!(
                    "Package '{}' uses a reserved name but has no signature",
                    name
                ));
            }
        }
        
        // Check signature
        match signature {
            None => {
                if self.policies.require_signatures {
                    TrustDecision::Deny("Package is not signed".to_string())
                } else if self.policies.warn_unsigned {
                    TrustDecision::Prompt("Package is not signed. Install anyway?".to_string())
                } else {
                    TrustDecision::Allow(TrustLevel::Untrusted)
                }
            }
            Some(sig) => {
                let fingerprint = &sig.key_fingerprint;
                
                // Check revocation
                if self.is_revoked(fingerprint) {
                    return TrustDecision::Deny("Publisher key has been revoked".to_string());
                }
                
                // Get trust level
                let level = self.get_trust_level(fingerprint);
                
                match level {
                    TrustLevel::Official | TrustLevel::Verified => {
                        TrustDecision::Allow(level)
                    }
                    TrustLevel::Trusted => {
                        TrustDecision::Allow(TrustLevel::Trusted)
                    }
                    TrustLevel::Untrusted => {
                        if self.policies.prompt_new_publishers {
                            TrustDecision::Prompt(format!(
                                "Package is from unknown publisher (key: {}...). Trust this publisher?",
                                &fingerprint[..12]
                            ))
                        } else if self.policies.warn_untrusted {
                            eprintln!("⚠️  Package from untrusted publisher: {}...", &fingerprint[..12]);
                            TrustDecision::Allow(TrustLevel::Untrusted)
                        } else {
                            TrustDecision::Allow(TrustLevel::Untrusted)
                        }
                    }
                }
            }
        }
    }

    // =========================================================================
    // Policy Management
    // =========================================================================

    /// Get policies.
    pub fn policies(&self) -> &TrustPolicies {
        &self.policies
    }

    /// Set policies.
    pub fn set_policies(&mut self, policies: TrustPolicies) {
        self.policies = policies;
        self.updated_at = Utc::now();
    }

    /// Enable strict mode (require signatures).
    pub fn enable_strict_mode(&mut self) {
        self.policies.require_signatures = true;
        self.policies.warn_unsigned = true;
        self.policies.warn_untrusted = true;
        self.policies.prompt_new_publishers = true;
        self.policies.allow_revoked = false;
        self.updated_at = Utc::now();
    }

    /// Enable permissive mode (no signature requirements).
    pub fn enable_permissive_mode(&mut self) {
        self.policies.require_signatures = false;
        self.policies.warn_unsigned = false;
        self.policies.warn_untrusted = false;
        self.policies.prompt_new_publishers = false;
        self.updated_at = Utc::now();
    }
}

// =============================================================================
// CLI Commands for Trust Management
// =============================================================================

/// Add a publisher to the trust store.
pub fn trust_publisher(
    name: &str,
    public_key_base64: &str,
) -> Result<()> {
    use base64::Engine;
    
    let key_bytes = base64::engine::general_purpose::STANDARD.decode(public_key_base64)
        .map_err(|e| Error::Registry(format!("Invalid public key: {}", e)))?;
    
    if key_bytes.len() != 32 {
        return Err(Error::Registry("Invalid public key length".to_string()));
    }
    
    let mut key_array = [0u8; 32];
    key_array.copy_from_slice(&key_bytes);
    
    let publisher = TrustedPublisher::new(name, &key_array);
    
    let mut store = TrustStore::load_default()?;
    store.add_publisher(publisher.clone());
    store.save_default()?;
    
    eprintln!("✅ Added trusted publisher: {}", name);
    eprintln!("   Fingerprint: {}", publisher.fingerprint);
    
    Ok(())
}

/// Remove a publisher from the trust store.
pub fn untrust_publisher(fingerprint: &str) -> Result<()> {
    let mut store = TrustStore::load_default()?;
    
    if let Some(publisher) = store.remove_publisher(fingerprint) {
        store.save_default()?;
        eprintln!("✅ Removed trusted publisher: {}", publisher.name);
    } else {
        eprintln!("⚠️  Publisher not found: {}", fingerprint);
    }
    
    Ok(())
}

/// List all trusted publishers.
pub fn list_trusted_publishers() -> Result<()> {
    let store = TrustStore::load_default()?;
    
    let publishers = store.list_publishers();
    
    if publishers.is_empty() {
        println!("No trusted publishers configured.");
        return Ok(());
    }
    
    println!("Trusted Publishers:");
    println!("{}", "-".repeat(60));
    
    for publisher in publishers {
        let status = if publisher.official {
            "🏆 Official"
        } else if publisher.verified {
            "✅ Verified"
        } else {
            "🔵 Trusted"
        };
        
        println!("{} {} ({})", status, publisher.name, &publisher.fingerprint[..12]);
        if let Some(ref email) = publisher.email {
            println!("   Email: {}", email);
        }
        println!("   Added: {}", publisher.registered_at.format("%Y-%m-%d"));
    }
    
    Ok(())
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trust_store_new() {
        let store = TrustStore::new();
        assert!(store.official_packages.contains("roast-std"));
        assert!(!store.policies.require_signatures);
    }

    #[test]
    fn test_reserved_names() {
        let store = TrustStore::new();
        
        assert!(store.is_reserved_name("roast-http"));
        assert!(store.is_reserved_name("std-io"));
        assert!(store.is_reserved_name("core-async"));
        assert!(!store.is_reserved_name("my-package"));
        assert!(!store.is_reserved_name("awesome-tool"));
    }

    #[test]
    fn test_add_remove_publisher() {
        let mut store = TrustStore::new();
        
        let key = [0u8; 32];
        let publisher = TrustedPublisher::new("test", &key);
        let fingerprint = publisher.fingerprint.clone();
        
        store.add_publisher(publisher);
        assert!(store.is_trusted(&fingerprint));
        
        store.remove_publisher(&fingerprint);
        assert!(!store.is_trusted(&fingerprint));
    }

    #[test]
    fn test_key_revocation() {
        let mut store = TrustStore::new();
        
        let fingerprint = "abc123";
        store.revoke_key(fingerprint, "Compromised");
        
        assert!(store.is_revoked(fingerprint));
        
        let revocation = store.get_revocation(fingerprint).unwrap();
        assert_eq!(revocation.reason, "Compromised");
    }

    #[test]
    fn test_trust_decision_unsigned() {
        let store = TrustStore::new();
        
        let decision = store.check_package("my-package", None);
        
        // Default policy should prompt for unsigned
        assert!(matches!(decision, TrustDecision::Prompt(_)));
    }

    #[test]
    fn test_trust_decision_reserved() {
        let mut store = TrustStore::new();
        store.policies.require_signatures = true;
        
        // Use a reserved name pattern that's NOT in the official packages list
        let decision = store.check_package("roast-malicious", None);
        
        // Reserved packages without signature should be denied
        assert!(matches!(decision, TrustDecision::Deny(_)));
    }

    #[test]
    fn test_trust_levels() {
        let store = TrustStore::new();
        
        // Unknown fingerprint should be untrusted
        let level = store.get_trust_level("unknown-fingerprint");
        assert_eq!(level, TrustLevel::Untrusted);
    }
}
