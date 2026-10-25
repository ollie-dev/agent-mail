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
    /// Credential Manager has no logon session to work in
    /// (`ERROR_NO_SUCH_LOGON_SESSION`), e.g. when called while impersonating.
    NoLogonSession,
}

/// `ERROR_NO_SUCH_LOGON_SESSION`.
pub const ERROR_NO_SUCH_LOGON_SESSION: u32 = 1312;

/// The store error for the Win32 error `code` of a failed Credential
/// Manager call. Pure, so it is tested on every OS.
pub fn from_win32(code: u32) -> StoreError {
    match code {
        ERROR_NO_SUCH_LOGON_SESSION => StoreError::NoLogonSession,
        _ => StoreError::Unavailable,
    }
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

/// The store a release build uses: Credential Manager on Windows.
#[cfg(windows)]
pub type RuntimeStore = CredentialManagerStore;

/// The store a release build uses: none yet on this OS, so every token is
/// unknown and storing fails with `store_unavailable`.
#[cfg(not(any(target_os = "macos", windows)))]
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

/// How the Windows Credential Manager store lays out a record. Pure
/// functions, compiled on every OS so they are tested everywhere.
///
/// One generic credential per record, target name `agent-mail:` followed by
/// the base64url (no padding) store key. A credential blob holds at most
/// [`BLOB_LIMIT`] bytes, so a longer value is split: the first part under
/// `agent-mail:<key>`, later parts under `agent-mail:<key>#1`,
/// `agent-mail:<key>#2`, and so on. The first part starts with the number of
/// parts as a big-endian `u16`.
pub mod credential_layout {
    use base64::Engine;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;

    use super::StoreError;

    /// Every agent-mail target name starts with this.
    pub const TARGET_PREFIX: &str = "agent-mail:";
    /// Credential Manager's blob limit (`CRED_MAX_CREDENTIAL_BLOB_SIZE`).
    pub const BLOB_LIMIT: usize = 2560;
    /// Bytes at the start of the first part holding the part count.
    pub const HEADER_LEN: usize = 2;

    /// The target name of a record's first part.
    pub fn target_name(key: &[u8]) -> String {
        format!("{TARGET_PREFIX}{}", URL_SAFE_NO_PAD.encode(key))
    }

    /// The target name of part `index` (0 is the first part).
    pub fn part_target(key: &[u8], index: usize) -> String {
        let first = target_name(key);
        if index == 0 {
            first
        } else {
            format!("{first}#{index}")
        }
    }

    /// The target names of a record of `parts` parts, in order.
    pub fn target_names(key: &[u8], parts: usize) -> Vec<String> {
        (0..parts).map(|i| part_target(key, i)).collect()
    }

    /// Splits `value` into blobs of at most [`BLOB_LIMIT`] bytes, the first
    /// starting with the part count. Fails if the value needs more parts
    /// than the count can record.
    pub fn split(value: &[u8]) -> Result<Vec<Vec<u8>>, StoreError> {
        let first_len = value.len().min(BLOB_LIMIT - HEADER_LEN);
        let (head, rest) = value.split_at(first_len);
        let count = 1 + rest.len().div_ceil(BLOB_LIMIT);
        let count = u16::try_from(count).map_err(|_| StoreError::Unavailable)?;
        let mut first = count.to_be_bytes().to_vec();
        first.extend_from_slice(head);
        let mut parts = vec![first];
        parts.extend(rest.chunks(BLOB_LIMIT).map(<[u8]>::to_vec));
        Ok(parts)
    }

    /// The part count recorded in a first part.
    pub fn part_count(first: &[u8]) -> Result<usize, StoreError> {
        match first {
            [high, low, ..] if first.len() <= BLOB_LIMIT => {
                match usize::from(u16::from_be_bytes([*high, *low])) {
                    0 => Err(StoreError::Unavailable),
                    count => Ok(count),
                }
            }
            _ => Err(StoreError::Unavailable),
        }
    }

    /// Joins the parts [`split`] made back into the value. Fails if a part
    /// is missing, extra or too long.
    pub fn join(parts: &[Vec<u8>]) -> Result<Vec<u8>, StoreError> {
        let first = parts.first().ok_or(StoreError::Unavailable)?;
        if part_count(first)? != parts.len() || parts.iter().any(|p| p.len() > BLOB_LIMIT) {
            return Err(StoreError::Unavailable);
        }
        let mut value = first[HEADER_LEN..].to_vec();
        for part in &parts[1..] {
            value.extend_from_slice(part);
        }
        Ok(value)
    }

    /// What a target name under [`TARGET_PREFIX`] is.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum Target {
        /// The first part of the record filed under this store key.
        Record(Vec<u8>),
        /// A later part of a record.
        Part,
        /// A name that does not decode: not filed by agent-mail.
        Foreign,
    }

    /// Classifies a target name; `None` if it is not under the prefix.
    pub fn classify(name: &str) -> Option<Target> {
        let rest = name.strip_prefix(TARGET_PREFIX)?;
        let decodes = |encoded: &str| URL_SAFE_NO_PAD.decode(encoded).ok();
        Some(match rest.split_once('#') {
            None => decodes(rest).map_or(Target::Foreign, Target::Record),
            Some((encoded, index)) => {
                let index_ok = !index.starts_with('0')
                    && !index.is_empty()
                    && index.bytes().all(|b| b.is_ascii_digit());
                if index_ok && decodes(encoded).is_some() {
                    Target::Part
                } else {
                    Target::Foreign
                }
            }
        })
    }
}

