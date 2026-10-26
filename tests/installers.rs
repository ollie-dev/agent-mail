//! The Windows installers for real: `install.ps1` against a `file://`
//! download directory holding the test-built `agent-mail.exe`, and
//! `uninstall.ps1`. They install to `%ProgramFiles%\agent-mail`, register
//! the Broker service and change the machine Path, so every test is opt-in
//! and needs an elevated shell:
//!
//!   cargo test --locked --test installers -- --ignored --test-threads=1
//!
//! Each test first checks that agent-mail is not installed, and a drop
//! guard removes the service, the install folder and the Path entry and
//! restores the machine Path, so a failed test leaves nothing behind.
//! All values are fake; the Credential is a placeholder.
#![cfg(windows)]

mod support;

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::Duration;

use agent_mail::broker::{self, windows as win};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde_json::{Value, json};
use support::{CREDENTIAL, FakeImap, LOGIN, Mode};

const ROOT: &str = env!("CARGO_MANIFEST_DIR");
const BIN: &str = env!("CARGO_BIN_EXE_agent-mail");
const SERVICE: &str = "agent-mail";
const ASSET: &str = "agent-mail-x86_64-pc-windows-msvc.zip";
const SERVICE_SDDL: &str = "D:(A;;CCLCSWRPWPDTLOCRRC;;;SY)(A;;CCDCLCSWRPWPDTLOCRSDRCWDWO;;;BA)(A;;CCLCSWLOCRRC;;;IU)(A;;CCLCSWLOCRRC;;;SU)";
const INSTALLED: &str =
    "agent-mail is installed on this machine; the installer tests would replace it";

// ---------------------------------------------------------------- helpers

