use crate::config::schema::Config;
use aes_gcm::aead::rand_core::RngCore;
use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};

const PREFIX: &str = "tsenc:v1:";

pub fn is_encrypted_secret(value: &str) -> bool {
    value.starts_with(PREFIX)
}

pub fn decrypt_config(config: &mut Config) -> Result<(), CredentialCryptoError> {
    let key_env = config.security.credential_encryption.key_env.clone();
    decrypt_string(&mut config.server.master_api_key, &key_env)?;
    for provider in &mut config.providers {
        if let Some(api_key) = provider.api_key.as_mut() {
            decrypt_string(api_key, &key_env)?;
        }
    }
    Ok(())
}

pub fn encrypted_for_storage(config: &Config) -> Result<Config, CredentialCryptoError> {
    let mut stored = config.clone();
    if !stored.security.credential_encryption.enabled {
        return Ok(stored);
    }
    let key_env = stored.security.credential_encryption.key_env.clone();
    encrypt_string(&mut stored.server.master_api_key, &key_env)?;
    for provider in &mut stored.providers {
        if let Some(api_key) = provider.api_key.as_mut() {
            encrypt_string(api_key, &key_env)?;
        }
    }
    Ok(stored)
}

fn encrypt_string(value: &mut String, key_env: &str) -> Result<(), CredentialCryptoError> {
    if value.is_empty()
        || is_encrypted_secret(value)
        || crate::util::redact::is_redacted_secret(value)
    {
        return Ok(());
    }
    let key = key_from_env(key_env)?;
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| CredentialCryptoError::InvalidKey)?;
    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, value.as_bytes())
        .map_err(|_| CredentialCryptoError::Encrypt)?;
    *value = format!(
        "{PREFIX}{}:{}",
        URL_SAFE_NO_PAD.encode(nonce_bytes),
        URL_SAFE_NO_PAD.encode(ciphertext)
    );
    Ok(())
}

fn decrypt_string(value: &mut String, key_env: &str) -> Result<(), CredentialCryptoError> {
    if !is_encrypted_secret(value) {
        return Ok(());
    }
    let encoded = value.trim_start_matches(PREFIX);
    let (nonce, ciphertext) = encoded
        .split_once(':')
        .ok_or(CredentialCryptoError::MalformedCiphertext)?;
    let nonce = URL_SAFE_NO_PAD
        .decode(nonce)
        .map_err(|_| CredentialCryptoError::MalformedCiphertext)?;
    if nonce.len() != 12 {
        return Err(CredentialCryptoError::MalformedCiphertext);
    }
    let ciphertext = URL_SAFE_NO_PAD
        .decode(ciphertext)
        .map_err(|_| CredentialCryptoError::MalformedCiphertext)?;
    let key = key_from_env(key_env)?;
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| CredentialCryptoError::InvalidKey)?;
    let plaintext = cipher
        .decrypt(Nonce::from_slice(&nonce), ciphertext.as_ref())
        .map_err(|_| CredentialCryptoError::Decrypt)?;
    *value = String::from_utf8(plaintext).map_err(|_| CredentialCryptoError::Decrypt)?;
    Ok(())
}

fn key_from_env(key_env: &str) -> Result<[u8; 32], CredentialCryptoError> {
    let raw = std::env::var(key_env)
        .map_err(|_| CredentialCryptoError::MissingKey(key_env.to_string()))?;
    if raw.is_empty() {
        return Err(CredentialCryptoError::MissingKey(key_env.to_string()));
    }
    let digest = Sha256::digest(raw.as_bytes());
    let mut key = [0u8; 32];
    key.copy_from_slice(&digest);
    Ok(key)
}

#[derive(Debug, thiserror::Error)]
pub enum CredentialCryptoError {
    #[error("credential encryption key env var {0} is not set")]
    MissingKey(String),
    #[error("credential encryption key is invalid")]
    InvalidKey,
    #[error("failed to encrypt credential")]
    Encrypt,
    #[error("failed to decrypt credential")]
    Decrypt,
    #[error("encrypted credential is malformed")]
    MalformedCiphertext,
    #[error("failed to generate encryption nonce: {0}")]
    Random(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::schema::{CredentialEncryptionConfig, SecurityConfig};

    #[test]
    fn encrypts_and_decrypts_config_secrets() {
        unsafe {
            std::env::set_var("TOKENSCAVENGER_TEST_KEY", "test-key");
        }
        let config = Config {
            security: SecurityConfig {
                credential_encryption: CredentialEncryptionConfig {
                    enabled: true,
                    key_env: "TOKENSCAVENGER_TEST_KEY".into(),
                },
            },
            server: crate::config::schema::ServerConfig {
                master_api_key: "master-secret".into(),
                ..Default::default()
            },
            providers: vec![crate::config::schema::ProviderConfig {
                id: "groq".into(),
                api_key: Some("provider-secret".into()),
                ..Default::default()
            }],
            ..Default::default()
        };

        let mut encrypted = encrypted_for_storage(&config).unwrap();
        assert!(is_encrypted_secret(&encrypted.server.master_api_key));
        assert!(is_encrypted_secret(
            encrypted.providers[0].api_key.as_deref().unwrap()
        ));
        decrypt_config(&mut encrypted).unwrap();
        assert_eq!(encrypted.server.master_api_key, "master-secret");
        assert_eq!(
            encrypted.providers[0].api_key.as_deref(),
            Some("provider-secret")
        );
    }
}
