use std::io::Write;

use agent_mail::store::RuntimeStore;
use agent_mail::terminal::InquireTerminal;

fn main() {
    let env = std::env::vars_os()
        .filter_map(|(k, v)| Some((k.into_string().ok()?, v.to_string_lossy().into_owned())))
        .collect();
    let store = RuntimeStore::default();
    let terminal = InquireTerminal;
    let context = agent_mail::Context::new(&store).with_terminal(&terminal);
    // Binary tests must not write to the user's audit log.
    #[cfg(feature = "test-support")]
    let context = context.with_audit_log(agent_mail::audit::AuditLog::for_test_binary());
    let output = agent_mail::run(std::env::args_os(), &env, &context);
    let _ = std::io::stdout().write_all(output.stdout.as_bytes());
    let _ = std::io::stderr().write_all(output.stderr.as_bytes());
    std::process::exit(output.code);
}
