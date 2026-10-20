use std::io::Write;

use agent_mail::store::RuntimeStore;

fn main() {
    let env = std::env::vars_os()
        .filter_map(|(k, v)| Some((k.into_string().ok()?, v.into_string().ok()?)))
        .collect();
    let store = RuntimeStore;
    let output = agent_mail::run(std::env::args_os(), &env, &agent_mail::Context::new(&store));
    let _ = std::io::stdout().write_all(output.stdout.as_bytes());
    let _ = std::io::stderr().write_all(output.stderr.as_bytes());
    std::process::exit(output.code);
}
