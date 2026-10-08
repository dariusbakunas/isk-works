use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    AeadCore, Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Serialize};

use crate::EsiError;

#[derive(Clone)]
pub struct SecretCipher {
    key: [u8; 32],
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct EncryptedSecret {
    pub version: u8,
    pub key_id: String,
    pub nonce: String,
    pub ciphertext: String,
}

impl SecretCipher {
    pub fn from_base64(key: &str) -> Result<Self, EsiError> {
        let bytes = STANDARD
            .decode(key)
            .map_err(|_| EsiError::Configuration("TOKEN_ENCRYPTION_KEY must be base64"))?;
        let key: [u8; 32] = bytes
            .try_into()
            .map_err(|_| EsiError::Configuration("TOKEN_ENCRYPTION_KEY must decode to 32 bytes"))?;
        Ok(Self { key })
    }

    pub fn encrypt(&self, secret: &str) -> Result<EncryptedSecret, EsiError> {
        let cipher = Aes256Gcm::new_from_slice(&self.key)
            .map_err(|_| EsiError::Configuration("invalid encryption key"))?;
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let ciphertext = cipher
            .encrypt(&nonce, secret.as_bytes())
            .map_err(|_| EsiError::SecretEncryption)?;
        Ok(EncryptedSecret {
            version: 1,
            key_id: "local-v1".to_string(),
            nonce: STANDARD.encode(nonce),
            ciphertext: STANDARD.encode(ciphertext),
        })
    }

    pub fn decrypt(&self, envelope: &EncryptedSecret) -> Result<String, EsiError> {
        if envelope.version != 1 {
            return Err(EsiError::SecretDecryption);
        }
        let nonce = STANDARD
            .decode(&envelope.nonce)
            .map_err(|_| EsiError::SecretDecryption)?;
        let ciphertext = STANDARD
            .decode(&envelope.ciphertext)
            .map_err(|_| EsiError::SecretDecryption)?;
        let nonce = Nonce::from_slice(&nonce);
        let plaintext = Aes256Gcm::new_from_slice(&self.key)
            .map_err(|_| EsiError::SecretDecryption)?
            .decrypt(nonce, ciphertext.as_ref())
            .map_err(|_| EsiError::SecretDecryption)?;
        String::from_utf8(plaintext).map_err(|_| EsiError::SecretDecryption)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets_round_trip_and_tampering_fails() {
        let cipher = SecretCipher::from_base64(&STANDARD.encode([7_u8; 32])).unwrap();
        let encrypted = cipher.encrypt("refresh-secret").unwrap();
        assert_eq!(cipher.decrypt(&encrypted).unwrap(), "refresh-secret");
        assert!(!format!("{encrypted:?}").contains("refresh-secret"));

        let mut tampered = encrypted;
        tampered.ciphertext.push('A');
        assert!(matches!(
            cipher.decrypt(&tampered),
            Err(EsiError::SecretDecryption)
        ));
    }
}
