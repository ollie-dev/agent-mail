use std::io::Write;

use agent_mail::store::{GuardedStore, RuntimeStore};
use agent_mail::terminal::InquireTerminal;
use agent_mail::{Cli, CliCommand};

fn main() {
    let env = std::env::vars_os()
        .filter_map(|(k, v)| Some((k.into_string().ok()?, v.to_string_lossy().into_owned())))
        .collect();
    // Every store access first passes the hardened-runtime self-check
    // (a no-op outside macOS release builds), run lazily on first access.
    let store = GuardedStore::new(RuntimeStore::default());
    let terminal = InquireTerminal;
    let context = agent_mail::Context::new(&store).with_terminal(&terminal);
    // Binary tests must not write to the user's audit log.
    #[cfg(feature = "test-support")]
    let context = context.with_audit_log(agent_mail::audit::AuditLog::for_test_binary());
    let output = match agent_mail::parse(std::env::args_os()) {
        Err(output) => output,
        Ok(Cli {
            command: CliCommand::Mcp,
            ..
        }) => {
            // stdout carries only JSON-RPC lines; diagnostics go to stderr.
            let result = agent_mail::mcp::serve(
                std::io::stdin().lock(),
                std::io::stdout().lock(),
                &env,
                &context,
            );
            if let Err(e) = result {
                let _ = writeln!(std::io::stderr(), "agent-mail mcp: {e}");
                std::process::exit(1);
            }
            std::process::exit(0);
        }
        // The Broker builds its own store, audit log and network.
        Ok(Cli {
            command: CliCommand::Broker,
            text,
        }) => agent_mail::broker::run_service(text),
        Ok(cli) => agent_mail::run_cli(cli, &env, &context),
    };
    let _ = std::io::stdout().write_all(output.stdout.as_bytes());
    let _ = std::io::stderr().write_all(output.stderr.as_bytes());
    std::process::exit(output.code);
}
