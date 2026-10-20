use clap::Parser;

fn main() {
    agent_mail::run(agent_mail::Cli::parse());
}
