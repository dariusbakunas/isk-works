//! Invite-only onboarding — pure code generation, normalization, hashing,
//! and the provisioning-time grant type. No I/O. See
//! `docs/security/invite-only-alpha.md`.
//!
//! An invite code is a high-entropy CSPRNG token, human-enterable as
//! `ISK-XXXX-XXXX-XXXX-XXXX`. It encodes nothing — no user id, workspace id,
//! or timestamp — it is just 80 bits of randomness. Only the SHA-256 of its
//! normalized form is ever stored; the raw code exists transiently in the
//! sign-in form, the HTTPS request body, and server memory while hashing,
//! and nowhere durable.

use sha2::{Digest, Sha256};
use uuid::Uuid;

use rand::{rngs::OsRng, RngCore};

/// Human-facing prefix. Purely cosmetic — stripped during normalization,
/// never part of the hashed payload.
pub const INVITE_CODE_PREFIX: &str = "ISK";

/// Crockford base32 alphabet: digits + uppercase letters with the visually
/// ambiguous `I`, `L`, `O`, `U` removed. Exactly 32 symbols, so a uniform
/// random byte maps to a symbol with `% 32` and no modulo bias.
const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// 16 symbols × 5 bits = 80 bits of entropy in the payload.
const PAYLOAD_SYMBOLS: usize = 16;

/// An `invite_codes` row id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InviteId(pub Uuid);

impl InviteId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for InviteId {
    fn default() -> Self {
        Self::new()
    }
}

/// Whether a login is allowed to provision a brand-new identity/workspace,
/// and if so under what invite. Returning users never reach a
/// `Required` decision — their identity already exists, so no invite is
/// consumed regardless of what was attached to the pending-auth row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InviteGrant {
    /// Invite mode is off (open registration), or this is a local dev-auth
    /// login. A new identity provisions with no invite.
    NotRequired,
    /// Invite mode is on. A new identity must atomically consume this
    /// pre-validated invite in the same transaction that provisions its
    /// workspace; if the invite can no longer be consumed (expired,
    /// disabled, or exhausted between login start and callback) the whole
    /// provisioning is rolled back and no workspace is created.
    Required(InviteId),
}

/// Generate a fresh, human-enterable invite code:
/// `ISK-XXXX-XXXX-XXXX-XXXX`. CSPRNG (`OsRng`), 80 bits of entropy, no
/// ambiguous characters, no embedded metadata.
#[must_use]
pub fn generate_invite_code() -> String {
    let mut bytes = [0_u8; PAYLOAD_SYMBOLS];
    OsRng.fill_bytes(&mut bytes);
    let payload: String = bytes
        .iter()
        .map(|byte| ALPHABET[(byte % 32) as usize] as char)
        .collect();

    let mut out = String::from(INVITE_CODE_PREFIX);
    for (index, symbol) in payload.chars().enumerate() {
        if index % 4 == 0 {
            out.push('-');
        }
        out.push(symbol);
    }
    out
}

/// Canonicalize user-entered input so equivalent codes hash identically:
/// uppercase, drop whitespace / dashes / a leading `ISK`, apply Crockford
/// input leniency (`O`→`0`, `I`/`L`→`1`), then keep only alphabet symbols.
/// Idempotent. Does not enforce length — an unrecognizable string simply
/// won't match any stored hash.
#[must_use]
pub fn normalize_invite_code(raw: &str) -> String {
    let upper = raw.trim().to_ascii_uppercase();
    let compact: String = upper
        .chars()
        .filter(|character| !character.is_whitespace() && *character != '-')
        .collect();
    let payload = compact.strip_prefix(INVITE_CODE_PREFIX).unwrap_or(&compact);

    payload
        .chars()
        .map(|character| match character {
            'O' => '0',
            'I' | 'L' => '1',
            other => other,
        })
        .filter(|character| ALPHABET.contains(&(*character as u8)))
        .collect()
}

/// SHA-256 (hex) of an already-normalized code. A fast cryptographic hash is
/// appropriate here: the codes are high-entropy random tokens, not
/// passwords, so there is no low-entropy space to brute-force.
#[must_use]
pub fn hash_invite_code(normalized: &str) -> String {
    format!("{:x}", Sha256::digest(normalized.as_bytes()))
}

/// Convenience: normalize then hash raw user input in one step. This is the
/// only value that should ever be compared against `invite_codes.code_hash`.
#[must_use]
pub fn hash_invite_code_from_raw(raw: &str) -> String {
    hash_invite_code(&normalize_invite_code(raw))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn generated_code_has_the_expected_shape() {
        let code = generate_invite_code();
        let groups: Vec<&str> = code.split('-').collect();
        assert_eq!(groups.len(), 5, "ISK + four groups: {code}");
        assert_eq!(groups[0], "ISK");
        for group in &groups[1..] {
            assert_eq!(group.len(), 4, "each group is four symbols: {code}");
            assert!(group.bytes().all(|byte| ALPHABET.contains(&byte)));
        }
    }

    #[test]
    fn generated_codes_do_not_repeat_and_carry_enough_entropy() {
        // 1000 draws from an 80-bit space: a collision would be
        // astronomically unlikely and points at a broken RNG.
        let mut seen = HashSet::new();
        for _ in 0..1000 {
            assert!(seen.insert(generate_invite_code()), "duplicate invite code");
        }
    }

    #[test]
    fn normalize_is_case_dash_and_prefix_insensitive() {
        let canonical = normalize_invite_code("ISK-7K3M-Q8PX-V2RT-W9NP");
        assert_eq!(canonical, "7K3MQ8PXV2RTW9NP");
        assert_eq!(
            normalize_invite_code("  isk-7k3m-q8px-v2rt-w9np  "),
            canonical
        );
        assert_eq!(normalize_invite_code("7k3m q8px v2rt w9np"), canonical);
        assert_eq!(normalize_invite_code("7K3MQ8PXV2RTW9NP"), canonical);
    }

    #[test]
    fn normalize_applies_crockford_input_leniency() {
        // A human who wrote O for 0 and I/L for 1 still resolves.
        assert_eq!(normalize_invite_code("ISK-OIL0-1111"), "01101111");
    }

    #[test]
    fn normalize_is_idempotent() {
        let once = normalize_invite_code("ISK-7K3M-Q8PX-V2RT-W9NP");
        assert_eq!(normalize_invite_code(&once), once);
    }

    #[test]
    fn a_generated_code_round_trips_through_normalize_and_hash() {
        let code = generate_invite_code();
        let direct = hash_invite_code_from_raw(&code);
        let lowered = hash_invite_code_from_raw(&code.to_ascii_lowercase());
        let spaced = hash_invite_code_from_raw(&code.replace('-', " "));
        assert_eq!(direct, lowered);
        assert_eq!(direct, spaced);
        assert_eq!(direct.len(), 64, "sha-256 hex");
    }

    #[test]
    fn different_codes_hash_differently() {
        assert_ne!(
            hash_invite_code_from_raw("ISK-AAAA-AAAA-AAAA-AAAA"),
            hash_invite_code_from_raw("ISK-AAAA-AAAA-AAAA-AAAB")
        );
    }
}
