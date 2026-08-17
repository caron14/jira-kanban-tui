use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "jira-kanban-tui", version, about = "Jira Kanban TUI")]
pub struct Cli {
    /// Override board ID from config
    #[arg(long)]
    pub board: Option<i64>,

    /// Path to config file
    #[arg(long)]
    pub config: Option<PathBuf>,

    /// Compatibility alias for `doctor`
    #[arg(long, default_value_t = false, hide = true)]
    pub doctor: bool,

    #[command(subcommand)]
    pub command: Option<Command>,

    /// Verbosity (-v, -vv)
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub verbose: u8,
}

#[derive(Subcommand, Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Diagnose configuration, credentials and Jira Board access
    Doctor,
}

impl Cli {
    pub fn is_doctor(&self) -> bool {
        self.doctor || self.command == Some(Command::Doctor)
    }
}
