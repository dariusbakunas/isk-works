//! `EncryptedSecret`: an encrypted token or verifier as stored at rest.
//! `iskworks-esi`'s `SecretCipher` produces and opens it; storage persists
//! it as JSON without depending on the cipher.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct EncryptedSecret {
    pub version: u8,
    pub key_id: String,
    pub nonce: String,
    pub ciphertext: String,
}