/// Runs `script` in Windows PowerShell, passed encoded so no quoting is
/// lost, and returns its trimmed stdout. Panics if it fails.
fn ps(script: &str) -> String {
    let script = format!(
        "$ProgressPreference = 'SilentlyContinue'; $ErrorActionPreference = 'Stop'; {script}"
    );
    let utf16: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let out = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-EncodedCommand"])
        .arg(BASE64.encode(utf16))
        .output()
        .expect("powershell.exe runs");
    assert!(out.status.success(), "{script}: {out:?}");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// `s` as a single-quoted PowerShell string.
fn quoted(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// The PowerShell hosts the scripts run under: Windows PowerShell 5.1
/// always, PowerShell 7 too where it is installed.
fn shells() -> Vec<&'static str> {
    let mut shells = vec!["powershell.exe"];
    let pwsh = Command::new("pwsh.exe")
        .args(["-NoProfile", "-Command", "exit 0"])
        .output();
    if pwsh.is_ok_and(|o| o.status.success()) {
        shells.push("pwsh.exe");
    }
    shells
}

fn install_folder() -> PathBuf {
    PathBuf::from(ps("[Environment]::GetFolderPath('ProgramFiles')")).join("agent-mail")
}

fn installed_binary() -> PathBuf {
    install_folder().join("agent-mail.exe")
}

fn program_data_folder() -> PathBuf {
    PathBuf::from(ps("[Environment]::GetFolderPath('CommonApplicationData')")).join("agent-mail")
}

/// Runs `sc.exe` with `args`: whether it exited with 0, and its stdout.
fn sc(args: &[&str]) -> (bool, String) {
    let out = Command::new("sc.exe")
        .args(args)
        .output()
        .expect("sc.exe runs");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
}

fn service_exists() -> bool {
    sc(&["query", SERVICE]).0
}

/// Polls `sc query` until the service is in `state`; whether it got there.
fn wait_for_state(state: &str) -> bool {
    for _ in 0..300 {
        if sc(&["query", SERVICE]).1.contains(state) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

/// The raw machine Path, read with `DoNotExpandEnvironmentNames`, and its
/// value kind.
#[derive(Debug, Clone, PartialEq, Eq)]
struct MachinePath {
    raw: String,
    kind: String,
}

const ENV_KEY: &str = r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment";

fn machine_path() -> MachinePath {
    let out = ps(&format!(
        "$k = [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey({}); \
         $v = [string]$k.GetValue('Path', '', 'DoNotExpandEnvironmentNames'); \
         $k.GetValueKind('Path').ToString() + ' ' + [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($v))",
        quoted(ENV_KEY)
    ));
    let (kind, encoded) = out.split_once(' ').unwrap_or((out.as_str(), ""));
    let bytes = BASE64.decode(encoded).unwrap();
    let units: Vec<u16> = bytes
        .chunks(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    MachinePath {
        raw: String::from_utf16(&units).unwrap(),
        kind: kind.to_string(),
    }
}

fn set_machine_path(path: &MachinePath) {
    let utf16: Vec<u8> = path.raw.encode_utf16().flat_map(u16::to_le_bytes).collect();
    ps(&format!(
        "$k = [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey({}, $true); \
         $v = [Text.Encoding]::Unicode.GetString([Convert]::FromBase64String('{}')); \
         $k.SetValue('Path', $v, [Microsoft.Win32.RegistryValueKind]::{})",
        quoted(ENV_KEY),
        BASE64.encode(utf16),
        path.kind
    ));
}

/// Deletes `path` and everything under it, retrying while a process that
/// just ended still holds a file.
fn remove_retrying(path: &Path) {
    for _ in 0..50 {
        match std::fs::remove_dir_all(path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                std::thread::sleep(Duration::from_millis(200));
            }
            _ => return,
        }
    }
}

/// A machine with agent-mail not installed, put back as it was when
/// dropped: the service is purged, stopped and deleted, the install folder
/// deleted and the machine Path restored.
struct CleanMachine {
    path: MachinePath,
    folder: PathBuf,
}

impl CleanMachine {
    fn new() -> Self {
        let folder = install_folder();
        if service_exists() || folder.exists() {
            panic!("{INSTALLED}");
        }
        Self {
            path: machine_path(),
            folder,
        }
    }
}

impl Drop for CleanMachine {
    fn drop(&mut self) {
        if service_exists() {
            let _ = sc(&["stop", SERVICE]);
            wait_for_state("STOPPED");
            // Deletes the Mandates the test stored and the audit folder.
            if self.folder.join("agent-mail.exe").exists() && sc(&["start", SERVICE, "purge"]).0 {
                wait_for_state("STOPPED");
            }
            let _ = sc(&["delete", SERVICE]);
        }
        remove_retrying(&self.folder);
        if machine_path() != self.path {
            set_machine_path(&self.path);
        }
    }
}

/// The fake IMAP server's test CA in `Cert:\LocalMachine\Root`, so the
/// Broker's platform verifier trusts it; removed again when dropped.
struct TrustedCa(String);

impl TrustedCa {
    fn add(server: &FakeImap) -> Self {
        let dir = temp_dir("ca");
        let file = dir.join("test-ca.cer");
        std::fs::write(&file, server.ca.as_ref()).unwrap();
        let thumbprint = ps(&format!(
            "$c = New-Object Security.Cryptography.X509Certificates.X509Certificate2({}); \
             $s = New-Object Security.Cryptography.X509Certificates.X509Store('Root', 'LocalMachine'); \
             $s.Open('ReadWrite'); $s.Add($c); $s.Close(); $c.Thumbprint",
            quoted(&file.display().to_string())
        ));
        assert_eq!(thumbprint.len(), 40, "{thumbprint}");
        Self(thumbprint)
    }
}

impl Drop for TrustedCa {
    fn drop(&mut self) {
        ps(&format!(
            "$s = New-Object Security.Cryptography.X509Certificates.X509Store('Root', 'LocalMachine'); \
             $s.Open('ReadWrite'); \
             $s.RemoveRange($s.Certificates.Find('FindByThumbprint', {}, $false)); $s.Close()",
            quoted(&self.0)
        ));
    }
}

/// A child process killed when dropped.
struct Running(Child);

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// A fresh, empty temporary directory for the test's own files.
fn temp_dir(label: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "agent-mail-installers-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The SHA-256 of `path`, lower-case hex.
fn sha256(path: &Path) -> String {
    ps(&format!(
        "(Get-FileHash -Algorithm SHA256 -LiteralPath {}).Hash.ToLowerInvariant()",
        quoted(&path.display().to_string())
    ))
}

/// A download directory holding a zip of `files` (name, bytes) and its
/// correct `.sha256`.
fn downloads_with(files: &[(&str, &[u8])]) -> PathBuf {
    let dir = temp_dir("downloads");
    let stage = temp_dir("stage");
    for (name, bytes) in files {
        std::fs::write(stage.join(name), bytes).unwrap();
    }
    let zip = dir.join(ASSET);
    ps(&format!(
        "Compress-Archive -Path {} -DestinationPath {}",
        quoted(&format!("{}\\*", stage.display())),
        quoted(&zip.display().to_string())
    ));
    let digest = sha256(&zip);
    assert_eq!(digest.len(), 64, "{digest}");
    std::fs::write(
        dir.join(format!("{ASSET}.sha256")),
        format!("{digest}  {ASSET}\n"),
    )
    .unwrap();
    dir
}

/// A download directory with the real test-built binary.
fn real_downloads() -> PathBuf {
    downloads_with(&[("agent-mail.exe", &std::fs::read(BIN).unwrap())])
}

/// A download directory with a binary that differs from the real one: the
/// same exe with bytes appended, which still runs.
fn other_downloads() -> (PathBuf, Vec<u8>) {
    let mut bytes = std::fs::read(BIN).unwrap();
    bytes.extend_from_slice(b"\0agent-mail installer test: appended bytes\0");
    (downloads_with(&[("agent-mail.exe", &bytes)]), bytes)
}

fn file_url(dir: &Path) -> String {
    format!("file:///{}", dir.display().to_string().replace('\\', "/"))
}

/// Runs `script` from the repository under `shell` with `args`, as an
/// operator runs it from a file.
fn run_script(shell: &str, script: &str, args: &[&str], envs: &[(&str, &str)]) -> Output {
    let mut command = Command::new(shell);
    command
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ])
        .arg(Path::new(ROOT).join(script))
        .args(args)
        .stdin(Stdio::null());
    for (name, value) in envs {
        command.env(name, value);
    }
    command.output().expect("the shell runs")
}

fn install(shell: &str, downloads: &Path) -> Output {
    run_script(
        shell,
        "install.ps1",
        &["-DownloadUrl", &file_url(downloads)],
        &[],
    )
}

fn uninstall(shell: &str, args: &[&str]) -> Output {
    run_script(shell, "uninstall.ps1", args, &[])
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn assert_ok(shell: &str, out: &Output) {
    assert_eq!(out.status.code(), Some(0), "{shell}: {out:?}");
}

/// Runs the installed binary with `args` and no token, as the test user.
fn agent_mail(args: &[&str]) -> Output {
    Command::new(installed_binary())
        .args(args)
        .env_remove("AGENT_MAIL_TOKEN")
        .env_remove(agent_mail::client::TEST_PIPE_VAR)
        .env_remove(agent_mail::audit::TEST_LOG_VAR)
        .stdin(Stdio::null())
        .output()
        .expect("the installed binary runs")
}

/// The IDs `mandate list` from the installed binary shows, retrying while
/// the Broker is still starting.
fn mandate_ids() -> Vec<String> {
    let mut last = None;
    for _ in 0..50 {
        let out = agent_mail(&["mandate", "list"]);
        if out.status.success() {
            let value: Value = serde_json::from_slice(&out.stdout).unwrap();
            return value["mandates"]
                .as_array()
                .unwrap()
                .iter()
                .map(|m| m["id"].as_str().unwrap().to_string())
                .collect();
        }
        last = Some(out);
        std::thread::sleep(Duration::from_millis(200));
    }
    panic!("mandate list failed: {last:?}");
}

/// The version the test binary prints, `x.y.z`.
fn binary_version() -> String {
    let out = Command::new(BIN).arg("--version").output().unwrap();
    let line = stdout(&out);
    line.trim()
        .strip_prefix("agent-mail ")
        .unwrap_or_else(|| panic!("unexpected --version output {line:?}"))
        .to_string()
}

/// The Broker service as `Win32_Service` describes it.
fn service_config() -> Value {
    let json = ps(
        "Get-CimInstance Win32_Service -Filter \"Name='agent-mail'\" | \
         Select-Object StartName, StartMode, DelayedAutoStart, State, PathName, ProcessId | ConvertTo-Json",
    );
    serde_json::from_str(&json).unwrap_or_else(|e| panic!("{e}: {json}"))
}

/// A connection to the installed Broker's pipe, checked by the client's
/// own connector to be served by `NT SERVICE\agent-mail`, past `hello`.
fn broker_session() -> std::fs::File {
    let connector = win::PipeConnector::broker();
    let mut pipe = None;
    for _ in 0..100 {
        match connector.open() {
            Ok(file) => {
                pipe = Some(file);
                break;
            }
            Err(_) => std::thread::sleep(Duration::from_millis(100)),
        }
    }
    let mut pipe = pipe.expect("the installed Broker's pipe accepted no client");
    let reply = call(&mut pipe, &json!({"hello": {"version": broker::VERSION}}));
    assert_eq!(reply["ok"]["version"], broker::VERSION, "{reply}");
    pipe
}

fn call(pipe: &mut std::fs::File, request: &Value) -> Value {
    broker::write_frame(pipe, &serde_json::to_vec(request).unwrap()).unwrap();
    let body = broker::read_frame(pipe).unwrap().expect("a reply");
    serde_json::from_slice(&body).unwrap()
}

/// Stores a Mandate for the fake Account on `server` through the installed
/// Broker, as an elevated `mandate create` does, and returns its ID.
fn store_mandate(server: &FakeImap) -> String {
    let mut pipe = broker_session();
    let reply = call(
        &mut pipe,
        &json!({"create": {
            "name": "installer-test",
            "accounts": [{"login": LOGIN, "host": "localhost", "port": server.port, "tls": "implicit", "credential": CREDENTIAL}],
            "grants": [{"operation": "list", "account": LOGIN, "mailboxes": ["INBOX"]}],
        }}),
    );
    reply["result"]["id"]
        .as_str()
        .unwrap_or_else(|| panic!("create failed: {reply}"))
        .to_string()
}

fn same_folder(entry: &str, folder: &Path) -> bool {
    entry
        .trim_end_matches('\\')
        .eq_ignore_ascii_case(folder.display().to_string().trim_end_matches('\\'))
}

/// The machine Path after an install: `before` with the install folder
/// appended once, every earlier entry unchanged, the value kind kept.
fn assert_path_installed(before: &MachinePath) {
    let folder = install_folder();
    let after = machine_path();
    assert_eq!(after.kind, before.kind, "value kind changed");
    assert_eq!(after.kind, "ExpandString");
    let expected = if before.raw.is_empty() {
        folder.display().to_string()
    } else {
        format!("{};{}", before.raw, folder.display())
    };
    assert_eq!(after.raw, expected);
    let entries: Vec<&str> = after.raw.split(';').collect();
    assert!(same_folder(entries.last().unwrap(), &folder));
    assert_eq!(
        entries.iter().filter(|e| same_folder(e, &folder)).count(),
        1,
        "{}",
        after.raw
    );
}

/// Rights on a folder that let a trustee change what it holds or who may
/// use it.
const WRITE_RIGHTS: u32 =
    0x2 | 0x4 | 0x10 | 0x40 | 0x100 | 0x1_0000 | 0x4_0000 | 0x8_0000 | 0x1000_0000 | 0x4000_0000;

/// `BUILTIN\Users`, `Authenticated Users`, `Everyone`, `INTERACTIVE`.
const UNPRIVILEGED: [&str; 4] = ["S-1-5-32-545", "S-1-5-11", "S-1-1-0", "S-1-5-4"];

fn assert_folder_admin_only() {
    let lines = ps(&format!(
        "$acl = Get-Acl -LiteralPath {}; \
         $acl.GetAccessRules($true, $true, [Security.Principal.SecurityIdentifier]) | \
         ForEach-Object {{ '{{0}} {{1}} {{2}}' -f $_.IdentityReference.Value, [int64]$_.FileSystemRights, $_.AccessControlType }}",
        quoted(&install_folder().display().to_string())
    ));
    assert!(!lines.is_empty());
    for line in lines.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        let [sid, rights, kind] = parts[..] else {
            panic!("unexpected rule {line}");
        };
        let rights = rights.parse::<i64>().unwrap() as u32;
        if kind == "Allow" && UNPRIVILEGED.contains(&sid) {
            assert_eq!(rights & WRITE_RIGHTS, 0, "{sid} may write: {line}");
        }
    }
}

/// The Broker as AC1 describes it: running under `NT SERVICE\agent-mail`
/// with delayed automatic start, the quoted path, the explicit access list.
fn assert_service_configured() {
    let config = service_config();
    assert_eq!(config["StartName"], r"NT SERVICE\agent-mail", "{config}");
    assert_eq!(config["StartMode"], "Auto", "{config}");
    assert_eq!(config["DelayedAutoStart"], true, "{config}");
    assert_eq!(config["State"], "Running", "{config}");
    assert_eq!(
        config["PathName"],
        format!("\"{}\" broker", installed_binary().display()),
        "{config}"
    );
    let (ok, shown) = sc(&["sdshow", SERVICE]);
    assert!(ok, "{shown}");
    // `sdshow` may append the audit part (`S:`); the access list is the D: part.
    let dacl = shown.trim().split("S:").next().unwrap();
    assert_eq!(dacl, SERVICE_SDDL);
}

fn folder_files() -> Vec<String> {
    let mut files: Vec<String> = std::fs::read_dir(install_folder())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    files.sort();
    files
}

/// Nothing is installed: no service, no install folder, the Path as
/// `before`.
fn assert_nothing_installed(shell: &str, before: &MachinePath) {
    assert!(!service_exists(), "{shell}: the service is registered");
    assert!(
        !install_folder().exists(),
        "{shell}: the install folder exists"
    );
    assert_eq!(&machine_path(), before, "{shell}: the machine Path changed");
}

// ---------------------------------------------------------------- AC1

#[test]
#[ignore = "needs an elevated shell; installs agent-mail and its Broker for real"]
fn job027_ac1_install_registers_and_starts_the_broker() {
    let downloads = real_downloads();
    let version = binary_version();
    for shell in shells() {
        let machine = CleanMachine::new();
        let out = install(shell, &downloads);
        assert_ok(shell, &out);
        assert_eq!(folder_files(), ["agent-mail.exe"], "{shell}");
        assert_eq!(sha256(&installed_binary()), sha256(Path::new(BIN)));
        assert_service_configured();
        assert_path_installed(&machine.path);
        assert_folder_admin_only();
        let whoami = agent_mail(&["whoami"]);
        let said = format!("{}{}", stdout(&whoami), stderr(&whoami));
        assert!(!said.trim().is_empty(), "{shell}: {whoami:?}");
        assert!(!said.contains("broker_unavailable"), "{shell}: {said}");
        assert!(!said.contains("broker_version_mismatch"), "{shell}: {said}");
        assert!(
            stdout(&out).contains(&format!("Installed agent-mail {version}")),
            "{shell}: {}",
            stdout(&out)
        );
        drop(machine);
    }
}

#[test]
#[ignore = "needs an elevated shell; installs agent-mail and its Broker for real"]
fn job027_ac1_failed_verification_installs_nothing() {
    let fake: &[u8] = b"MZ fake agent-mail.exe for installer tests";
    for shell in shells() {
        let machine = CleanMachine::new();
        let refused = |out: &Output, message: &str| {
            assert_eq!(out.status.code(), Some(1), "{shell}: {out:?}");
            assert!(stderr(out).contains(message), "{shell}: {}", stderr(out));
            assert_nothing_installed(shell, &machine.path);
        };

        let downloads = downloads_with(&[("agent-mail.exe", fake)]);
        std::fs::write(
            downloads.join(format!("{ASSET}.sha256")),
            format!("{}  {ASSET}\n", "0".repeat(64)),
        )
        .unwrap();
        refused(
            &install(shell, &downloads),
            "checksum mismatch for agent-mail-x86_64-pc-windows-msvc.zip; nothing was installed",
        );

        let digest = "a".repeat(64);
        for malformed in [
            format!("{digest}  {ASSET}.sha256\n"),
            format!("{digest} {ASSET}\n"),
            format!("{}  {ASSET}\n", "A".repeat(64)),
            format!("{digest}  {ASSET}\r\n"),
            String::new(),
        ] {
            let downloads = downloads_with(&[("agent-mail.exe", fake)]);
            std::fs::write(downloads.join(format!("{ASSET}.sha256")), &malformed).unwrap();
            refused(
                &install(shell, &downloads),
                &format!("checksum file for {ASSET} is malformed; nothing was installed"),
            );
        }

        let downloads = downloads_with(&[("agent-mail.exe", fake)]);
        std::fs::remove_file(downloads.join(ASSET)).unwrap();
        let out = install(shell, &downloads);
        refused(&out, "could not download ");
        assert!(
            stderr(&out).contains("; nothing was installed"),
            "{}",
            stderr(&out)
        );

        for files in [
            &[("other.exe", fake)][..],
            &[("agent-mail.exe", fake), ("extra.dll", b"x")][..],
        ] {
            refused(
                &install(shell, &downloads_with(files)),
                &format!("{ASSET} must hold exactly agent-mail.exe; nothing was installed"),
            );
        }

        let out = run_script(
            shell,
            "install.ps1",
            &["-DownloadUrl", &file_url(&real_downloads())],
            &[("PROCESSOR_ARCHITECTURE", "ARM64")],
        );
        refused(&out, "x64");
        drop(machine);
    }
}

// ---------------------------------------------------------------- AC2

#[test]
#[ignore = "needs an elevated shell; installs agent-mail and its Broker for real"]
fn job027_ac2_rerun_updates_in_place_and_keeps_the_mandates() {
    let server = FakeImap::start(Mode::Implicit);
    let _ca = TrustedCa::add(&server);
    let downloads = real_downloads();
    let (other, other_bytes) = other_downloads();
    let other_exe = temp_dir("other").join("agent-mail.exe");
    std::fs::write(&other_exe, &other_bytes).unwrap();
    for shell in shells() {
        let machine = CleanMachine::new();
        assert_ok(shell, &install(shell, &downloads));
        let id = store_mandate(&server);

        assert_ok(shell, &install(shell, &other));
        assert_eq!(sha256(&installed_binary()), sha256(&other_exe), "{shell}");
        assert_eq!(folder_files(), ["agent-mail.exe"], "{shell}");
        assert_service_configured();
        assert_path_installed(&machine.path);
        assert_eq!(mandate_ids(), std::slice::from_ref(&id), "{shell}");

        // An MCP server started from the binary keeps running meanwhile.
        let mcp = Running(
            Command::new(installed_binary())
                .arg("mcp")
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        );
        std::thread::sleep(Duration::from_millis(500));
        assert_ok(shell, &install(shell, &downloads));
        assert_eq!(
            sha256(&installed_binary()),
            sha256(Path::new(BIN)),
            "{shell}"
        );
        assert_service_configured();
        assert_eq!(mandate_ids(), std::slice::from_ref(&id), "{shell}");
        drop(mcp);
        drop(machine);
    }
}

#[test]
#[ignore = "needs an elevated shell; installs agent-mail and its Broker for real"]
fn job027_ac2_a_binary_that_does_not_run_restores_the_previous_version() {
    let downloads = real_downloads();
    let broken = downloads_with(&[("agent-mail.exe", b"not an executable at all")]);
    for shell in shells() {
        let machine = CleanMachine::new();
        assert_ok(shell, &install(shell, &downloads));
        let before = sha256(&installed_binary());

        let out = install(shell, &broken);
        assert_eq!(out.status.code(), Some(1), "{shell}: {out:?}");
        assert!(
            stderr(&out).contains(
                "agent-mail installer: the new binary did not start; the previous version was restored"
            ),
            "{shell}: {}",
            stderr(&out)
        );
        assert_eq!(sha256(&installed_binary()), before, "{shell}");
        assert_eq!(service_config()["State"], "Running", "{shell}");
        assert_path_installed(&machine.path);
        drop(machine);
    }
}

// ---------------------------------------------------------------- AC3

#[test]
#[ignore = "needs an elevated shell; installs agent-mail and its Broker for real"]
fn job027_ac3_uninstall_without_an_install_changes_nothing() {
    for shell in shells() {
        let machine = CleanMachine::new();
        let out = uninstall(shell, &["-Force"]);
        assert_ok(shell, &out);
        assert_eq!(
            stdout(&out).trim(),
            "agent-mail is not installed.",
            "{shell}"
        );
        assert_nothing_installed(shell, &machine.path);
        drop(machine);
    }
}

#[test]
#[ignore = "needs an elevated shell; installs agent-mail and its Broker for real"]
fn job027_ac3_uninstall_without_a_confirmation_or_switch_changes_nothing() {
    let downloads = real_downloads();
    for shell in shells() {
        let machine = CleanMachine::new();
        assert_ok(shell, &install(shell, &downloads));
        let out = uninstall(shell, &[]);
        assert_eq!(out.status.code(), Some(1), "{shell}: {out:?}");
        assert!(
            stderr(&out).contains(
                "uninstall.ps1: cannot ask for confirmation; rerun with -KeepData or -Force"
            ),
            "{shell}: {}",
            stderr(&out)
        );
        assert!(service_exists(), "{shell}");
        assert_eq!(service_config()["State"], "Running", "{shell}");
        assert!(installed_binary().exists(), "{shell}");
        assert_path_installed(&machine.path);
        drop(machine);
    }
}

// ---------------------------------------------------------------- AC6

#[test]
#[ignore = "needs an elevated shell; installs agent-mail and its Broker for real"]
fn job027_ac6_broker_runs_as_its_virtual_account_and_keeps_mandates_over_a_restart() {
    let server = FakeImap::start(Mode::Implicit);
    let _ca = TrustedCa::add(&server);
    let downloads = real_downloads();
    let sid = broker::service_sid(SERVICE);
    for shell in shells() {
        let machine = CleanMachine::new();
        assert_ok(shell, &install(shell, &downloads));
        let pid = service_config()["ProcessId"].as_u64().unwrap() as u32;
        // Read from the process token; if a service token's access list
        // keeps even an administrator out, WMI (running as SYSTEM) reads it.
        let user = win::process_user_sid(pid).unwrap_or_else(|_| {
            ps(&format!(
                "(Invoke-CimMethod -InputObject (Get-CimInstance Win32_Process -Filter 'ProcessId={pid}') -MethodName GetOwnerSid).Sid"
            ))
        });
        assert_eq!(user, sid, "{shell}");
        let pipe = win::connect(broker::PIPE_NAME).unwrap();
        assert_eq!(win::pipe_owner(&pipe).unwrap(), sid, "{shell}");
        drop(pipe);

        let id = store_mandate(&server);
        assert!(sc(&["stop", SERVICE]).0);
        assert!(wait_for_state("STOPPED"));
        assert!(sc(&["start", SERVICE]).0);
        assert!(wait_for_state("RUNNING"));
        assert_eq!(mandate_ids(), [id], "{shell}");
        drop(machine);
    }
}

#[test]
#[ignore = "needs an elevated shell; installs agent-mail and its Broker for real"]
fn job027_ac6_uninstall_with_force_leaves_nothing_behind() {
    let server = FakeImap::start(Mode::Implicit);
    let _ca = TrustedCa::add(&server);
    let downloads = real_downloads();
    for shell in shells() {
        let machine = CleanMachine::new();
        assert_ok(shell, &install(shell, &downloads));
        store_mandate(&server);
        // An audited request, so the Broker writes its audit log.
        agent_mail(&["list"]);
        assert!(program_data_folder().join("audit.log").exists(), "{shell}");

        let out = uninstall(shell, &["-Force"]);
        assert_ok(shell, &out);
        assert!(
            stdout(&out).trim_end().ends_with("agent-mail was removed."),
            "{shell}: {}",
            stdout(&out)
        );
        assert!(!service_exists(), "{shell}");
        assert!(!install_folder().exists(), "{shell}");
        assert_eq!(machine_path(), machine.path, "{shell}: Path not restored");
        assert!(!program_data_folder().exists(), "{shell}");

        assert_ok(shell, &install(shell, &downloads));
        assert!(mandate_ids().is_empty(), "{shell}");
        drop(machine);
    }
}

#[test]
#[ignore = "needs an elevated shell; installs agent-mail and its Broker for real"]
fn job027_ac6_uninstall_with_keep_data_keeps_the_mandates_and_the_audit_log() {
    let server = FakeImap::start(Mode::Implicit);
    let _ca = TrustedCa::add(&server);
    let downloads = real_downloads();
    for shell in shells() {
        let machine = CleanMachine::new();
        assert_ok(shell, &install(shell, &downloads));
        let id = store_mandate(&server);
        agent_mail(&["list"]);
        let log = program_data_folder().join("audit.log");
        assert!(log.exists(), "{shell}");

        // -KeepData wins over -Force.
        let out = uninstall(shell, &["-KeepData", "-Force"]);
        assert_ok(shell, &out);
        assert!(
            stdout(&out).trim_end().ends_with("agent-mail was removed."),
            "{shell}: {}",
            stdout(&out)
        );
        assert!(!service_exists(), "{shell}");
        assert!(!install_folder().exists(), "{shell}");
        assert_eq!(machine_path(), machine.path, "{shell}: Path not restored");
        assert!(log.exists(), "{shell}: the audit log was deleted");

        assert_ok(shell, &install(shell, &downloads));
        assert!(mandate_ids().contains(&id), "{shell}");
        let out = uninstall(shell, &["-Force"]);
        assert_ok(shell, &out);
        assert!(!program_data_folder().exists(), "{shell}");
        drop(machine);
    }
}
