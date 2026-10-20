//! Agent Tokens and the sealed Mandate record (ADR-0003).
//!
//! Two keys are derived from an Agent Token with HKDF-SHA256 under distinct
//! labels: the store lookup key, and the ChaCha20-Poly1305 key that seals the
//! record. The record's plaintext holds the token hash, the Credential and the
//! Policy together, so none of them can be swapped independently.

use std::fmt;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ring::aead::{self, Aad, LessSafeKey, Nonce, UnboundKey};
use ring::digest;
use ring::hkdf;
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use crate::mandate::Mandate;

/// Prefix that makes Agent Tokens recognisable.
pub const TOKEN_PREFIX: &str = "amt_";
const TOKEN_BYTES: usize = 32;
const RECORD_VERSION: u8 = 1;
const HKDF_SALT: &[u8] = b"agent-mail/v1/token-salt";
const STORE_KEY_INFO: &[u8] = b"agent-mail/v1/store-key";
const SEAL_KEY_INFO: &[u8] = b"agent-mail/v1/seal-key";
const AAD_PREFIX: &[u8] = b"agent-mail/v1/record";

/// The token that unlocks exactly one Mandate. Its `Debug` output is redacted
/// and its memory is wiped when dropped.
#[derive(Clone, PartialEq, Eq)]
pub struct AgentToken {
    text: String,
    secret: [u8; TOKEN_BYTES],
}

/// A string that is not a well-formed Agent Token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MalformedToken;

/// A sealed record that cannot be opened with the presented token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenError;

impl AgentToken {
    /// Mints a fresh token: the prefix plus 256 random bits, base64url.
    pub fn mint() -> Self {
        let mut secret = Zeroizing::new([0u8; TOKEN_BYTES]);
        SystemRandom::new()
            .fill(secret.as_mut())
            .expect("system random number generator failed");
        Self {
            text: format!("{TOKEN_PREFIX}{}", URL_SAFE_NO_PAD.encode(*secret)),
            secret: *secret,
        }
    }

    /// Parses a presented token, rejecting anything not minted by [`Self::mint`].
    pub fn parse(text: &str) -> Result<Self, MalformedToken> {
        let body = text.strip_prefix(TOKEN_PREFIX).ok_or(MalformedToken)?;
        let bytes = Zeroizing::new(URL_SAFE_NO_PAD.decode(body).map_err(|_| MalformedToken)?);
        if bytes.len() != TOKEN_BYTES {
            return Err(MalformedToken);
        }
        let mut token = Self {
            text: text.to_string(),
            secret: [0u8; TOKEN_BYTES],
        };
        token.secret.copy_from_slice(&bytes);
        Ok(token)
    }

    /// The token as the Agent presents it. Handle with care.
    pub fn expose(&self) -> &str {
        &self.text
    }

    fn derive(&self, info: &[u8], out: &mut [u8]) {
        let prk = hkdf::Salt::new(hkdf::HKDF_SHA256, HKDF_SALT).extract(&self.secret);
        let infos = [info];
        prk.expand(&infos, OutputLen(out.len()))
            .and_then(|okm| okm.fill(out))
            .expect("HKDF output length is valid");
    }

    /// The key under which the store files this token's record.
    pub fn store_key(&self) -> Vec<u8> {
        let mut key = vec![0u8; 32];
        self.derive(STORE_KEY_INFO, &mut key);
        key
    }

    fn seal_key(&self) -> LessSafeKey {
        let mut key = Zeroizing::new([0u8; 32]);
        self.derive(SEAL_KEY_INFO, key.as_mut());
        let unbound = UnboundKey::new(&aead::CHACHA20_POLY1305, key.as_ref())
            .expect("key length matches the AEAD");
        LessSafeKey::new(unbound)
    }

    fn hash(&self) -> String {
        URL_SAFE_NO_PAD.encode(digest::digest(&digest::SHA256, &self.secret))
    }
}

impl Zeroize for AgentToken {
    fn zeroize(&mut self) {
        self.text.zeroize();
        self.secret.zeroize();
    }
}

impl Drop for AgentToken {
    fn drop(&mut self) {
        self.zeroize();
    }
}

impl ZeroizeOnDrop for AgentToken {}

