//! The Mandate store port.
//!
//! A store maps a token-derived lookup key to a sealed record. It never sees
//! a token or a Credential. The store a build uses is fixed at compile time:
//! nothing at runtime can select another one.

/// Why a store call failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreError {
    /// The store could not be read or written.
    Unavailable,
    /// The binary is not signed with the hardened runtime, so the store was
    /// not accessed at all (see [`crate::selfcheck`]).
    HardenedRuntimeMissing,
}

/// One stored record: its key and its value.
pub type Entry = (Vec<u8>, Vec<u8>);

/// Where sealed Mandates live.
pub trait MandateStore {
    /// Returns the sealed record filed under `key`, if any.
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, StoreError>;
    /// Files `value` under `key`. Add-only: fails if a record is already
    /// filed under `key`, and never replaces it.
    fn put(&self, key: &[u8], value: &[u8]) -> Result<(), StoreError>;
    /// Removes the record filed under `key`. Returns whether one existed.
    fn delete(&self, key: &[u8]) -> Result<bool, StoreError>;
    /// Every key the store holds, each with its value, in no set order.
    fn entries(&self) -> Result<Vec<Entry>, StoreError>;
    /// Whether the store may be accessed at all, without accessing it.
    /// Passes unless the store is a [`GuardedStore`] whose check fails.
    fn check(&self) -> Result<(), StoreError> {
        Ok(())
    }
}

/// The hardened-runtime gate a [`GuardedStore`] consults.
#[derive(Debug, Clone, Copy)]
enum Gate {
    /// This process's own check ([`crate::selfcheck::check`]).
    Process,
    /// A fixed code-signing status. Test builds only.
    #[cfg(any(test, feature = "test-support"))]
    Fixed(Option<u32>),
}

/// Runs the hardened-runtime self-check before every call to the inner
/// store, and never forwards a call when it fails: the call returns
/// [`StoreError::HardenedRuntimeMissing`] instead.
#[derive(Debug)]
pub struct GuardedStore<S: MandateStore> {
    inner: S,
    gate: Gate,
}

impl<S: MandateStore> GuardedStore<S> {
    /// Guards `inner` with this process's self-check.
    pub fn new(inner: S) -> Self {
        Self {
            inner,
            gate: Gate::Process,
        }
    }

    /// Guards `inner` with a fixed code-signing `status`, judged by
    /// [`crate::selfcheck::hardened_runtime_ok`]. Test builds only.
    #[cfg(any(test, feature = "test-support"))]
    pub fn with_status(inner: S, status: Option<u32>) -> Self {
        Self {
            inner,
            gate: Gate::Fixed(status),
        }
    }

    /// The guarded store, for inspecting it. Test builds only.
    #[cfg(any(test, feature = "test-support"))]
    pub fn inner(&self) -> &S {
        &self.inner
    }

    /// Passes if the self-check passes; touches no store.
    pub fn check(&self) -> Result<(), StoreError> {
        let ok = match self.gate {
            Gate::Process => crate::selfcheck::check(),
            #[cfg(any(test, feature = "test-support"))]
            Gate::Fixed(status) => crate::selfcheck::hardened_runtime_ok(status),
        };
        if ok {
            Ok(())
        } else {
            Err(StoreError::HardenedRuntimeMissing)
        }
    }
}

impl<S: MandateStore> MandateStore for GuardedStore<S> {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, StoreError> {
        GuardedStore::check(self)?;
        self.inner.get(key)
    }

    fn put(&self, key: &[u8], value: &[u8]) -> Result<(), StoreError> {
        GuardedStore::check(self)?;
        self.inner.put(key, value)
    }

    fn delete(&self, key: &[u8]) -> Result<bool, StoreError> {
        GuardedStore::check(self)?;
        self.inner.delete(key)
    }

    fn entries(&self) -> Result<Vec<Entry>, StoreError> {
        GuardedStore::check(self)?;
        self.inner.entries()
    }

    fn check(&self) -> Result<(), StoreError> {
        GuardedStore::check(self)?;
        self.inner.check()
    }
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
        Err(StoreError::Unavailable)
    }

    fn delete(&self, _key: &[u8]) -> Result<bool, StoreError> {
        Ok(false)
    }

    fn entries(&self) -> Result<Vec<Entry>, StoreError> {
        Ok(Vec::new())
    }
}

#[cfg(target_os = "macos")]
pub use keychain::KeychainStore;

