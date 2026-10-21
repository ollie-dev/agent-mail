//! The Mandate store port.
//!
//! A store maps a token-derived lookup key to a sealed record. It never sees
//! a token or a Credential. The store a build uses is fixed at compile time:
//! nothing at runtime can select another one.

/// The store could not be read or written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreError;

/// Where sealed Mandates live.
pub trait MandateStore {
    /// Returns the sealed record filed under `key`, if any.
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, StoreError>;
    /// Files `value` under `key`. Add-only: fails if a record is already
    /// filed under `key`, and never replaces it.
    fn put(&self, key: &[u8], value: &[u8]) -> Result<(), StoreError>;
    /// Removes the record filed under `key`. Returns whether one existed.
    fn delete(&self, key: &[u8]) -> Result<bool, StoreError>;
}

/// The store a release build uses: the Keychain on macOS.
#[cfg(target_os = "macos")]
pub type RuntimeStore = KeychainStore;

/// The store a release build uses: none yet on this OS, so every token is
/// unknown and storing fails with `store_unavailable`.
#[cfg(not(target_os = "macos"))]
pub type RuntimeStore = EmptyStore;

/// A store that holds nothing and refuses writes. Used where no OS credential
/// store is supported yet.
#[derive(Debug, Default, Clone, Copy)]
pub struct EmptyStore;

impl MandateStore for EmptyStore {
    fn get(&self, _key: &[u8]) -> Result<Option<Vec<u8>>, StoreError> {
        Ok(None)
    }

    fn put(&self, _key: &[u8], _value: &[u8]) -> Result<(), StoreError> {
        Err(StoreError)
    }

    fn delete(&self, _key: &[u8]) -> Result<bool, StoreError> {
        Ok(false)
    }
}

#[cfg(target_os = "macos")]
pub use keychain::KeychainStore;

#[cfg(target_os = "macos")]
mod keychain {
    use base64::Engine;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use core_foundation::data::CFData;
    use security_framework::item::{ItemAddOptions, ItemAddValue, ItemClass};
    use security_framework::passwords;

    use super::{MandateStore, StoreError};

    /// Keychain service name of every Mandate item.
    pub const SERVICE: &str = "agent-mail";
    /// `errSecItemNotFound`.
    const ITEM_NOT_FOUND: i32 = -25300;

    /// The macOS Keychain: one generic-password item per Mandate, service
    /// `agent-mail`, item account the base64url-encoded store key. The item is
    /// created by this process through the Security framework, so its access
    /// list trusts only the agent-mail binary.
    #[derive(Debug, Default, Clone, Copy)]
    pub struct KeychainStore;

    fn item_account(key: &[u8]) -> String {
        URL_SAFE_NO_PAD.encode(key)
    }

    impl MandateStore for KeychainStore {
        fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, StoreError> {
            match passwords::get_generic_password(SERVICE, &item_account(key)) {
                Ok(value) => Ok(Some(value)),
                Err(e) if e.code() == ITEM_NOT_FOUND => Ok(None),
                Err(_) => Err(StoreError),
            }
        }

        /// Add-only (`SecItemAdd`): an existing item, and its access list,
        /// is never updated; `errSecDuplicateItem` is an error.
        fn put(&self, key: &[u8], value: &[u8]) -> Result<(), StoreError> {
            let mut options = ItemAddOptions::new(ItemAddValue::Data {
                class: ItemClass::generic_password(),
                data: CFData::from_buffer(value),
            });
            options
                .set_service(SERVICE)
                .set_account_name(item_account(key));
            options.add().map_err(|_| StoreError)
        }

        fn delete(&self, key: &[u8]) -> Result<bool, StoreError> {
            match passwords::delete_generic_password(SERVICE, &item_account(key)) {
                Ok(()) => Ok(true),
                Err(e) if e.code() == ITEM_NOT_FOUND => Ok(false),
                Err(_) => Err(StoreError),
            }
        }
    }
}

#[cfg(any(test, feature = "test-support"))]
pub use memory::InMemoryStore;

#[cfg(any(test, feature = "test-support"))]
mod memory {
    use std::collections::BTreeMap;
    use std::fmt;
    use std::sync::Mutex;

    use super::{MandateStore, StoreError};

    /// Marker that a release build must not contain (checked by a test).
    pub const MARKER: &str = "agent-mail-in-memory-store";

    /// Keeps the marker bytes in every binary this module is compiled into,
    /// whether or not the store is used, so the release check is meaningful.
    #[used]
    static MARKER_BYTES: [u8; MARKER.len()] = *b"agent-mail-in-memory-store";

    /// A test-only store holding raw records in memory.
    #[derive(Default)]
    pub struct InMemoryStore {
        entries: Mutex<BTreeMap<Vec<u8>, Vec<u8>>>,
    }

    impl InMemoryStore {
        /// An empty store.
        pub fn new() -> Self {
            Self::default()
        }

        /// Every raw key and value, for inspecting what the store holds.
        pub fn entries(&self) -> Vec<(Vec<u8>, Vec<u8>)> {
            self.entries
                .lock()
                .expect("store lock poisoned")
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        }
    }

    impl fmt::Debug for InMemoryStore {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{MARKER}({} entries)", self.entries().len())
        }
    }

    impl MandateStore for InMemoryStore {
        fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, StoreError> {
            Ok(self
                .entries
                .lock()
                .map_err(|_| StoreError)?
                .get(key)
                .cloned())
        }

        fn put(&self, key: &[u8], value: &[u8]) -> Result<(), StoreError> {
            let mut entries = self.entries.lock().map_err(|_| StoreError)?;
            if entries.contains_key(key) {
                return Err(StoreError);
            }
            entries.insert(key.to_vec(), value.to_vec());
            Ok(())
        }

        fn delete(&self, key: &[u8]) -> Result<bool, StoreError> {
            Ok(self
                .entries
                .lock()
                .map_err(|_| StoreError)?
                .remove(key)
                .is_some())
        }
    }
}