#[cfg(windows)]
pub use credential_manager::CredentialManagerStore;

#[cfg(windows)]
mod credential_manager {
    use std::ptr;

    use windows_sys::Win32::Foundation::{ERROR_NOT_FOUND, GetLastError};
    use windows_sys::Win32::Security::Credentials::{
        CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW, CredDeleteW, CredEnumerateW,
        CredFree, CredReadW, CredWriteW,
    };

    use super::credential_layout::{
        TARGET_PREFIX, Target, classify, join, part_count, part_target, split, target_name,
        target_names,
    };
    use super::{Entry, MandateStore, StoreError, from_win32};

    /// Windows Credential Manager: one generic credential per Mandate,
    /// persisted `CRED_PERSIST_LOCAL_MACHINE`, laid out as
    /// [`super::credential_layout`] describes. The credentials belong to the
    /// account the process runs as: in the Broker (ADR-0004) that is the
    /// service account `NT SERVICEgent-mail`, out of reach of the user's
    /// own processes; in-process, any program of the same user can read them.
    #[derive(Debug, Default, Clone, Copy)]
    pub struct CredentialManagerStore;

    /// `s` as a NUL-terminated UTF-16 string.
    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// The NUL-terminated UTF-16 string at `p`.
    ///
    /// # Safety
    /// `p` is null or points to a NUL-terminated UTF-16 string.
    unsafe fn from_wide(p: *const u16) -> String {
        if p.is_null() {
            return String::new();
        }
        let mut len = 0;
        // SAFETY: the string is NUL-terminated, so every read up to and
        // including the NUL is in bounds.
        while unsafe { *p.add(len) } != 0 {
            len += 1;
        }
        // SAFETY: `len` units were just read.
        String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(p, len) })
    }

    /// The blob of the generic credential `target`, if it exists.
    fn read(target: &str) -> Result<Option<Vec<u8>>, StoreError> {
        let name = wide(target);
        let mut credential: *mut CREDENTIALW = ptr::null_mut();
        // SAFETY: `name` is NUL-terminated; `credential` receives a buffer
        // freed below with CredFree.
        let ok = unsafe { CredReadW(name.as_ptr(), CRED_TYPE_GENERIC, 0, &mut credential) };
        if ok == 0 {
            // SAFETY: no preconditions.
            return match unsafe { GetLastError() } {
                ERROR_NOT_FOUND => Ok(None),
                code => Err(from_win32(code)),
            };
        }
        // SAFETY: on success `credential` points to a valid CREDENTIALW whose
        // blob holds `CredentialBlobSize` bytes.
        let blob = unsafe {
            let c = &*credential;
            if c.CredentialBlob.is_null() || c.CredentialBlobSize == 0 {
                Vec::new()
            } else {
                std::slice::from_raw_parts(c.CredentialBlob, c.CredentialBlobSize as usize).to_vec()
            }
        };
        // SAFETY: the buffer came from CredReadW and is freed once.
        unsafe { CredFree(credential.cast()) };
        Ok(Some(blob))
    }

    /// Writes the generic credential `target`. Callers check first that it
    /// does not exist: CredWriteW itself would replace it.
    fn write(target: &str, blob: &[u8]) -> Result<(), StoreError> {
        let mut name = wide(target);
        let size = u32::try_from(blob.len()).map_err(|_| StoreError::Unavailable)?;
        let credential = CREDENTIALW {
            Type: CRED_TYPE_GENERIC,
            TargetName: name.as_mut_ptr(),
            CredentialBlobSize: size,
            // CredWriteW only reads the blob.
            CredentialBlob: blob.as_ptr().cast_mut(),
            Persist: CRED_PERSIST_LOCAL_MACHINE,
            ..Default::default()
        };
        // SAFETY: every pointer in `credential` is valid for the call.
        if unsafe { CredWriteW(&credential, 0) } == 0 {
            // SAFETY: no preconditions.
            Err(from_win32(unsafe { GetLastError() }))
        } else {
            Ok(())
        }
    }

    /// Deletes the generic credential `target`. Returns whether it existed.
    fn remove(target: &str) -> Result<bool, StoreError> {
        let name = wide(target);
        // SAFETY: `name` is NUL-terminated.
        if unsafe { CredDeleteW(name.as_ptr(), CRED_TYPE_GENERIC, 0) } != 0 {
            return Ok(true);
        }
        // SAFETY: no preconditions.
        match unsafe { GetLastError() } {
            ERROR_NOT_FOUND => Ok(false),
            code => Err(from_win32(code)),
        }
    }

    /// The target name of every generic credential under `agent-mail:`.
    /// Reads no blob.
    fn target_names_under_prefix() -> Result<Vec<String>, StoreError> {
        let filter = wide(&format!("{TARGET_PREFIX}*"));
        let mut count = 0u32;
        let mut credentials: *mut *mut CREDENTIALW = ptr::null_mut();
        // SAFETY: `filter` is NUL-terminated; `credentials` receives an
        // array of `count` pointers freed below with CredFree.
        let ok = unsafe { CredEnumerateW(filter.as_ptr(), 0, &mut count, &mut credentials) };
        if ok == 0 {
            // SAFETY: no preconditions.
            return match unsafe { GetLastError() } {
                ERROR_NOT_FOUND => Ok(Vec::new()),
                code => Err(from_win32(code)),
            };
        }
        let mut names = Vec::new();
        for i in 0..count as usize {
            // SAFETY: the array holds `count` valid CREDENTIALW pointers.
            let credential = unsafe { &**credentials.add(i) };
            if credential.Type == CRED_TYPE_GENERIC {
                // SAFETY: TargetName is a NUL-terminated string.
                names.push(unsafe { from_wide(credential.TargetName) });
            }
        }
        // SAFETY: the array came from CredEnumerateW and is freed once.
        unsafe { CredFree(credentials.cast()) };
        Ok(names)
    }

    impl CredentialManagerStore {
        /// The target name of every generic credential whose name starts
        /// with `agent-mail:` (`CredEnumerateW`), including later parts and
        /// names that do not decode. Reads no value.
        pub fn target_names(&self) -> Result<Vec<String>, StoreError> {
            target_names_under_prefix()
        }
    }

    impl MandateStore for CredentialManagerStore {
        /// Reads the first part, then each later part it records. A missing
        /// part is an error.
        fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, StoreError> {
            let Some(first) = read(&target_name(key))? else {
                return Ok(None);
            };
            let count = part_count(&first)?;
            let mut parts = vec![first];
            for index in 1..count {
                parts.push(read(&part_target(key, index))?.ok_or(StoreError::Unavailable)?);
            }
            join(&parts).map(Some)
        }

        /// Add-only: fails without writing if any of the record's target
        /// names already exists, so no item is ever replaced. Later parts are
        /// written first and the first part last, so a record is only found
        /// once complete; on failure the parts written are removed.
        fn put(&self, key: &[u8], value: &[u8]) -> Result<(), StoreError> {
            let parts = split(value)?;
            let names = target_names(key, parts.len());
            for name in &names {
                if read(name)?.is_some() {
                    return Err(StoreError::Unavailable);
                }
            }
            let mut written: Vec<&str> = Vec::new();
            let order = (1..parts.len()).chain(std::iter::once(0));
            for index in order {
                if let Err(e) = write(&names[index], &parts[index]) {
                    for name in written {
                        let _ = remove(name);
                    }
                    return Err(e);
                }
                written.push(&names[index]);
            }
            Ok(())
        }

        /// Removes the later parts, then the first part.
        fn delete(&self, key: &[u8]) -> Result<bool, StoreError> {
            let Some(first) = read(&target_name(key))? else {
                return Ok(false);
            };
            // A first part whose count is unreadable has no parts we know of.
            let count = part_count(&first).unwrap_or(1);
            for index in (1..count).rev() {
                remove(&part_target(key, index))?;
            }
            remove(&target_name(key))
        }

        /// Every record under `agent-mail:`. A record that cannot be read
        /// (e.g. a part is missing) is returned with an empty value, and a
        /// name that does not decode is returned as its raw bytes with an
        /// empty value, so both are counted as unreadable, never dropped
        /// and never targeted. Later parts are not records.
        fn entries(&self) -> Result<Vec<Entry>, StoreError> {
            let mut entries = Vec::new();
            for name in target_names_under_prefix()? {
                match classify(&name) {
                    Some(Target::Record(key)) => match self.get(&key) {
                        Ok(Some(value)) => entries.push((key, value)),
                        // Deleted since the enumeration: no longer held.
                        Ok(None) => {}
                        Err(_) => entries.push((key, Vec::new())),
                    },
                    Some(Target::Foreign) => entries.push((name.into_bytes(), Vec::new())),
                    Some(Target::Part) | None => {}
                }
            }
            Ok(entries)
        }
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
