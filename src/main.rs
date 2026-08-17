use anyhow::Result;
use clap::Parser;
use jira_kanban_tui::infrastructure::{cli::Cli, config::Config, logging};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    logging::init(&cli);

    if cli.is_doctor() {
        return jira_kanban_tui::infrastructure::doctor::run(&cli).await;
    }

    let (config, config_error) = match Config::load(&cli) {
        Ok(config) => (config, None),
        Err(error) => (None, Some(error.to_string())),
    };
    jira_kanban_tui::app::run(cli, config, config_error).await
}
