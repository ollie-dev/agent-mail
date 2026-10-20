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
    /// Files `value` under `key`.
    fn put(&self, key: &[u8], value: &[u8]) -> Result<(), StoreError>;
}

/// The store a non-test build uses until the Keychain store arrives
/// (ticket 03). It holds no Mandates, so every token is unknown.
#[derive(Debug, Default, Clone, Copy)]
pub struct RuntimeStore;

impl MandateStore for RuntimeStore {
    fn get(&self, _key: &[u8]) -> Result<Option<Vec<u8>>, StoreError> {
        Ok(None)
    }

    fn put(&self, _key: &[u8], _value: &[u8]) -> Result<(), StoreError> {
        Err(StoreError)
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
            self.entries
                .lock()
                .map_err(|_| StoreError)?
                .insert(key.to_vec(), value.to_vec());
            Ok(())
        }
    }
}
