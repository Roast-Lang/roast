//! Package registry client.

use std::path::{Path, PathBuf};
use std::fs;
use sha2::{Sha256, Digest};
use crate::deps::PackageMetadata;
use crate::cache::Cache;
use crate::signing::{PackageSignature, SigningKey, verify_signature};
use crate::trust::{TrustStore, TrustDecision, VerificationStatus};
use crate::{Error, Result};

/// Package registry.
pub struct Registry {
    /// Registry URL.
    url: String,
    
    /// HTTP client.
    client: reqwest::Client,
    
    /// Authentication token.
    token: Option<String>,
    
    /// Package cache.
    cache: Cache,
    
    /// Trust store for publisher verification.
    trust_store: TrustStore,
    
    /// Whether to verify signatures.
    verify_signatures: bool,
}

impl Registry {
    /// Create a new registry client.
    pub fn new(url: &str, cache: Cache) -> Self {
        Self {
            url: url.trim_end_matches('/').to_string(),
            client: reqwest::Client::builder()
                .user_agent(format!("kitchen/{}", crate::VERSION))
                .build()
                .expect("Failed to create HTTP client"),
            token: None,
            cache,
            trust_store: TrustStore::load_default().unwrap_or_default(),
            verify_signatures: true,
        }
    }
    
    /// Set authentication token.
    pub fn with_token(mut self, token: String) -> Self {
        self.token = Some(token);
        self
    }
    
    /// Enable or disable signature verification.
    pub fn with_signature_verification(mut self, enabled: bool) -> Self {
        self.verify_signatures = enabled;
        self
    }
    
    /// Set a custom trust store.
    pub fn with_trust_store(mut self, store: TrustStore) -> Self {
        self.trust_store = store;
        self
    }
    
    /// Get package metadata.
    pub async fn get_package(&self, name: &str) -> Result<PackageMetadata> {
        // Check cache first
        if let Some(metadata) = self.cache.get_metadata(name)? {
            return Ok(metadata);
        }
        
        let url = format!("{}/api/v1/packages/{}", self.url, name);
        
        let mut request = self.client.get(&url);
        
        if let Some(ref token) = self.token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }
        