#[cfg(target_os = "macos")]
mod keychain {
    use base64::Engine;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use core_foundation::data::CFData;
    use security_framework::item::{
        ItemAddOptions, ItemAddValue, ItemClass, ItemSearchOptions, Limit, SearchResult,
    };
    use security_framework::passwords;

    use super::{Entry, MandateStore, StoreError};

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

    /// An item's store key (if its account decodes) and its raw account.
    type ItemAccount = (Option<Vec<u8>>, String);

    impl KeychainStore {
        /// The store key of every item with service `agent-mail`, found by
        /// attributes only (`SecItemCopyMatching`, `kSecMatchLimitAll`). Reads
        /// no value, so it never triggers a Keychain access prompt.
        pub fn keys(&self) -> Result<Vec<Vec<u8>>, StoreError> {
            Ok(self
                .accounts()?
                .into_iter()
                .filter_map(|(key, _)| key)
                .collect())
        }

        /// Each item's raw item account, with its store key if the account
        /// is valid base64url.
        fn accounts(&self) -> Result<Vec<ItemAccount>, StoreError> {
            let found = ItemSearchOptions::new()
                .class(ItemClass::generic_password())
                .service(SERVICE)
                .load_attributes(true)
                .limit(Limit::All)
                .search();
            let found = match found {
                Ok(found) => found,
                Err(e) if e.code() == ITEM_NOT_FOUND => return Ok(Vec::new()),
                Err(_) => return Err(StoreError::Unavailable),
            };
            found
                .iter()
                .map(|item| {
                    let account = match item {
                        SearchResult::Dict(_) => item
                            .simplify_dict()
                            .and_then(|attributes| attributes.get("acct").cloned()),
                        _ => None,
                    }
                    .ok_or(StoreError::Unavailable)?;
                    let key = URL_SAFE_NO_PAD.decode(&account).ok();
                    Ok((key, account))
                })
                .collect()
        }
    }

    impl MandateStore for KeychainStore {
        fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, StoreError> {
            match passwords::get_generic_password(SERVICE, &item_account(key)) {
                Ok(value) => Ok(Some(value)),
                Err(e) if e.code() == ITEM_NOT_FOUND => Ok(None),
                Err(_) => Err(StoreError::Unavailable),
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
            options.add().map_err(|_| StoreError::Unavailable)
        }

        fn delete(&self, key: &[u8]) -> Result<bool, StoreError> {
            match passwords::delete_generic_password(SERVICE, &item_account(key)) {
                Ok(()) => Ok(true),
                Err(e) if e.code() == ITEM_NOT_FOUND => Ok(false),
                Err(_) => Err(StoreError::Unavailable),
            }
        }

        /// Reads every value found by [`KeychainStore::keys`] one by one,
        /// since fetching data with `kSecMatchLimitAll` is refused. An item
        /// whose value cannot be read (e.g. access denied) is returned with
        /// an empty value, so it is counted as unreadable, not dropped.
        fn entries(&self) -> Result<Vec<Entry>, StoreError> {
            let mut entries = Vec::new();
            for (key, account) in self.accounts()? {
                let Some(key) = key else {
                    // Not filed by agent-mail: count it, never target it.
                    entries.push((account.into_bytes(), Vec::new()));
                    continue;
                };
                match passwords::get_generic_password(SERVICE, &account) {
                    Ok(value) => entries.push((key, value)),
                    // Deleted since the search: no longer held.
                    Err(e) if e.code() == ITEM_NOT_FOUND => {}
                    Err(_) => entries.push((key, Vec::new())),
                }
            }
            Ok(entries)
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

    use super::{Entry, MandateStore, StoreError};

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
        pub fn entries(&self) -> Vec<Entry> {
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
                .map_err(|_| StoreError::Unavailable)?
                .get(key)
                .cloned())
        }

        fn put(&self, key: &[u8], value: &[u8]) -> Result<(), StoreError> {
            let mut entries = self.entries.lock().map_err(|_| StoreError::Unavailable)?;
            if entries.contains_key(key) {
                return Err(StoreError::Unavailable);
            }
            entries.insert(key.to_vec(), value.to_vec());
            Ok(())
        }

        fn delete(&self, key: &[u8]) -> Result<bool, StoreError> {
            Ok(self
                .entries
                .lock()
                .map_err(|_| StoreError::Unavailable)?
                .remove(key)
                .is_some())
        }

        fn entries(&self) -> Result<Vec<Entry>, StoreError> {
            Ok(InMemoryStore::entries(self))
        }
    }
}
