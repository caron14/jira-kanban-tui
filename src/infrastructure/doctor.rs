use crate::infrastructure::{cli::Cli, config::Config, token};
use anyhow::Result;

pub async fn run(cli: &Cli) -> Result<()> {
    println!("jira-kanban-tui doctor");
    let path = Config::path(cli);
    println!("config: {}", path.display());
    let Some(config) = Config::load(cli)? else {
        anyhow::bail!("configuration not found; run jira-kanban-tui to complete the guided Setup");
    };
    println!("config: OK (v{})", config.version);

    let providers = token::build_providers(&config.jira);
    let (_, secret) = token::resolve_token(&providers)?.ok_or_else(|| {
        anyhow::anyhow!(
            "credential not found; open Setup or configure {}",
            config.jira.token_env.as_deref().unwrap_or("the OS keyring")
        )
    })?;
    println!("credential: OK");

    let service = crate::jira::JiraService::new(&config.jira, secret)?;
    let viewer = service.viewer().await?;
    println!("viewer: OK ({})", viewer.label);

    let mut failures = Vec::new();
    for board_ref in service.board_refs() {
        match service.load_board_and_issues(&board_ref).await {
            Ok((board, issues)) => {
                println!("board {board_ref}: OK ({}; {} issues)", board.name, issues.len())
            }
            Err(error) => {
                println!("board {board_ref}: ERROR {error}");
                failures.push(board_ref);
            }
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        anyhow::bail!(
            "{} Board(s) failed; verify each Board ID and Jira permission shown above",
            failures.len()
        )
    }
}