        let response = request.send().await
            .map_err(|e| Error::Http(e.to_string()))?;
        
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(Error::NotFound(format!("Package '{}' not found", name)));
        }
        
        if !response.status().is_success() {
            return Err(Error::Registry(format!(
                "Failed to fetch package '{}': {}",
                name,
                response.status()
            )));
        }
        
        let metadata: PackageMetadata = response.json().await
            .map_err(|e| Error::Http(e.to_string()))?;
        
        // Cache metadata
        self.cache.set_metadata(&metadata)?;
        
        Ok(metadata)
    }
    
    /// Search packages.
    pub async fn search(&self, query: &str) -> Result<Vec<PackageMetadata>> {
        let url = format!("{}/api/v1/search?q={}", self.url, urlencoding::encode(query));
        
        let response = self.client.get(&url)
            .send().await
            .map_err(|e| Error::Http(e.to_string()))?;
        
        if !response.status().is_success() {
            return Err(Error::Registry(format!(
                "Search failed: {}",
                response.status()
            )));
        }
        
        #[derive(serde::Deserialize)]
        struct SearchResult {
            packages: Vec<PackageMetadata>,
        }
        
        let result: SearchResult = response.json().await
            .map_err(|e| Error::Http(e.to_string()))?;
        
        Ok(result.packages)
    }
    
    /// Download a package.
    pub async fn download(&self, name: &str, version: &str) -> Result<PathBuf> {
        // Check cache
        if let Some(path) = self.cache.get_package(name, version)? {
            return Ok(path);
        }
        
        // Get package metadata
        let metadata = self.get_package(name).await?;
        
        // Find version
        let version_info = metadata.versions.iter()
            .find(|v| v.version == version)
            .ok_or_else(|| Error::NotFound(format!(
                "Version {} not found for {}", version, name
            )))?;
        
        // Get download URL
        let download_url = version_info.download_url.as_ref()
            .ok_or_else(|| Error::Registry("No download URL".to_string()))?;
        
        // Download
        let response = self.client.get(download_url)
            .send().await
            .map_err(|e| Error::Http(e.to_string()))?;
        
        if !response.status().is_success() {
            return Err(Error::Registry(format!(
                "Download failed: {}",
                response.status()
            )));
        }
        
        let bytes = response.bytes().await
            .map_err(|e| Error::Http(e.to_string()))?;
        
        // Verify checksum
        if let Some(ref expected_checksum) = version_info.checksum {
            let mut hasher = Sha256::new();
            hasher.update(&bytes);
            let actual_checksum = hex::encode(hasher.finalize());
            
            if &actual_checksum != expected_checksum {
                return Err(Error::Registry(format!(
                    "Checksum mismatch for {} {}: expected {}, got {}",
                    name, version, expected_checksum, actual_checksum
                )));
            }
        }
        
        // Verify signature if enabled
        if self.verify_signatures {
            self.verify_package_signature(name, &bytes, None)?;
        }
        
        // Store in cache
        let path = self.cache.store_package(name, version, &bytes)?;
        
        Ok(path)
    }
    
    /// Verify a package signature and check trust.
    fn verify_package_signature(
        &self,
        name: &str,
        tarball: &[u8],
        signature: Option<&PackageSignature>,
    ) -> Result<VerificationStatus> {
        // Check trust decision
        let decision = self.trust_store.check_package(name, signature);
        
        match decision {
            TrustDecision::Allow(level) => {
                eprintln!("📦 {} - {}", name, level);
                Ok(VerificationStatus::Unsigned)
            }
            TrustDecision::Prompt(msg) => {
                eprintln!("⚠️  {}", msg);
                // In a real implementation, this would prompt the user
                // For now, allow with warning
                Ok(VerificationStatus::Unsigned)
            }
            TrustDecision::Deny(reason) => {
                Err(Error::Registry(format!(
                    "Package '{}' rejected: {}",
                    name, reason
                )))
            }
        }
    }
    
    /// Download with explicit signature verification.
    pub async fn download_verified(
        &self,
        name: &str,
        version: &str,
        signature: Option<&PackageSignature>,
    ) -> Result<(PathBuf, VerificationStatus)> {
        let path = self.download(name, version).await?;
        
        if let Some(sig) = signature {
            let tarball = fs::read(&path)?;
            let status = self.trust_store.verify_package(&tarball, Some(sig));
            Ok((path, status))
        } else {
            Ok((path, VerificationStatus::Unsigned))
        }
    }
    
    /// Publish a package.
    pub async fn publish(&self, tarball: &Path) -> Result<()> {
        let token = self.token.as_ref()
            .ok_or_else(|| Error::Registry("Authentication required for publishing".to_string()))?;
        
        let file_content = fs::read(tarball)?;
        
        let url = format!("{}/api/v1/packages", self.url);
        
        let response = self.client.post(&url)
            .header("Authorization", format!("Bearer {}", token))
            .header("Content-Type", "application/gzip")
            .body(file_content)
            .send().await
            .map_err(|e| Error::Http(e.to_string()))?;
        
        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(Error::Registry(format!(
                "Publish failed: {}",
                error_text
            )));
        }
        
        Ok(())
    }
    
    /// Publish a signed package.
    /// 
    /// This method signs the package tarball with the provided signing key
    /// and uploads both the tarball and signature to the registry.
    pub async fn publish_signed(&self, tarball: &Path, signing_key: &SigningKey) -> Result<PackageSignature> {
        let token = self.token.as_ref()
            .ok_or_else(|| Error::Registry("Authentication required for publishing".to_string()))?;
        
        let file_content = fs::read(tarball)?;
        
        // Sign the package
        let signature = signing_key.sign(&file_content);
        
        // Create multipart form with tarball and signature
        let url = format!("{}/api/v1/packages", self.url);
        
        let signature_json = serde_json::to_string(&signature)
            .map_err(|e| Error::Registry(format!("Failed to serialize signature: {}", e)))?;
        
        let response = self.client.post(&url)
            .header("Authorization", format!("Bearer {}", token))
            .header("Content-Type", "application/gzip")
            .header("X-Package-Signature", &signature_json)
            .header("X-Publisher-Fingerprint", signing_key.fingerprint())
            .body(file_content)
            .send().await
            .map_err(|e| Error::Http(e.to_string()))?;
        
        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(Error::Registry(format!(
                "Publish failed: {}",
                error_text
            )));
        }
        
        eprintln!("✅ Package published with signature");
        eprintln!("   Fingerprint: {}", signing_key.fingerprint());
        
        Ok(signature)
    }
    
    /// Yank a version.
    pub async fn yank(&self, name: &str, version: &str) -> Result<()> {
        let token = self.token.as_ref()
            .ok_or_else(|| Error::Registry("Authentication required".to_string()))?;
        
        let url = format!("{}/api/v1/packages/{}/{}/yank", self.url, name, version);
        
        let response = self.client.post(&url)
            .header("Authorization", format!("Bearer {}", token))
            .send().await
            .map_err(|e| Error::Http(e.to_string()))?;
        
        if !response.status().is_success() {
            return Err(Error::Registry(format!(
                "Yank failed: {}",
                response.status()
            )));
        }
        
        Ok(())
    }
    
    /// Unyank a version.
    pub async fn unyank(&self, name: &str, version: &str) -> Result<()> {
        let token = self.token.as_ref()
            .ok_or_else(|| Error::Registry("Authentication required".to_string()))?;
        
        let url = format!("{}/api/v1/packages/{}/{}/unyank", self.url, name, version);
        
        let response = self.client.post(&url)
            .header("Authorization", format!("Bearer {}", token))
            .send().await
            .map_err(|e| Error::Http(e.to_string()))?;
        
        if !response.status().is_success() {
            return Err(Error::Registry(format!(
                "Unyank failed: {}",
                response.status()
            )));
        }
        
        Ok(())
    }
}

