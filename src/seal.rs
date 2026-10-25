//! Agent Tokens and the sealed Mandate record (ADR-0003).
//!
//! Two keys are derived from an Agent Token with HKDF-SHA256 under distinct
//! labels: the store lookup key, and the ChaCha20-Poly1305 key that seals the
//! record. The record's plaintext holds the token hash, the Credential and the
//! Policy together, so none of them can be swapped independently.
//!
//! A stored value (format version 2) is laid out as
//! `version (1) | listing length (4, big-endian) | listing | nonce (12) |
//! ciphertext and tag`. The listing is the Mandate's listable metadata in
//! clear JSON (never a Credential, token or token hash), readable without a
//! token by [`listing`]. It is bound to the sealed part as AEAD associated
//! data together with the version and the store key, so changing any byte of
//! it makes [`open`] fail.

use std::fmt;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ring::aead::{self, Aad, LessSafeKey, Nonce, UnboundKey};
use ring::digest;
use ring::hkdf;
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use crate::mandate::{Grant, Mandate, TlsMode};

/// Prefix that makes Agent Tokens recognisable.
pub const TOKEN_PREFIX: &str = "amt_";
const TOKEN_BYTES: usize = 32;
/// Version byte of the current record format.
pub const RECORD_VERSION: u8 = 2;
const LISTING_LEN_BYTES: usize = 4;
const HKDF_SALT: &[u8] = b"agent-mail/v1/token-salt";
const STORE_KEY_INFO: &[u8] = b"agent-mail/v1/store-key";
const SEAL_KEY_INFO: &[u8] = b"agent-mail/v1/seal-key";
const AAD_PREFIX: &[u8] = b"agent-mail/v2/record";

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
    /// The stored value: version, listing, then the sealed part (see the
    /// module documentation).
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

/// A Mandate's listable metadata, stored in clear next to the sealed part.
/// Holds no Credential, token or token hash. It is shown without a token, so
/// it is trusted only once [`open`] has verified the record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Listing {
    /// Short ID.
    pub id: String,
    /// Name the human gave the Mandate.
    pub name: String,
    /// Creation time, Unix seconds (UTC).
    pub created: u64,
    /// The covered Accounts, without their Credentials.
    pub accounts: Vec<ListedAccount>,
    /// The Policy's Grants.
    pub grants: Vec<Grant>,
    /// The SID of the Windows user who created the Mandate through the
    /// Broker (ADR-0004), which serves it to that user only. `None` for
    /// records written in-process, and then left out of the JSON, so those
    /// records keep their bytes. Bound to the record by the AEAD like every
    /// other listing field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
}

/// An Account as listed: everything but its Credential.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListedAccount {
    /// IMAP login name.
    pub login: String,
    /// IMAP server host name.
    pub host: String,
    /// IMAP server port.
    pub port: u16,
    /// How the connection is secured.
    pub tls: TlsMode,
}

impl Listing {
    /// The listable metadata of `mandate`.
    pub fn of(mandate: &Mandate) -> Self {
        Self {
            id: mandate.id.clone(),
            name: mandate.name.clone(),
            created: mandate.created,
            accounts: mandate
                .accounts
                .iter()
                .map(|a| ListedAccount {
                    login: a.login.clone(),
                    host: a.host.clone(),
                    port: a.port,
                    tls: a.tls,
                })
                .collect(),
            grants: mandate.policy.grants.clone(),
            owner: None,
        }
    }
}

/// A stored value whose listable metadata cannot be read: an old record
/// format, or bytes that are not a record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnreadableRecord;

/// The two parts of a stored value: the clear listing bytes and the sealed
/// part (nonce, ciphertext and tag).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordParts<'a> {
    /// The listing, as clear JSON bytes.
    pub listing: &'a [u8],
    /// Nonce, then ciphertext with tag.
    pub sealed: &'a [u8],
}

