//! Tests that drive the built `agent-mail` binary from the outside.

use std::process::Command;

#[test]
fn version_flag_prints_name_and_crate_version() {
    let output = Command::new(env!("CARGO_BIN_EXE_agent-mail"))
        .arg("--version")
        .output()
        .expect("failed to run agent-mail");

    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        format!("agent-mail {}\n", env!("CARGO_PKG_VERSION"))
    );
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
    assert_eq!(output.status.code(), Some(0));
}