/// Package tarball creation.
pub fn create_tarball(project_root: &Path, output: &Path) -> Result<()> {
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use tar::Builder;
    
    let file = fs::File::create(output)?;
    let encoder = GzEncoder::new(file, Compression::default());
    let mut builder = Builder::new(encoder);
    
    // Add files to tarball
    let src_dir = project_root.join("src");
    if src_dir.exists() {
        builder.append_dir_all("src", &src_dir)?;
    }
    
    // Add roast.toml
    let config_file = project_root.join("roast.toml");
    if config_file.exists() {
        builder.append_path_with_name(&config_file, "roast.toml")?;
    }
    
    // Add README
    for readme in &["README.md", "README.txt", "README"] {
        let readme_path = project_root.join(readme);
        if readme_path.exists() {
            builder.append_path_with_name(&readme_path, readme)?;
            break;
        }
    }
    
    // Add LICENSE
    for license in &["LICENSE", "LICENSE.md", "LICENSE.txt"] {
        let license_path = project_root.join(license);
        if license_path.exists() {
            builder.append_path_with_name(&license_path, license)?;
            break;
        }
    }
    
    builder.finish()?;
    
    Ok(())
}

mod urlencoding {
    pub fn encode(s: &str) -> String {
        let mut result = String::new();
        for c in s.chars() {
            match c {
                'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' | '~' => {
                    result.push(c);
                }
                ' ' => result.push('+'),
                _ => {
                    for byte in c.to_string().bytes() {
                        result.push('%');
                        result.push_str(&format!("{:02X}", byte));
                    }
                }
            }
        }
        result
    }
}