impl fmt::Debug for AgentToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AgentToken(<redacted>)")
    }
}

struct OutputLen(usize);

impl hkdf::KeyType for OutputLen {
    fn len(&self) -> usize {
        self.0
    }
}

/// A fresh short Mandate ID: 8 random lowercase hex digits.
pub fn short_id() -> String {
    let mut bytes = [0u8; 4];
    SystemRandom::new()
        .fill(&mut bytes)
        .expect("system random number generator failed");
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// A sealed Mandate as the store holds it.
#[derive(Clone, PartialEq, Eq)]
pub struct SealedRecord {
    /// Lookup key, derived from the token.
    pub key: Vec<u8>,
    /// Version byte, nonce, then ciphertext with tag.
    pub value: Vec<u8>,
}

impl fmt::Debug for SealedRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SealedRecord")
            .field("key_len", &self.key.len())
            .field("value_len", &self.value.len())
            .finish()
    }
}

#[derive(Serialize, Deserialize)]
struct Plaintext {
    token_hash: String,
    mandate: Mandate,
}

fn aad(store_key: &[u8]) -> Vec<u8> {
    [AAD_PREFIX, store_key].concat()
}

/// Seals `mandate` under `token`.
pub fn seal(token: &AgentToken, mandate: &Mandate) -> SealedRecord {
    let key = token.store_key();
    let plaintext = Plaintext {
        token_hash: token.hash(),
        mandate: mandate.clone(),
    };
    // Size the buffer up front so the plaintext is never reallocated, which
    // would leave unwiped copies behind; the buffer is wiped on drop.
    let mut counter = ByteCounter(0);
    serde_json::to_writer(&mut counter, &plaintext).expect("a Mandate always serialises");
    let mut buf = Zeroizing::new(Vec::with_capacity(
        counter.0 + aead::CHACHA20_POLY1305.tag_len(),
    ));
    serde_json::to_writer(&mut *buf, &plaintext).expect("a Mandate always serialises");
    let mut nonce = [0u8; aead::NONCE_LEN];
    SystemRandom::new()
        .fill(&mut nonce)
        .expect("system random number generator failed");
    token
        .seal_key()
        .seal_in_place_append_tag(
            Nonce::assume_unique_for_key(nonce),
            Aad::from(aad(&key)),
            &mut *buf,
        )
        .expect("record fits the AEAD limits");
    let mut value = Vec::with_capacity(1 + nonce.len() + buf.len());
    value.push(RECORD_VERSION);
    value.extend_from_slice(&nonce);
    value.extend_from_slice(&buf);
    SealedRecord { key, value }
}

/// Counts serialised bytes without storing them.
struct ByteCounter(usize);

impl std::io::Write for ByteCounter {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        self.0 += data.len();
        Ok(data.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Opens a sealed record with `token`. Fails on any tampering or wrong token.
pub fn open(token: &AgentToken, value: &[u8]) -> Result<Mandate, OpenError> {
    let (&version, rest) = value.split_first().ok_or(OpenError)?;
    if version != RECORD_VERSION || rest.len() < aead::NONCE_LEN {
        return Err(OpenError);
    }
    let (nonce, ciphertext) = rest.split_at(aead::NONCE_LEN);
    let nonce = Nonce::try_assume_unique_for_key(nonce).map_err(|_| OpenError)?;
    let mut buf = Zeroizing::new(ciphertext.to_vec());
    let plaintext = token
        .seal_key()
        .open_in_place(nonce, Aad::from(aad(&token.store_key())), &mut buf)
        .map_err(|_| OpenError)?;
    let plaintext: Plaintext = serde_json::from_slice(plaintext).map_err(|_| OpenError)?;
    if plaintext.token_hash != token.hash() {
        return Err(OpenError);
    }
    Ok(plaintext.mandate)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wiped_on_drop<T: ZeroizeOnDrop>() {}

    #[test]
    fn token_is_wiped_on_drop_and_zeroizes() {
        wiped_on_drop::<AgentToken>();
        let mut token = AgentToken::mint();
        token.zeroize();
        assert_eq!(token.expose(), "");
        assert_eq!(token.secret, [0u8; TOKEN_BYTES]);
    }
}
