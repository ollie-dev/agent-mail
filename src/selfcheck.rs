//! The hardened-runtime self-check (ADR-0002).
//!
//! Keychain items trust only the agent-mail binary. That binding protects a
//! Mandate only if no other code can run inside the binary, which on macOS
//! needs the hardened runtime (it refuses `DYLD_INSERT_LIBRARIES` and
//! similar injection). A release binary therefore reads its own
//! code-signing status before any Keychain access and refuses the access if
//! the runtime flag is missing.
//!
//! The check is active only in macOS release builds
//! (`cfg(all(target_os = "macos", not(debug_assertions)))`); every other
//! build passes it. Nothing at runtime can switch it off: no environment
//! variable, argument, file or Cargo feature is consulted.

use std::sync::OnceLock;

/// `CS_OPS_STATUS`: the `csops` operation that reads the status flags.
pub const CS_OPS_STATUS: u32 = 0;
/// `CS_RUNTIME`: the process runs with the hardened runtime.
pub const CS_RUNTIME: u32 = 0x0001_0000;

/// Whether the code-signing `status` shows the hardened runtime. `None`
/// means the status could not be read, which fails.
pub fn hardened_runtime_ok(status: Option<u32>) -> bool {
    matches!(status, Some(flags) if flags & CS_RUNTIME != 0)
}

/// The running process's code-signing status flags, or `None` if they
/// cannot be read.
#[cfg(target_os = "macos")]
pub fn process_status() -> Option<u32> {
    unsafe extern "C" {
        // libSystem: int csops(pid_t pid, unsigned int ops, void *useraddr,
        // size_t usersize);
        fn csops(pid: libc::pid_t, ops: u32, useraddr: *mut libc::c_void, usersize: usize) -> i32;
    }
    let mut flags: u32 = 0;
    // SAFETY: `flags` is a live, writable u32 and its exact size is passed,
    // so the kernel writes at most those four bytes.
    let result = unsafe {
        csops(
            libc::getpid(),
            CS_OPS_STATUS,
            (&raw mut flags).cast::<libc::c_void>(),
            std::mem::size_of::<u32>(),
        )
    };
    (result == 0).then_some(flags)
}

/// The running process's code-signing status: none on this OS.
#[cfg(not(target_os = "macos"))]
pub fn process_status() -> Option<u32> {
    None
}

/// Whether this process may access the Keychain: computed once per process
/// and cached. In macOS release builds this is
/// [`hardened_runtime_ok`] of [`process_status`]; every other build passes.
pub fn check() -> bool {
    static RESULT: OnceLock<bool> = OnceLock::new();
    *RESULT.get_or_init(evaluate)
}

#[cfg(all(target_os = "macos", not(debug_assertions)))]
fn evaluate() -> bool {
    hardened_runtime_ok(process_status())
}

#[cfg(not(all(target_os = "macos", not(debug_assertions))))]
fn evaluate() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_flag_set_passes() {
        assert!(hardened_runtime_ok(Some(0x0001_0003)));
    }

    #[test]
    fn adhoc_linker_signed_without_runtime_fails() {
        assert!(!hardened_runtime_ok(Some(0x0002_0003)));
    }

    #[test]
    fn no_flags_fail() {
        assert!(!hardened_runtime_ok(Some(0)));
    }

    #[test]
    fn unreadable_status_fails() {
        assert!(!hardened_runtime_ok(None));
    }

    #[cfg(debug_assertions)]
    #[test]
    fn debug_builds_pass_whatever_the_status() {
        // The test binary is not signed with the hardened runtime.
        assert!(!hardened_runtime_ok(process_status()));
        assert!(check());
    }
}
