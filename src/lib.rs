//! Agent Mail library target.
//!
//! Holds the CLI definition now; the core and its adapters land here in later
//! tickets so integration tests can drive them in-process.

use clap::Parser;

/// Command-line interface of the `agent-mail` binary.
#[derive(Debug, Parser)]
#[command(name = "agent-mail", version, about, arg_required_else_help = true)]
pub struct Cli {}

/// Runs the binary with already parsed arguments.
pub fn run(_cli: Cli) {}