/// Splits a stored value into its parts without a token.
pub fn parts(value: &[u8]) -> Result<RecordParts<'_>, UnreadableRecord> {
    let (&version, rest) = value.split_first().ok_or(UnreadableRecord)?;
    if version != RECORD_VERSION || rest.len() < LISTING_LEN_BYTES {
        return Err(UnreadableRecord);
    }
    let (len, rest) = rest.split_at(LISTING_LEN_BYTES);
    let len = u32::from_be_bytes(len.try_into().expect("four bytes")) as usize;
    if rest.len() < len {
        return Err(UnreadableRecord);
    }
    let (listing, sealed) = rest.split_at(len);
    if sealed.len() < aead::NONCE_LEN + aead::CHACHA20_POLY1305.tag_len() {
        return Err(UnreadableRecord);
    }
    Ok(RecordParts { listing, sealed })
}

/// Joins a listing and a sealed part into a stored value. The inverse of
/// [`parts`]; whether the result opens is up to [`open`].
pub fn assemble(listing: &[u8], sealed: &[u8]) -> Vec<u8> {
    let len = u32::try_from(listing.len()).expect("listing fits a record");
    let mut value = Vec::with_capacity(1 + LISTING_LEN_BYTES + listing.len() + sealed.len());
    value.push(RECORD_VERSION);
    value.extend_from_slice(&len.to_be_bytes());
    value.extend_from_slice(listing);
    value.extend_from_slice(sealed);
    value
}

/// Reads the listable metadata of a stored value, without a token. Not
/// authenticated: only [`open`] proves it matches the sealed Mandate.
pub fn listing(value: &[u8]) -> Result<Listing, UnreadableRecord> {
    let parts = parts(value)?;
    serde_json::from_slice(parts.listing).map_err(|_| UnreadableRecord)
}

fn aad(store_key: &[u8], listing: &[u8]) -> Vec<u8> {
    let len = u32::try_from(listing.len()).expect("listing fits a record");
    [
        AAD_PREFIX,
        &[RECORD_VERSION],
        store_key,
        &len.to_be_bytes(),
        listing,
    ]
    .concat()
}

/// Seals `mandate` under `token`, with no owner.
pub fn seal(token: &AgentToken, mandate: &Mandate) -> SealedRecord {
    seal_for(token, mandate, None)
}

/// Seals `mandate` under `token`, recording `owner` (a SID string) in the
/// authenticated listing.
pub fn seal_for(token: &AgentToken, mandate: &Mandate, owner: Option<&str>) -> SealedRecord {
    let key = token.store_key();
    let listing = Listing {
        owner: owner.map(str::to_string),
        ..Listing::of(mandate)
    };
    let listing = serde_json::to_vec(&listing).expect("a listing always serialises");
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
            Aad::from(aad(&key, &listing)),
            &mut *buf,
        )
        .expect("record fits the AEAD limits");
    let sealed = [&nonce[..], &buf[..]].concat();
    let value = assemble(&listing, &sealed);
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
/// The clear listing must be byte-for-byte authentic, and must also match the
/// Mandate sealed inside.
pub fn open(token: &AgentToken, value: &[u8]) -> Result<Mandate, OpenError> {
    open_with_owner(token, value).map(|(mandate, _)| mandate)
}

/// Like [`open`], also returning the owner recorded in the authenticated
/// listing.
pub fn open_with_owner(
    token: &AgentToken,
    value: &[u8],
) -> Result<(Mandate, Option<String>), OpenError> {
    let parts = parts(value).map_err(|_| OpenError)?;
    let (nonce, ciphertext) = parts.sealed.split_at(aead::NONCE_LEN);
    let nonce = Nonce::try_assume_unique_for_key(nonce).map_err(|_| OpenError)?;
    let mut buf = Zeroizing::new(ciphertext.to_vec());
    let plaintext = token
        .seal_key()
        .open_in_place(
            nonce,
            Aad::from(aad(&token.store_key(), parts.listing)),
            &mut buf,
        )
        .map_err(|_| OpenError)?;
    let plaintext: Plaintext = serde_json::from_slice(plaintext).map_err(|_| OpenError)?;
    if plaintext.token_hash != token.hash() {
        return Err(OpenError);
    }
    let mut listed: Listing = serde_json::from_slice(parts.listing).map_err(|_| OpenError)?;
    // The owner lives only in the listing, which the AEAD authenticates.
    let owner = listed.owner.take();
    if listed != Listing::of(&plaintext.mandate) {
        return Err(OpenError);
    }
    Ok((plaintext.mandate, owner))
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
